#!/usr/bin/env python3
"""Capture production workflows and build a local frame-review gallery."""

import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
from urllib.parse import quote


FRAME = re.compile(r"^(.*)-(\d+)-tiny-skia\.png$")


def repository(explicit=None):
    if explicit:
        candidates = [Path(explicit).resolve()]
    else:
        candidates = [Path.cwd(), *Path(__file__).resolve().parents]
    for candidate in candidates:
        if (candidate / "Cargo.lock").is_file() and (candidate / "tests/parchmint-ui-driver").is_dir():
            return candidate
    raise ValueError("Run from the ParchMint root or supply --repo.")


def new_output(path, repo):
    path = Path(path).resolve()
    if path.is_relative_to(repo):
        raise ValueError("Keep review artifacts outside the repository.")
    path.mkdir(parents=True, exist_ok=False)
    return path


def run(command, repo, log, env=None):
    print("Running:", " ".join(map(str, command)), flush=True)
    with log.open("x", encoding="utf-8") as stream:
        result = subprocess.run(command, cwd=repo, env=env, stdout=stream, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f"Command failed ({result.returncode}); see {log}")


def metadata(repo, output, binary=None):
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=repo, text=True).strip()
    info = {
        "commit": git("rev-parse", "HEAD"),
        "worktree": git("status", "--short"),
        "platform": platform.platform(),
    }
    if binary:
        with binary.open("rb") as stream:
            info["binary_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
        info["binary"] = str(binary)
    (output / "run.json").write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")


def capture(args, repo):
    output = new_output(args.output, repo)
    metadata(repo, output)
    suites = ["motion", "usability"] if args.suite == "all" else [args.suite]
    for suite in suites:
        env = os.environ.copy()
        env.pop("PARCHMINT_MOTION_FILTER", None)
        env.pop("PARCHMINT_MOTION_FRAMES", None)
        env.pop("PARCHMINT_REVIEW_ARTIFACTS", None)
        variable = "PARCHMINT_MOTION_FRAMES" if suite == "motion" else "PARCHMINT_REVIEW_ARTIFACTS"
        env[variable] = str(output / suite)
        if args.filter and suite == "motion":
            env["PARCHMINT_MOTION_FILTER"] = args.filter
        command = ["cargo", "test", "--locked", "-j", "1", "-p", "parchmint-ui-driver",
                   "--test", f"{suite}_flows", "--", "--test-threads=1"]
        run(command, repo, output / f"{suite}.log", env)
        if not list((output / suite).glob("*.png")):
            raise RuntimeError(f"No {suite} captures were produced; check the filter and log.")
    print(output)


def frame_groups(root):
    groups = {}
    for path in sorted(root.rglob("*.png")):
        relative = path.relative_to(root)
        match = FRAME.fullmatch(path.name)
        name = relative.parent / (match[1] if match else path.stem)
        elapsed = int(match[2]) if match else 0
        groups.setdefault(str(name), []).append((elapsed, path))
    if not groups:
        raise ValueError(f"No PNGs found under {root}")
    for name, frames in groups.items():
        frames.sort(key=lambda frame: frame[0])
        times = [time for time, _ in frames]
        if len(set(times)) != len(times):
            raise ValueError(f"Duplicate frame time in {name}")
    return groups


def write_gallery(groups, output):
    sections = []
    for name, frames in groups.items():
        data = [{"time": time, "url": quote(os.path.relpath(path, output).replace(os.sep, "/"))}
                for time, path in frames]
        encoded = html.escape(json.dumps(data), quote=True)
        sections.append(f'''<section data-frames="{encoded}">
<h2>{html.escape(name)}</h2><button type="button">Play</button>
<input aria-label="Frame for {html.escape(name, quote=True)}" type="range" min="0" max="{len(frames)-1}" value="0">
<output>0 ms</output><a target="_blank" rel="noopener"><img alt="{html.escape(name, quote=True)}"></a></section>''')
    document = '''<!doctype html><html lang="en"><meta charset="utf-8">
<title>ParchMint frame review</title>
<style>body{font:16px system-ui;background:#ddd;color:#222;margin:24px}section{margin:24px 0;padding:16px;background:white}img{display:block;max-width:100%;margin-top:12px}input{width:50%;margin:0 12px}h2{font-size:18px}</style>
<h1>ParchMint frame review</h1><p>Sampled captures. Click an image for full resolution. Playback uses recorded frame intervals, not native frame pacing.</p>
''' + "\n".join(sections) + '''<script>
for (const section of document.querySelectorAll('section')) {
  const frames=JSON.parse(section.dataset.frames), slider=section.querySelector('input');
  const button=section.querySelector('button'), image=section.querySelector('img');
  let timer=null;
  function show() {const f=frames[Number(slider.value)]; image.src=f.url; section.querySelector('a').href=f.url; section.querySelector('output').textContent=f.time+' ms';}
  function stop() {clearTimeout(timer); timer=null; button.textContent='Play';}
  function step() {const i=Number(slider.value); if(i+1===frames.length){stop();return;}
    timer=setTimeout(()=>{slider.value=i+1;show();step();}, frames[i+1].time-frames[i].time);}
  slider.addEventListener('input',()=>{stop();show();});
  button.addEventListener('click',()=>{if(timer!==null){stop();return;} if(Number(slider.value)+1===frames.length)slider.value=0;show();button.textContent='Pause';step();});
  show();
}
</script></html>'''
    (output / "index.html").write_text(document, encoding="utf-8")


def contact_sheets(groups, output):
    from PIL import Image, ImageDraw
    for index, (name, frames) in enumerate(groups.items()):
        columns = min(3, len(frames))
        sheet = Image.new("RGB", (480 * columns, 330 * ((len(frames) + columns - 1) // columns)), "white")
        draw = ImageDraw.Draw(sheet)
        for cell, (time, path) in enumerate(frames):
            with Image.open(path) as source:
                thumbnail = source.convert("RGB")
                thumbnail.thumbnail((480, 300))
            x, y = cell % columns * 480, cell // columns * 330
            sheet.paste(thumbnail, (x, y + 25))
            draw.text((x + 4, y + 4), f"{name}: {time} ms", fill="black")
        sheet.save(output / f"sheet-{index:03}.png")


def gallery(args, repo):
    groups = frame_groups(Path(args.frames).resolve())
    output = new_output(args.output, repo)
    write_gallery(groups, output)
    if args.sheets:
        contact_sheets(groups, output)
    print(output / "index.html")


def compare(args, repo):
    before, after = Path(args.before).resolve(), Path(args.after).resolve()
    reference = sorted(before.rglob("*.png"))
    if not reference:
        raise ValueError("No reference PNGs found.")
    missing = [path.relative_to(before) for path in reference
               if not (after / path.relative_to(before)).is_file()]
    if missing:
        raise ValueError(f"Missing actual frames: {missing}")
    output = new_output(args.output, repo)
    verifier = str(Path(args.verifier).resolve())
    differences = []
    for index, path in enumerate(reference):
        relative = path.relative_to(before)
        report = output / f"{index:03}.json"
        command = [verifier, "compare", "--reference", str(path),
                   "--actual", str(after / relative), "--diff", str(output / f"{index:03}.png"),
                   "--report", str(report)]
        with (output / f"{index:03}.log").open("x", encoding="utf-8") as log:
            result = subprocess.run(command, cwd=repo, stdout=log, stderr=subprocess.STDOUT)
        if result.returncode not in [0, 1]:
            raise RuntimeError(f"Comparison failed for {relative}; see {output}")
        metrics = json.loads(report.read_text(encoding="utf-8"))
        if not metrics["matches"]:
            differences.append(str(relative))
    (output / "summary.json").write_text(json.dumps({
        "compared": len(reference), "different": differences,
    }, indent=2) + "\n", encoding="utf-8")
    if differences:
        raise RuntimeError(f"{len(differences)} frames differ; inspect {output}")
    print(f"{len(reference)} frames are pixel-identical.")


def native(args, repo):
    if sys.platform != "linux":
        raise ValueError("Native data isolation in this helper supports Linux; use a disposable OS profile elsewhere.")
    if not args.interactive and (args.width <= 0 or args.height <= 0):
        raise ValueError("Window dimensions must be positive.")
    if not args.interactive and args.target != "launcher" and (args.width < 1280 or args.height < 720):
        raise ValueError("Project windows require at least 1280x720 logical pixels.")
    desktop = Path(args.desktop).resolve()
    if not desktop.is_file():
        raise ValueError(f"Build the production desktop first: {desktop}")
    output = new_output(args.output, repo)
    metadata(repo, output, desktop)
    fixture = repo / "tests/parchmint-test-support/fixtures/canonical/minimal-project"
    project = output / "project"
    shutil.copytree(fixture, project)
    env = os.environ.copy()
    for variable, name in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")]:
        env[variable] = str(output / name)
    command = [str(desktop), "capture", "--target", args.target,
               "--appearance", args.appearance, "--project", str(project),
               "--scale", str(args.scale), "--logical-width", str(args.width),
               "--logical-height", str(args.height),
               "--require-size", f"{args.width * args.scale}x{args.height * args.scale}",
               "--output", str(output / "native.png")]
    if args.interactive:
        command = [str(desktop), str(project)]
    run(command, repo, output / "native.log", env)
    if not args.interactive and not (output / "native.png").is_file():
        raise RuntimeError("Native capture succeeded without a PNG.")
    print(output if args.interactive else output / "native.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo")
    commands = parser.add_subparsers(dest="command", required=True)
    collect = commands.add_parser("capture", help="Run production motion/usability flows")
    collect.add_argument("--suite", choices=["all", "motion", "usability"], default="all")
    collect.add_argument("--filter", help="Comma-separated motion capture name prefixes")
    collect.add_argument("--output", required=True)
    view = commands.add_parser("gallery", help="Create a scrub/play gallery from existing PNGs")
    view.add_argument("--frames", required=True)
    view.add_argument("--output", required=True)
    view.add_argument("--sheets", action="store_true", help="Also create contact sheets (requires Pillow)")
    difference = commands.add_parser("compare", help="Require pixel-identical frames using the production verifier")
    difference.add_argument("--before", required=True)
    difference.add_argument("--after", required=True)
    difference.add_argument("--output", required=True)
    difference.add_argument("--verifier", required=True)
    window = commands.add_parser("native", help="Capture a native Linux window with isolated data")
    window.add_argument("--desktop", required=True)
    window.add_argument("--output", required=True)
    window.add_argument("--target", choices=["launcher", "editor", "cards", "settings", "export"], default="editor")
    window.add_argument("--appearance", choices=["light", "dark"], default="light")
    window.add_argument("--scale", type=int, choices=[1, 2], default=1)
    window.add_argument("--width", type=int, default=1280)
    window.add_argument("--height", type=int, default=720)
    window.add_argument("--interactive", action="store_true", help="Open the normal resizable app instead of capturing; choose appearance and scale in the UI/OS")
    args = parser.parse_args()
    try:
        {"capture": capture, "gallery": gallery, "compare": compare, "native": native}[args.command](args, repository(args.repo))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
