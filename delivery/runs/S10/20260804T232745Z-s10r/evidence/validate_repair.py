#!/usr/bin/env python3
"""Read-only validator for the S10 1.0.1 design-handoff repair candidate."""

from __future__ import annotations

import csv
import hashlib
import json
import re
import subprocess
import sys
import zipfile
from pathlib import Path
from struct import unpack

import jsonschema
import yaml


REPO = Path(__file__).resolve().parents[5]
HANDOFF_100 = REPO / "delivery/design-handoff/1.0.0"
HANDOFF_101 = REPO / "delivery/design-handoff/1.0.1"
RECON_100 = REPO / "delivery/design-reconciliation/1.0.0"
RECON_101 = REPO / "delivery/design-reconciliation/1.0.1"
SOURCE = REPO / "delivery/runs/S10/20260804T230332Z-s10p/evidence/source/parchmint-ui.penpot"
REPAIR = REPO / "delivery/runs/S10/20260804T230332Z-s10p/evidence/penpot_repair.json"
FILE_ID = "2be68822-842f-8175-8008-65eef13b0227"
ROOT_FRAME = "00000000-0000-0000-0000-000000000000"
APPROVED_SHAPES = {
    "039701d9-7e2b-8031-8008-67b6372f8819",
    "039701d9-7e2b-8031-8008-67b6372f445b",
    "039701d9-7e2b-8031-8008-67ab4a5407a6",
    "039701d9-7e2b-8031-8008-67ab4a5c9a97",
}
BOARDS = {
    "e96ec683-a782-802c-8008-65f886281b72": "PM / Screen / editor-dual-two-manuscript",
    "e96ec683-a782-802c-8008-65fb6192c697": "PM / Screen / recovered-after-crash",
}
PNG_HASHES = {
    "references/light/editor-dual-light.png": "a2cde6fb572ae5a02e83fe8c11d5d1acd63ffcf52c83621ee320df3fe6900418",
    "references/dark/editor-dual-dark.png": "d069e89e1595ee017f122e27998e9be55a4f36bb9fd33e4217e85bf87883b616",
    "references/light/error-recovery-light.png": "af3fc78b83975471b604884bee9b147e1a03ca21bc16f2a1dd31668ae37980ad",
    "references/dark/error-recovery-dark.png": "c09a16bac930ff42a87c12f9c7acb3f0081b780280fbe86244cc7fab1960a94e",
}
BASELINE_HASHES = {
    "design-manifest.yaml": "8d24ec366280e5e411258d2f7877a9ef93b9b25a364e3a5b6560930be3d2f3d3",
    "checksums.sha256": "0dfe85a871ac9db19ee2f3dcae390e8f40c14a3f2df198b51e349ceb43a3b1bf",
}

results: list[dict[str, str]] = []


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def check(name: str, condition: bool, detail: str) -> None:
    results.append({"name": name, "result": "passed" if condition else "failed", "detail": detail})


