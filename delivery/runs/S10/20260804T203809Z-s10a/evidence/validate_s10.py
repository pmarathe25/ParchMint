#!/usr/bin/env python3
"""Read-only S10 reconciliation validation for frozen handoff and owned outputs."""
from __future__ import annotations

import csv
import hashlib
import re
import subprocess
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[5]
HANDOFF = ROOT / "delivery/design-handoff/1.0.0"
RECON = ROOT / "delivery/design-reconciliation/1.0.0"
TRACE = ROOT / "delivery/traceability.csv"
UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")
EXPECTED_IDS_HASH = "3e45235bfb66349ae9f0967e2e7a6c81c3159cbe7e37f6ab7cf60b0a55703169"
REQUIRED_ISSUES = {"ISSUE-001", "ISSUE-002", "ISSUE-003", "ISSUE-004", "ISSUE-005"}


def fail(message: str) -> None:
    raise AssertionError(message)


def load(path: Path):
    with path.open(encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def requirement_ids(raw: str) -> set[str]:
    result: set[str] = set()
    for part in re.split(r"[|,]", raw):
        part = part.strip()
        match = re.fullmatch(r"([A-Z][A-Z0-9]*)-(\d+)[–-](\d+)", part)
        if match:
            prefix, start, end = match.groups()
            result.update(f"{prefix}-{i:0{len(start)}d}" for i in range(int(start), int(end) + 1))
        elif re.fullmatch(r"[A-Z][A-Z0-9]*-\d+", part):
            result.add(part)
        elif part:
            fail(f"unparseable requirement binding {part!r}")
    return result


def main() -> int:
    subprocess.run([sys.executable, "delivery/design-handoff/scripts/build-checksums.py", str(HANDOFF), "--verify"], cwd=ROOT, check=True)
    manifest = load(HANDOFF / "design-manifest.yaml")
    if manifest["handoff"]["status"] != "approved" or manifest["handoff"]["version"] != "1.0.0":
        fail("approved 1.0.0 handoff required")
    manifest_hash = hashlib.sha256((HANDOFF / "design-manifest.yaml").read_bytes()).hexdigest()
    if manifest_hash != "8d24ec366280e5e411258d2f7877a9ef93b9b25a364e3a5b6560930be3d2f3d3":
        fail("manifest checksum differs from dispatch")
    required_files = sorted(["design-reconciliation.md", "implementation-map.yaml", "visual-regression-plan.md", "open-issues.yaml", "work-breakdown.md", "approval.yaml"])
    if [path.name for path in sorted(RECON.iterdir()) if path.is_file()] != required_files:
        fail("reconciliation package does not contain exactly the six required files")
    implementation = load(RECON / "implementation-map.yaml")
    issues = load(RECON / "open-issues.yaml")
    approval = load(RECON / "approval.yaml")
    status = load(ROOT / "delivery/runs/S10/20260804T203809Z-s10a/status.yaml")
    handoff = load(ROOT / "delivery/runs/S10/20260804T203809Z-s10a/handoff.yaml")
    if implementation["schema_version"] != 2 or implementation["handoff_version"] != "1.0.0":
        fail("implementation map shape/version invalid")
    if issues["schema_version"] != 2 or issues["handoff_version"] != "1.0.0":
        fail("open issues shape/version invalid")
    issue_ids = {issue["id"] for issue in issues["issues"]}
    if not REQUIRED_ISSUES <= issue_ids:
        fail("all five material handoff conflicts must remain recorded")
    if approval["gate_id"] != "G10" or approval["status"] != "pending" or approval["manifest_sha256"] != manifest_hash:
        fail("G10 approval must be pending and bound to the frozen manifest")
    if status["result"] != "needs_approval" or status["stage_id"] != "S10" or handoff["stage_id"] != "S10":
        fail("S10 run artifact shape/result invalid")
    if status["candidate_commit"] != handoff["candidate_commit"] or not (ROOT / "delivery/runs/S10/20260804T203809Z-s10a/report.md").is_file():
        fail("S10 candidate provenance or report path invalid")
    with (HANDOFF / "specs/screen-inventory.csv").open(newline="", encoding="utf-8") as handle:
        screens = list(csv.DictReader(handle))
    with (HANDOFF / "specs/component-matrix.csv").open(newline="", encoding="utf-8") as handle:
        components = list(csv.DictReader(handle))
    screen_ids = {row["penpot_board_id"] for row in screens}
    component_ids = {row["penpot_component_id"] for row in components}
    if len(screens) != 80 or len(components) != 79 or not all(UUID.fullmatch(x) for x in screen_ids | component_ids):
        fail("frozen screen/component inventories are incomplete or contain invalid IDs")
    by_fixture = {}
    for row in screens:
        if row["baseline_status"] == "baseline":
            by_fixture.setdefault(row["fixture_id"], set()).add(row["theme"])
            image = HANDOFF / row["reference_image"]
            if not image.is_file():
                fail(f"missing baseline {image}")
    if len(by_fixture) != 10 or any(themes != {"light", "dark"} for themes in by_fixture.values()):
        fail("expected ten complete Light/Dark baseline fixture pairs")
    visual = (RECON / "visual-regression-plan.md").read_text(encoding="utf-8")
    for row in screens:
        if row["baseline_status"] == "baseline" and row["reference_image"] not in visual:
            fail(f"visual plan omits {row['reference_image']}")
    with TRACE.open(newline="", encoding="utf-8") as handle:
        trace_rows = list(csv.DictReader(handle))
    fields = list(trace_rows[0])
    if len(trace_rows) != 259 or fields[:4] != ["requirement_id", "requirement_summary", "penpot_screen_ids", "penpot_component_ids"]:
        fail("traceability CSV shape changed")
    sequence = "\n".join(row["requirement_id"] for row in trace_rows)
    if hashlib.sha256(sequence.encode()).hexdigest() != EXPECTED_IDS_HASH or len({row["requirement_id"] for row in trace_rows}) != 259:
        fail("traceability requirement IDs/order changed")
    trace = {row["requirement_id"]: row for row in trace_rows}
    applicable = set()
    for rows, field in ((screens, "penpot_board_id"), (components, "penpot_component_id")):
        for row in rows:
            applicable.update(requirement_ids(row["requirements"]))
    for requirement in applicable:
        if requirement not in trace:
            fail(f"handoff references unknown requirement {requirement}")
        row = trace[requirement]
        for value in filter(None, row["penpot_screen_ids"].split("|")):
            if value not in screen_ids:
                fail(f"unknown screen mapping {value} for {requirement}")
        for value in filter(None, row["penpot_component_ids"].split("|")):
            if value not in component_ids:
                fail(f"unknown component mapping {value} for {requirement}")
        if not row["penpot_screen_ids"] and not row["penpot_component_ids"]:
            covered = any(requirement in issue.get("requirements", []) and issue["severity"] == "blocking" for issue in issues["issues"])
            if not covered:
                fail(f"applicable requirement lacks mapping or blocking issue: {requirement}")
    for path in [*RECON.glob("*"), TRACE, Path(__file__)]:
        data = path.read_bytes()
        if b"\r" in data or any(line.rstrip(b" \t") != line for line in data.splitlines()):
            fail(f"LF/no-whitespace integrity failure: {path.relative_to(ROOT)}")
    print(f"S10 validation passed: {len(applicable)} applicable requirements, {len(screen_ids)} screen IDs, {len(component_ids)} component IDs, 10 Light/Dark pairs")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, KeyError, subprocess.CalledProcessError) as error:
        print(f"S10 validation failed: {error}", file=sys.stderr)
        raise SystemExit(1)
