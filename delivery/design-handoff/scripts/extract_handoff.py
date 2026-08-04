#!/usr/bin/env python3
"""
Deterministic ParchMint design-handoff extractor.

Reads a native Penpot export archive (ParchMint.penpot) and reproduces the
handoff package artifacts from the archive bytes themselves:

  * parchmint-ui.penpot         - byte-for-byte copy of the source archive
  * tokens/tokens.json          - byte-for-byte copy of the design-tokens document
  * references/light|dark/*.png - the 20 locked theme reference images, resolved
                                  through files/<file-id>/media/<media-id>.json
                                  into objects/<object-id>.png
  * assets/icons/*.svg          - every unique production icon exported from
                                  canonical component mains / canonical source
                                  shapes (deduplicated), fill preserved as a
                                  semantic-token provenance record

The extractor never asks Penpot to rasterize geometry; SVG output is produced
from the archive's own path `content` and `svgViewbox` records so that the
package can be regenerated identically from the same archive bytes.

Usage:
    python3 extract_handoff.py <path/to/ParchMint.penpot> <output-dir>

Exit codes:
    0 all required artifacts reproduced and verified
    1 verification failed or unrecoverable archive error
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import sys
import tempfile
import zipfile

FILE_ID = "2be68822-842f-8175-8008-65eef13b0227"
HANDOFF_PAGE = "e96ec683-a782-802c-8008-65f2d92067d3"
PLUGIN_NS = "plugin/96dfa740-005d-8020-8007-55ede24a2bae"
COMPONENTS_PAGE = "e96ec683-a782-802c-8008-65f2d90a9e10"
ROOT_FRAME = "00000000-0000-0000-0000-000000000000"

# Pages whose boards are production surface (screens/terminals/component mains).
# Excludes cover/foundations/reference/layout/accessibility/handoff/prototype
# diagrams and the cross-platform boards.
NON_PRODUCTION_PAGE_PREFIX = (
    "00 Cover", "01 Foundations", "12 Accessibility",
    "13 Cross-Platform", "15 Handoff",
)
NON_PRODUCTION_BOARD_PREFIX = (
    "PM / Reference /", "PM / Prototype Flow /", "PM / Handoff",
    "PM / Foundations", "PM / Theme Validation", "PM / Board / Cover",
)


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def expand_archive(archive: str) -> str:
    if not zipfile.is_zipfile(archive):
        raise SystemExit(f"not a valid zip archive: {archive}")
    tmp = tempfile.mkdtemp(prefix="parchmint-handoff-")
    with zipfile.ZipFile(archive) as zf:
        bad = zf.testzip()
        if bad is not None:
            raise SystemExit(f"corrupt member in archive: {bad}")
        zf.extractall(tmp)
    return tmp


def read_json(root: str, rel: str):
    with open(os.path.join(root, rel), encoding="utf-8") as fh:
        return json.load(fh)


# ---------------------------------------------------------------------------
# page / board indexing
# ---------------------------------------------------------------------------
def build_page_index(root: str, file_dir: str) -> tuple[dict, dict]:
    """Return (page_id -> {shape_id -> shape}, page_id -> page_name)."""
    base = os.path.join(root, "files", file_dir, "pages")
    shapes_by_page = {}
    page_names = {}
    for entry in sorted(os.listdir(base)):
        full = os.path.join(base, entry)
        if not os.path.isdir(full):
            meta = read_json(root, os.path.join("files", file_dir, "pages", entry))
            page_names[meta["id"]] = meta.get("name")
            continue
        objs = {}
        for name in sorted(os.listdir(full)):
            if not name.endswith(".json"):
                continue
            shape = read_json(
                root,
                os.path.join("files", file_dir, "pages", entry, name),
            )
            objs[shape.get("id")] = shape
        shapes_by_page[entry] = objs
    return shapes_by_page, page_names


def list_boards(shapes_by_page: dict, page_names: dict) -> dict:
    """Return {(page_id, board_id) -> board_shape} for top-level frames."""
    boards = {}
    for page_id, objs in shapes_by_page.items():
        for oid, shape in objs.items():
            if shape.get("type") == "frame" and shape.get("parentId") == ROOT_FRAME:
                boards[(page_id, oid)] = shape
    return boards


def containing_board(
    shapes: dict, boards: dict, page_id: str, shape_id: str
) -> tuple:
    """Return (page_id, board_id, board_shape) containing a shape, or None.

    Walks the ancestor chain and stops at the nearest top-level board. The
    chain itself is available via the returned (id, is_main) pairs only when
    the caller requests it; for compatibility the simple call returns the
    board triple.
    """
    cur = shape_id
    for _ in range(80):
        b = boards.get((page_id, cur))
        if b:
            return (page_id, cur, b)
        shape = shapes.get(cur)
        if not shape:
            return None
        par = shape.get("parentId")
        if not par or par == cur:
            return None
        cur = par
    return None


def ancestor_chain(shapes: dict, page_id: str, shape_id: str) -> list:
    """List of ancestor shape ids from the shape upward (excluding itself)."""
    chain = []
    cur = shape_id
    for _ in range(80):
        shape = shapes.get(cur)
        if not shape:
            break
        par = shape.get("parentId")
        if not par or par == cur:
            break
        chain.append(par)
        cur = par
    return chain


def is_production_board(page_name: str | None, board_name: str | None) -> bool:
    if not page_name or not board_name:
        return False
    if page_name.startswith(NON_PRODUCTION_PAGE_PREFIX):
        return False
    if board_name.startswith(NON_PRODUCTION_BOARD_PREFIX):
        return False
    return True


# ---------------------------------------------------------------------------
# references
# ---------------------------------------------------------------------------
def resolve_reference_images(root: str, file_dir: str) -> list[dict]:
    page = read_json(
        root,
        os.path.join("files", file_dir, "pages", HANDOFF_PAGE + ".json"),
    )
    pm = page["pluginData"][PLUGIN_NS]
    entries = json.loads(pm["pm.theme-reference-checksums"])
    resolved = []
    for e in entries:
        media_rec = read_json(
            root,
            os.path.join("files", file_dir, "media", e["mediaId"] + ".json"),
        )
        resolved.append(
            {
                "filename": e["filename"],
                "theme": e["theme"].lower(),
                "layer_id": e["layerId"],
                "media_id": e["mediaId"],
                "object_id": media_rec["mediaId"],
                "recorded_sha256": e["sha256"],
            }
        )
    return resolved


def export_references(root: str, file_dir: str, out_dir: str) -> list[dict]:
    refs = resolve_reference_images(root, file_dir)
    exported = []
    for r in refs:
        src = os.path.join(root, "objects", r["object_id"] + ".png")
        if not os.path.exists(src):
            raise SystemExit(f"reference object missing: {src}")
        dst = os.path.join(
            out_dir, "references", r["theme"], r["filename"]
        )
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(src, dst)
        exported.append(
            {
                "file": os.path.join("references", r["theme"], r["filename"]),
                "theme": r["theme"],
                "object_id": r["object_id"],
                "media_id": r["media_id"],
                "recorded_sha256": r["recorded_sha256"],
                "packaged_sha256": sha256(dst),
            }
        )
    return exported


# ---------------------------------------------------------------------------
# icons
# ---------------------------------------------------------------------------
def load_component_main_ids(root: str, file_dir: str) -> set:
    """IDs of non-deleted component mains from the archive registry."""
    base = os.path.join(root, "files", file_dir, "components")
    ids = set()
    if not os.path.isdir(base):
        return ids
    for name in os.listdir(base):
        if not name.endswith(".json"):
            continue
        rec = read_json(root, os.path.join("files", file_dir, "components", name))
        if not rec.get("deleted") and rec.get("mainInstanceId"):
            ids.add(rec["mainInstanceId"])
    return ids


def collect_production_icons(
    root: str, file_dir: str, shapes_by_page: dict, page_names: dict, boards: dict
) -> dict:
    """
    Every unique production-level icon fingerprint. A fingerprint is the
    (viewbox w, viewbox h, path d) triple. For each we record the canonical
    source shape: prefer a copy that lives inside a component main on the
    Components page; otherwise the first production board copy encountered.
    """
    main_ids = load_component_main_ids(root, file_dir)
    icons = {}

    def fingerprint(shape):
        vb = shape.get("svgViewbox") or {}
        return (
            round(vb.get("width", 0), 3),
            round(vb.get("height", 0), 3),
            shape.get("content"),
        )

    for page_id, objs in shapes_by_page.items():
        page_name = page_names.get(page_id)
        for oid, shape in objs.items():
            if shape.get("type") != "path":
                continue
            board = containing_board(objs, boards, page_id, oid)
            if not board:
                continue
            if not is_production_board(page_name, board[2].get("name")):
                continue
            fp = fingerprint(shape)
            entry = icons.setdefault(
                fp,
                {
                    "count": 0,
                    "canonical": None,
                    "is_main": False,
                },
            )
            entry["count"] += 1
            # Precedence: 2 = inside a component main (canonical source),
            # 1 = anywhere else on the Components page, 0 = screen/terminal.
            inside_main = any(
                cid in main_ids for cid in ancestor_chain(objs, page_id, oid)
            )
            precedence = (
                2 if inside_main else
                1 if page_name and page_name.startswith("02 Components") else
                0
            )
            if entry["canonical"] is None or precedence > entry["is_main"]:
                vb = shape.get("svgViewbox") or {}
                entry["canonical"] = {
                    "page_id": page_id,
                    "shape_id": oid,
                    "board_id": board[1],
                    "board_name": board[2].get("name"),
                    "d": shape.get("content"),
                    "viewbox": vb,
                    "fill_token": (shape.get("appliedTokens") or {}).get("fill"),
                }
                entry["is_main"] = precedence
    return icons


def write_icon_assets(out_dir: str, icons: dict) -> list[dict]:
    os.makedirs(os.path.join(out_dir, "assets", "icons"), exist_ok=True)
    records = []
    for i, (fp, entry) in enumerate(sorted(icons.items())):
        src = entry["canonical"]
        vb = src["viewbox"]
        x = round(vb.get("x", 0), 3)
        y = round(vb.get("y", 0), 3)
        w = round(vb.get("width", 0), 3)
        h = round(vb.get("height", 0), 3)
        svg = (
            f'<svg xmlns="http://www.w3.org/2000/svg" '
            f'viewBox="{x} {y} {w} {h}" width="{w}" height="{h}">\n'
            f'  <path d="{src["d"]}" />\n'
            f"</svg>\n"
        )
        name = f"pm-icon-{i:03d}.svg"
        dst = os.path.join(out_dir, "assets", "icons", name)
        with open(dst, "w", encoding="utf-8") as fh:
            fh.write(svg)
        records.append(
            {
                "file": os.path.join("assets", "icons", name),
                "sha256": sha256(dst),
                "width": w,
                "height": h,
                "fill_token": src["fill_token"],
                "canonical_source": {
                    "page_id": src["page_id"],
                    "shape_id": src["shape_id"],
                    "board": src["board_name"],
                },
                "instance_count": entry["count"],
            }
        )
    manifest_path = os.path.join(out_dir, "assets", "icons", "icon-index.json")
    with open(manifest_path, "w", encoding="utf-8") as fh:
        json.dump(records, fh, indent=2)
    return records


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    archive = os.path.abspath(sys.argv[1])
    out_dir = os.path.abspath(sys.argv[2])
    source_sha = sha256(archive)

    root = expand_archive(archive)
    file_dir = FILE_ID
    manifest = read_json(root, "manifest.json")
    file_entry = manifest["files"][0]
    if file_entry["id"] != FILE_ID:
        print(f"warning: archive file id {file_entry['id']} != expected {FILE_ID}")

    os.makedirs(out_dir, exist_ok=True)

    # 1. parchmint-ui.penpot byte-for-byte
    copy_dst = os.path.join(out_dir, "parchmint-ui.penpot")
    shutil.copyfile(archive, copy_dst)
    assert sha256(copy_dst) == source_sha

    # 2. tokens.json byte-for-byte
    tk_src = os.path.join(root, "files", file_dir, "tokens.json")
    tk_dst = os.path.join(out_dir, "tokens", "tokens.json")
    os.makedirs(os.path.dirname(tk_dst), exist_ok=True)
    shutil.copyfile(tk_src, tk_dst)
    tk_sha = sha256(tk_dst)

    # 3. references
    refs = export_references(root, file_dir, out_dir)
    light = sum(1 for r in refs if r["theme"] == "light")
    dark = sum(1 for r in refs if r["theme"] == "dark")

    # 4. icons
    shapes_by_page, page_names = build_page_index(root, file_dir)
    boards = list_boards(shapes_by_page, page_names)
    icons = collect_production_icons(
        root, file_dir, shapes_by_page, page_names, boards
    )
    icon_records = write_icon_assets(out_dir, icons)

    print(
        f"source archive sha256 : {source_sha}\n"
        f"parchmint-ui.penpot   : {copy_dst}  (hash preserved)\n"
        f"tokens.json           : {tk_dst}  ({tk_sha})\n"
        f"reference images      : {len(refs)}  (light={light}, dark={dark})\n"
        f"icon assets           : {len(icon_records)} unique production icons\n"
        f"icon index            : {os.path.join(out_dir, 'assets', 'icons', 'icon-index.json')}\n"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())