def read_yaml(path: Path):
    with path.open(encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def csv_rows(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as fh:
        return list(csv.DictReader(fh))


def checksum_ok(root: Path) -> tuple[bool, str]:
    recorded: dict[str, str] = {}
    for line in (root / "checksums.sha256").read_text(encoding="utf-8").splitlines():
        value, marker, rel = line.partition("  ")
        if not marker:
            return False, f"malformed checksum entry: {line!r}"
        recorded[rel] = value
    actual = {
        str(path.relative_to(root)): digest(path)
        for path in sorted(root.rglob("*"))
        if path.is_file() and path.name != "checksums.sha256"
    }
    return recorded == actual, f"{len(actual)} files covered"


def png_dimensions(path: Path) -> tuple[int, int]:
    data = path.read_bytes()[:24]
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise ValueError("not a PNG with IHDR")
    return unpack(">II", data[16:24])


def archive_shapes(path: Path) -> tuple[dict[str, dict], set[str]]:
    shapes: dict[str, dict] = {}
    pages: set[str] = set()
    prefix = f"files/{FILE_ID}/pages/"
    with zipfile.ZipFile(path) as archive:
        for name in archive.namelist():
            if not name.startswith(prefix) or not name.endswith(".json"):
                continue
            relative = name[len(prefix):]
            parts = relative.split("/")
            if parts:
                pages.add(parts[0].removesuffix(".json"))
            if len(parts) != 2:
                continue
            value = json.loads(archive.read(name))
            if isinstance(value, dict) and value.get("id"):
                shapes[value["id"]] = value
    return shapes, pages


def material_projection(shape: dict) -> dict:
    keys = ("id", "name", "type", "parentId", "frameId", "width", "height", "hidden", "componentId", "shapeRef")
    return {key: shape.get(key) for key in keys}


def main() -> int:
    manifest = read_yaml(HANDOFF_101 / "design-manifest.yaml")
    schema = json.loads((REPO / "delivery/templates/design-handoff/design-manifest.schema.json").read_text())
    try:
        jsonschema.validate(manifest, schema)
        schema_valid = True
    except jsonschema.ValidationError as exc:
        schema_valid = False
        schema_detail = exc.message
    else:
        schema_detail = "schema v2"
    check("candidate-manifest-schema", schema_valid, schema_detail)

    identical_paths = {
        str(path.relative_to(HANDOFF_100))
        for path in HANDOFF_100.rglob("*") if path.is_file()
    } == {
        str(path.relative_to(HANDOFF_101))
        for path in HANDOFF_101.rglob("*") if path.is_file()
    }
    check("complete-immutable-candidate", identical_paths, "1.0.1 retains the complete 1.0.0 file topology")
    checksums_valid, checksums_detail = checksum_ok(HANDOFF_101)
    check("candidate-checksums", checksums_valid, checksums_detail)

    baseline_ok = all(digest(HANDOFF_100 / rel) == expected for rel, expected in BASELINE_HASHES.items())
    baseline_checksums_ok, baseline_checksums_detail = checksum_ok(HANDOFF_100)
    check("approved-1.0.0-unchanged", baseline_ok and baseline_checksums_ok, baseline_checksums_detail)

    draft = manifest["handoff"]
    check(
        "draft-pending-approval",
        draft["version"] == "1.0.1" and draft["status"] == "draft" and not draft["approved_at"] and not draft["approved_by"],
        "1.0.1 is draft with no approver or approval timestamp",
    )

    required_paths = [manifest["penpot"]["source_file"], manifest["checksums_file"], *manifest["required_specs"].values()]
    required_paths.extend(asset["file"] for asset in manifest["assets"] if "file" in asset)
    required_paths.extend(screen["references"] for screen in manifest["screens"])
    flattened = [item for value in required_paths for item in (value if isinstance(value, list) else [value])]
    check("manifest-referenced-paths", all((HANDOFF_101 / path).is_file() for path in flattened), f"{len(flattened)} referenced files exist")

    source_matches = digest(HANDOFF_101 / "parchmint-ui.penpot") == digest(SOURCE)
    with zipfile.ZipFile(HANDOFF_101 / "parchmint-ui.penpot") as archive:
        zip_valid = archive.testzip() is None
        file_identity = any(name.startswith(f"files/{FILE_ID}/") for name in archive.namelist())
    check("native-source-zip-and-file-id", zip_valid and file_identity and source_matches, "valid ZIP, expected file ID, and byte-identical user export")

    candidate_shapes, candidate_pages = archive_shapes(HANDOFF_101 / "parchmint-ui.penpot")
    baseline_shapes, baseline_pages = archive_shapes(HANDOFF_100 / "parchmint-ui.penpot")
    boards_ok = all(
        board_id in candidate_shapes
        and candidate_shapes[board_id].get("name") == expected_name
        and (candidate_shapes[board_id].get("width"), candidate_shapes[board_id].get("height")) == (1440, 900)
        for board_id, expected_name in BOARDS.items()
    )
    check("authoritative-board-identity", boards_ok, "two boards retain IDs, names, and 1440x900 dimensions")
    hidden_ok = all(candidate_shapes.get(shape_id, {}).get("hidden") is True for shape_id in APPROVED_SHAPES)
    check("approved-repair-shapes-hidden", hidden_ok, "four Research/Harbor Notes shapes are hidden")

    compared = set(candidate_shapes) & set(baseline_shapes)
    unexpected = [
        shape_id for shape_id in sorted(compared - APPROVED_SHAPES)
        if material_projection(candidate_shapes[shape_id]) != material_projection(baseline_shapes[shape_id])
    ]
    page_drift = candidate_pages != baseline_pages
    check("no-material-archive-structural-drift", not unexpected and not page_drift, f"{len(unexpected)} unexpected material shape changes; page identity stable={not page_drift}")

    repair = json.loads(REPAIR.read_text())
    repair_hashes = {f"references/{theme}/{name}": record["sha256"] for name, record in repair["exports"].items() for theme in ["light" if name.endswith("-light.png") else "dark"]}
    png_ok = all(digest(HANDOFF_101 / rel) == expected and png_dimensions(HANDOFF_101 / rel) == (1440, 900) and repair_hashes.get(rel) == expected for rel, expected in PNG_HASHES.items())
    check("four-repair-png-provenance", png_ok, "hashes, dimensions, and repair-run provenance match")

    component_rows = csv_rows(HANDOFF_101 / "specs/component-matrix.csv")
    component_old = csv_rows(HANDOFF_100 / "specs/component-matrix.csv")
    components = {row["component_id"]: row for row in component_rows}
    removals_ok = (
        "scopedsubtree" not in components["PM/GlobalSearchPanel"]["states"]
        and all(state not in components["PM/ExportDialog"]["states"] for state in ("group", "selected-documents"))
        and all(state not in components["PM/RestoreDialog"]["states"] for state in ("document", "group-subtree"))
        and "disabledperdocument" not in components["PM/SpellcheckUnderline"]["states"]
        and all(state in components["PM/RestoreDialog"]["states"] for state in ("whole-project", "restoring", "error"))
    )
    check("canonical-component-states", removals_ok, "unsupported states removed; allowed RestoreDialog states retained")
    interaction = (HANDOFF_101 / "specs/interaction-spec.md").read_text(encoding="utf-8")
    inventory = (HANDOFF_101 / "specs/screen-inventory.csv").read_text(encoding="utf-8")
    check("comments-and-export-wording", "Entire Manual" not in interaction and "Entire Manuscript" in interaction and "continuous, unsectioned" in interaction and "default is all threads" in inventory, "canonical wording present")

    screens = csv_rows(HANDOFF_101 / "specs/screen-inventory.csv")
    screens_old = csv_rows(HANDOFF_100 / "specs/screen-inventory.csv")
    component_ids_stable = {r["component_id"]: r["penpot_component_id"] for r in component_rows} == {r["component_id"]: r["penpot_component_id"] for r in component_old}
    screen_ids_stable = {r["screen_id"]: (r["penpot_page_id"], r["penpot_board_id"]) for r in screens} == {r["screen_id"]: (r["penpot_page_id"], r["penpot_board_id"]) for r in screens_old}
    check("stable-79-components-80-screens", len(component_rows) == 79 and len(screens) == 80 and component_ids_stable and screen_ids_stable, "mapping counts and Penpot IDs retained")

    approval = read_yaml(RECON_101 / "approval.yaml")
    issues = read_yaml(RECON_101 / "open-issues.yaml")
    manifest_hash = digest(HANDOFF_101 / "design-manifest.yaml")
    map_text = (RECON_101 / "implementation-map.yaml").read_text(encoding="utf-8")
    resolved = {item["id"] for item in issues["resolved_issues"]}
    open_ids = {item["id"] for item in issues["open_issues"]}
    recon_ok = (
        approval["handoff_version"] == "1.0.1"
        and approval["manifest_sha256"] == manifest_hash
        and approval["status"] == "pending"
        and not approval["approved_by"] and not approval["approved_at"]
        and "delivery/design-handoff/1.0.0" not in map_text
        and {"ISSUE-001", "ISSUE-002", "ISSUE-003", "ISSUE-004", "ISSUE-005"} <= resolved
        and open_ids == {"ISSUE-006", "ISSUE-007"}
    )
    check("reconciliation-consistency", recon_ok, "candidate paths/hash, mappings, resolved issues, and nonblocking limits agree")

    with (REPO / "delivery/traceability.csv").open(encoding="utf-8", newline="") as fh:
        trace = csv.DictReader(fh)
        trace_rows = list(trace)
        trace_ok = bool(trace.fieldnames) and bool(trace_rows) and subprocess.run(["git", "diff", "--quiet", "--", "delivery/traceability.csv"], cwd=REPO).returncode == 0
    check("traceability-integrity", trace_ok, f"{len(trace_rows)} rows parse and traceability is unchanged because stable IDs need no update")

    text_files = [
        path for root in (HANDOFF_101, RECON_101)
        for path in root.rglob("*")
        if path.suffix in {".csv", ".json", ".md", ".py", ".sha256", ".yaml"}
    ]
    whitespace_ok = all(b"\r\n" not in path.read_bytes() and not re.search(rb"[ \t]+\n", path.read_bytes()) for path in text_files)
    check("lf-and-trailing-whitespace", whitespace_ok, f"{len(text_files)} text artifacts checked")

    passed = all(item["result"] == "passed" for item in results)
    print(json.dumps({"schema_version": 1, "stage_id": "S10", "run_id": "20260804T232745Z-s10r", "result": "passed" if passed else "failed", "checks": results, "limits": ["Static design/package evidence only; this does not prove native-interactive, accessibility, performance, or platform behavior."]}, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
