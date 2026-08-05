#!/usr/bin/env python3
"""Deterministic structural checks for the 1.0.1 S10 repair candidate."""

import csv
import hashlib
import json
import re
import struct
import subprocess
import sys
import zipfile
from collections import Counter
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[5]
HANDOFF = ROOT / "delivery/design-handoff/1.0.1"
SOURCE = HANDOFF / "parchmint-ui.penpot"
TRACEABILITY = ROOT / "delivery/traceability.csv"
COMPONENTS = HANDOFF / "specs/component-matrix.csv"
SCREENS = HANDOFF / "specs/screen-inventory.csv"
DISPOSITIONS = Path(__file__).with_name("requirement-dispositions.yaml")
IMPLEMENTATION_MAP = ROOT / "delivery/design-reconciliation/1.0.1/implementation-map.yaml"
CHECKSUMS = HANDOFF / "checksums.sha256"
APPROVED_HANDOFF_BASELINE = "276baa3"

SOURCE_SHA = "2c41059ee5b6b5eb2099d1cc5e090dd42c91d0835ff97d66104b46610d20a35d"
OLD_LIGHT_SHA = "a2cde6fb572ae5a02e83fe8c11d5d1acd63ffcf52c83621ee320df3fe6900418"
DARK_SHA = "e8ec7283998a5423a73440be42393729c884b3171085f4254a5d8785d5b49d5c"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def png_dimensions(path: Path):
    header = path.read_bytes()[:24]
    if header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        raise ValueError(f"not a PNG: {path}")
    return struct.unpack(">II", header[16:24])


def csv_rows(path: Path):
    with path.open(newline="") as stream:
        return list(csv.DictReader(stream))


def has_requirement(value: str, requirement: str) -> bool:
    """Recognize direct IDs and the inventory's inclusive en-dash ranges."""
    if requirement in value:
        return True
    prefix, number = requirement.rsplit("-", 1)
    for start, end in re.findall(rf"{re.escape(prefix)}-(\d{{3}})–(\d{{3}})", value):
        if int(start) <= int(number) <= int(end):
            return True
    return False


def source_shapes():
    targets = {
        "039701d9-7e2b-8031-8008-67b6372f8819": False,
        "039701d9-7e2b-8031-8008-67b6372f445b": False,
        "039701d9-7e2b-8031-8008-67ab4a5407a6": True,
        "039701d9-7e2b-8031-8008-67ab4a5c9a97": True,
    }
    found = {}
    with zipfile.ZipFile(SOURCE) as archive:
        for name in archive.namelist():
            if not name.endswith(".json"):
                continue
            try:
                document = json.loads(archive.read(name))
            except (UnicodeDecodeError, json.JSONDecodeError):
                continue
            stack = [document]
            while stack:
                node = stack.pop()
                if isinstance(node, dict):
                    if node.get("id") in targets:
                        found[node["id"]] = node.get("hidden")
                    stack.extend(node.values())
                elif isinstance(node, list):
                    stack.extend(node)
    return targets, found


def validate_checksums() -> list[str]:
    """Require the handoff checksum manifest to describe precisely its files."""
    failures = []
    recorded = {}
    for line in CHECKSUMS.read_text().splitlines():
        digest_value, separator, relative_path = line.partition("  ")
        if not separator or not re.fullmatch(r"[0-9a-f]{64}", digest_value):
            failures.append(f"malformed checksum record: {line!r}")
            continue
        if relative_path in recorded:
            failures.append(f"duplicate checksum record: {relative_path}")
            continue
        recorded[relative_path] = digest_value

    actual = {
        path.relative_to(HANDOFF).as_posix()
        for path in HANDOFF.rglob("*")
        if path.is_file() and path != CHECKSUMS
    }
    missing = sorted(actual - recorded.keys())
    stale = sorted(recorded.keys() - actual)
    if missing:
        failures.append("checksum records missing files: " + ", ".join(missing))
    if stale:
        failures.append("checksum records reference missing files: " + ", ".join(stale))
    for relative_path, expected in recorded.items():
        path = HANDOFF / relative_path
        if path.exists() and digest(path) != expected:
            failures.append(f"checksum digest mismatch: {relative_path}")
    return failures


