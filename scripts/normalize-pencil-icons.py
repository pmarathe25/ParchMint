#!/usr/bin/env python3
"""Normalize imported pencil SVGs. Requires cairosvg, Pillow, numpy and scipy.

Run with a directory of original SVGs and an output directory. The six retained
contours preserve the pencil silhouette and translucency at toolbar sizes while
avoiding dozens of nearly identical grain contours per icon. The brand is separate.
"""
import argparse
import io
import xml.etree.ElementTree as ET
from pathlib import Path

import cairosvg
import numpy as np
from PIL import Image
from scipy import ndimage

parser = argparse.ArgumentParser()
parser.add_argument("source", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
ET.register_namespace("", "http://www.w3.org/2000/svg")
for source in sorted(args.source.glob("*.svg")):
    root = ET.parse(source).getroot()
    paths = [node for node in root if node.tag.endswith("path")]
    if len(paths) not in (33, 34):
        raise ValueError(f"{source}: expected the original contour layers")
    image = Image.open(io.BytesIO(cairosvg.svg2png(url=str(source))))
    labels, count = ndimage.label(np.asarray(image)[:, :, 3] > 80)
    regions = ndimage.find_objects(labels)
    components = sorted(
        [(int((labels == index + 1).sum()), region) for index, region in enumerate(regions)],
        key=lambda item: item[0], reverse=True,
    )
    # The corrected right chevron contains a stray piece of adjacent artwork.
    kept = components[:1] if source.stem == "chevron-right" else [
        item for item in components if item[0] >= components[0][0] * 0.02
    ]
    left = min(region[1].start for _, region in kept)
    right = max(region[1].stop for _, region in kept)
    top = min(region[0].start for _, region in kept)
    bottom = max(region[0].stop for _, region in kept)
    # Letter glyphs share a cap-height rather than a maximum-dimension fit.
    # Underline includes its separate baseline; strikethrough has a wide crossbar.
    occupancy = 0.72 if source.stem in ("bold", "italic") else 0.82
    if source.stem == "strikethrough":
        center_y = (top + bottom) / 2
        scale_y = (right - left) * (0.72 / 0.82) / (bottom - top)
        for path in paths:
            path.set("transform", f"translate(0 {center_y}) scale(1 {scale_y:.5f}) translate(0 {-center_y})")
    side = max(right - left, bottom - top) / occupancy
    root.set("viewBox", f"{(left + right - side) / 2:.2f} {(top + bottom - side) / 2:.2f} {side:.2f} {side:.2f}")
    previous = 0
    keep = dict(zip([2, 7, 13, 19, 25, 31], [.10, .26, .44, .62, .80, .96]))
    for index, path in enumerate(paths):
        if index not in keep:
            root.remove(path)
            continue
        level = keep[index]
        path.set("fill-opacity", f"{(level - previous) / (1 - previous):.5f}")
        previous = level
    for node in root:
        if node.tag.endswith("desc"):
            node.text = "Pencil artwork normalized to a centered optical box and six translucent vector contours."
    ET.ElementTree(root).write(args.output / source.name, encoding="utf-8", xml_declaration=True)
