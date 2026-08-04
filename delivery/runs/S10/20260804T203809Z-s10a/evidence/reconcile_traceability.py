#!/usr/bin/env python3
"""Derive S10's frozen-handoff Penpot UUID mappings; default mode is read-only."""
from __future__ import annotations

import argparse
import csv
import re
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[5]
HANDOFF = ROOT / "delivery/design-handoff/1.0.0/specs"
TRACEABILITY = ROOT / "delivery/traceability.csv"
RANGE = re.compile(r"^([A-Z][A-Z0-9]*)-(\d+)[–-](\d+)$")


def requirement_ids(raw: str) -> list[str]:
    result: list[str] = []
    for part in re.split(r"[|,]", raw):
        part = part.strip()
        match = RANGE.fullmatch(part)
        if match:
            prefix, start, end = match.groups()
            result.extend(f"{prefix}-{value:0{len(start)}d}" for value in range(int(start), int(end) + 1))
        elif re.fullmatch(r"[A-Z][A-Z0-9]*-\d+", part):
            result.append(part)
        elif part:
            raise ValueError(f"unsupported requirement binding: {part!r}")
    return result


def append_unique(values: list[str], value: str) -> None:
    if value not in values:
        values.append(value)


def mappings() -> tuple[dict[str, list[str]], dict[str, list[str]]]:
    screen_map: dict[str, list[str]] = defaultdict(list)
    component_map: dict[str, list[str]] = defaultdict(list)
    with (HANDOFF / "screen-inventory.csv").open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            for requirement in requirement_ids(row["requirements"]):
                append_unique(screen_map[requirement], row["penpot_board_id"])
    with (HANDOFF / "component-matrix.csv").open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            for requirement in requirement_ids(row["requirements"]):
                append_unique(component_map[requirement], row["penpot_component_id"])
    return screen_map, component_map


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true", help="write only the two S10-owned mapping columns")
    args = parser.parse_args()
    screen_map, component_map = mappings()
    with TRACEABILITY.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
        fields = handle.seek(0) or next(csv.reader(handle))
    changed = 0
    for row in rows:
        requirement = row["requirement_id"]
        screens = "|".join(screen_map.get(requirement, []))
        components = "|".join(component_map.get(requirement, []))
        if row["penpot_screen_ids"] != screens or row["penpot_component_ids"] != components:
            changed += 1
            row["penpot_screen_ids"] = screens
            row["penpot_component_ids"] = components
    if args.write:
        with TRACEABILITY.open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=fields, lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)
        print(f"updated {changed} requirements")
    else:
        print(f"would update {changed} requirements; {len(screen_map)} screen-mapped, {len(component_map)} component-mapped")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