def main() -> int:
    failures = []
    if digest(SOURCE) != SOURCE_SHA:
        failures.append("unexpected refreshed Penpot source SHA-256")
    if not zipfile.is_zipfile(SOURCE):
        failures.append("Penpot source is not a ZIP archive")

    targets, found = source_shapes()
    if found != targets:
        failures.append(f"Explorer/tab visibility mismatch: {found!r}")

    component_rows = csv_rows(COMPONENTS)
    screen_rows = csv_rows(SCREENS)
    trace_rows = {row["requirement_id"]: row for row in csv_rows(TRACEABILITY)}
    component_by_id = {row["component_id"]: row for row in component_rows}
    screen_by_id = {row["screen_id"]: row for row in screen_rows}

    required_component_bindings = (
        ("PM/LauncherProjectCard", "PRJ-010"),
        ("PM/AppearanceChoice", "APPR-006"),
        ("PM/StyleEditor", "FMT-019"),
        ("PM/MetadataDefinitionEditor", "META-011"),
        ("PM/Card/MetadataValue", "CARD-010"),
        ("PM/StatusBar", "WORD-002"),
        ("PM/SpellcheckUnderline", "SPELL-005"),
        ("PM/SpellingContextMenu", "SPELL-004"),
        ("PM/SpellingContextMenu", "SPELL-005"),
        ("PM/LoadingState", "A11Y-007"),
        ("PM/ProgressState", "A11Y-007"),
        ("PM/Toast", "A11Y-007"),
    )
    for component, requirement in required_component_bindings:
        if not has_requirement(component_by_id.get(component, {}).get("requirements", ""), requirement):
            failures.append(f"missing component annotation {component} -> {requirement}")
    for component in ("PM/ErrorBanner", "PM/SpellcheckUnderline"):
        if has_requirement(component_by_id.get(component, {}).get("requirements", ""), "SPELL-004"):
            failures.append(f"SPELL-004 must not be annotated on {component}")
    menu_states = component_by_id.get("PM/SpellingContextMenu", {}).get("states", "")
    if "dictionaryerror" not in menu_states:
        failures.append("SpellingContextMenu lacks dictionaryerror state")

    required_screen_bindings = (
        ("launcher-light", "PRJ-010"),
        ("settings-delete-unused-style-light", "FMT-019"),
        ("settings-metadata-fields-light", "META-011"),
        ("cards-light", "CARD-010"),
        ("editor-spellcheck-suggestions-light", "SPELL-004"),
        ("editor-spellcheck-suggestions-light", "SPELL-005"),
        ("layout-1280x720-reference", "WS-011"),
        ("platform-windows-reference", "PLAT-001"),
    )
    for screen, requirement in required_screen_bindings:
        if not has_requirement(screen_by_id.get(screen, {}).get("requirements", ""), requirement):
            failures.append(f"missing screen annotation {screen} -> {requirement}")

    direct = {
        "PRJ-010": ("e96ec683-a782-802c-8008-65f767f20cfd", ("e96ec683-a782-802c-8008-65f701e0505c",)),
        "WS-011": ("c5362ef2-ec03-8060-8008-68ac7f8d72b3", ()),
        "FMT-019": ("e96ec683-a782-802c-8008-65fab0e9731e", ("e96ec683-a782-802c-8008-65f7052e0f41",)),
        "META-011": ("e96ec683-a782-802c-8008-65fac2af3c48", ("e96ec683-a782-802c-8008-65f7064e136b",)),
        "CARD-010": ("e96ec683-a782-802c-8008-65f906f35093", ("e96ec683-a782-802c-8008-65f6c1031f26",)),
        "WORD-002": (None, ("e96ec683-a782-802c-8008-65f5e7195d18",)),
        "SPELL-004": ("e96ec683-a782-802c-8008-6609938011df", ("e96ec683-a782-802c-8008-66077d6e0583",)),
        "SPELL-005": ("e96ec683-a782-802c-8008-6609938011df", ("e96ec683-a782-802c-8008-66077bd31eb2", "e96ec683-a782-802c-8008-66077d6e0583")),
        "A11Y-007": (None, ("e96ec683-a782-802c-8008-65f5a65dd2a8",)),
        "A11Y-009": ("469ffc7d-964a-806d-8008-6c33b32674a9", ("e96ec683-a782-802c-8008-65f5a74ac4f2",)),
    }
    for requirement, (screen_id, component_ids) in direct.items():
        row = trace_rows.get(requirement, {})
        if screen_id and screen_id not in row.get("penpot_screen_ids", ""):
            failures.append(f"missing trace screen mapping for {requirement}")
        for component_id in component_ids:
            if component_id not in row.get("penpot_component_ids", ""):
                failures.append(f"missing trace component mapping for {requirement}: {component_id}")

    tree = trace_rows.get("TREE-001", {})
    for marker in ("e96ec683-a782-802c-8008-65f886281b72", "e96ec683-a782-802c-8008-65f89294547e", "Whenever Explorer is shown"):
        if marker not in (tree.get("penpot_screen_ids", "") + tree.get("current_notes", "")):
            failures.append("TREE-001 invariant is not fully recorded")
    for requirement in ("SAVE-014", "EXP-009"):
        row = trace_rows.get(requirement, {})
        if not row.get("penpot_screen_ids") or not row.get("penpot_component_ids"):
            failures.append(f"prior mapped requirement regressed: {requirement}")

    blank = {rid for rid, row in trace_rows.items() if not row["penpot_screen_ids"] and not row["penpot_component_ids"]}
    disposition_doc = yaml.safe_load(DISPOSITIONS.read_text())
    listed = {
        requirement
        for category in disposition_doc["categories"].values()
        for requirement in category["requirements"]
    }
    if blank != listed:
        missing = sorted(blank - listed)
        extra = sorted(listed - blank)
        if missing:
            failures.append("unclassified unmapped requirements: " + ", ".join(missing))
        if extra:
            failures.append("dispositions that are not unmapped requirements: " + ", ".join(extra))

    mapped_screen_ids = re.findall(r"screen_id:\s*([^,}\s]+)", IMPLEMENTATION_MAP.read_text())
    inventory_screen_ids = [row["screen_id"] for row in screen_rows]
    if Counter(mapped_screen_ids) != Counter(inventory_screen_ids):
        missing = sorted((Counter(inventory_screen_ids) - Counter(mapped_screen_ids)).elements())
        extra = sorted((Counter(mapped_screen_ids) - Counter(inventory_screen_ids)).elements())
        if missing:
            failures.append("implementation map omits inventory screens: " + ", ".join(missing))
        if extra:
            failures.append("implementation map has non-inventory screens: " + ", ".join(extra))

    light = HANDOFF / "references/light/editor-dual-light.png"
    dark = HANDOFF / "references/dark/editor-dual-dark.png"
    if digest(light) == OLD_LIGHT_SHA:
        failures.append("external blocker: refreshed Light editor-dual reference has not been supplied")
    if digest(dark) != DARK_SHA:
        failures.append("unexpected repaired Dark editor-dual reference SHA-256")
    for reference in (light, dark):
        try:
            if png_dimensions(reference) != (1440, 900):
                failures.append(f"unexpected editor-dual dimensions: {reference}")
        except ValueError as error:
            failures.append(str(error))

    failures.extend(validate_checksums())
    baseline_check = subprocess.run(
        ["git", "diff", "--exit-code", APPROVED_HANDOFF_BASELINE, "--", "delivery/design-handoff/1.0.0"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if baseline_check.returncode:
        failures.append("approved 1.0.0 handoff differs from its immutable baseline")

    if failures:
        print("FAILED")
        for failure in failures:
            print("- " + failure)
        return 1
    print("passed: source visibility, exact inventory/map coverage, annotations, traceability, dispositions, checksums, immutable 1.0.0, and refreshed references")
    return 0


if __name__ == "__main__":
    sys.exit(main())
