import html
import json
from pathlib import Path
import re
import shutil
import sys
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import review


class ReviewTests(unittest.TestCase):
    def test_frame_groups_sort_numeric_times_and_keep_screens_separate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["split-1000-tiny-skia.png", "split-016-tiny-skia.png",
                         "split-000-tiny-skia.png", "static-tiny-skia.png"]:
                (root / name).touch()
            groups = review.frame_groups(root)
            self.assertEqual([time for time, _ in groups["split"]], [0, 16, 1000])
            self.assertEqual(len(groups["static-tiny-skia"]), 1)

    def test_missing_or_duplicate_frames_are_not_silently_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaises(ValueError):
                review.frame_groups(root)
            (root / "split-016-tiny-skia.png").touch()
            (root / "split-16-tiny-skia.png").touch()
            with self.assertRaises(ValueError):
                review.frame_groups(root)

    def test_artifacts_cannot_overwrite_or_enter_repository(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repo = root / "repo"
            repo.mkdir()
            with self.assertRaises(ValueError):
                review.new_output(repo / "artifacts", repo)
            output = review.new_output(root / "output", repo)
            self.assertTrue(output.is_dir())
            with self.assertRaises(FileExistsError):
                review.new_output(output, repo)

    @unittest.skipIf(sys.platform == "win32", "Windows symlinks require privileges")
    def test_repository_alias_cannot_bypass_output_isolation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            repo = root / "repository"
            repo.mkdir()
            alias = root / "alias"
            alias.symlink_to(repo, target_is_directory=True)
            with self.assertRaises(ValueError):
                review.new_output(alias / "artifacts", alias)
            self.assertFalse((repo / "artifacts").exists())

    def test_gallery_preserves_times_and_escapes_names_and_urls(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            frames = [(0, output / "a space.png"), (48, output / "b#frame.png")]
            review.write_gallery({'pane <script> "': frames}, output)
            page = (output / "index.html").read_text(encoding="utf-8")
            data = html.unescape(re.search(r'data-frames="([^"]+)"', page)[1])
            self.assertEqual(json.loads(data), [
                {"time": 0, "url": "a%20space.png"},
                {"time": 48, "url": "b%23frame.png"},
            ])
            self.assertIn("pane &lt;script&gt;", page)
            self.assertNotIn("pane <script>", page)

    @unittest.skipUnless(shutil.which("node"), "Node is needed to exercise playback logic")
    def test_gallery_playback_pause_scrub_and_end(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            review.write_gallery({"split": [(0, root / "a.png"), (16, root / "b.png"), (48, root / "c.png")]}, root)
            page = (root / "index.html").read_text()
            script = re.search(r"<script>(.*?)</script>", page, re.S)[1]
            check = r'''
const assert=require('node:assert/strict'), vm=require('node:vm'), fs=require('node:fs');
const frames=[{time:0,url:'a'},{time:16,url:'b'},{time:48,url:'c'}];
const controls={input:{value:0},button:{},img:{},a:{},output:{}};
for(const control of Object.values(controls))control.addEventListener=(name,fn)=>control[name]=fn;
const section={dataset:{frames:JSON.stringify(frames)},querySelector:name=>controls[name]};
let scheduled=null;
vm.runInNewContext(fs.readFileSync(0,'utf8'), {
 document:{querySelectorAll:()=>[section]},
 setTimeout:(fn,delay)=>{scheduled={fn,delay};return 1;},clearTimeout:()=>{scheduled=null;}
});
assert.equal(controls.img.src,'a'); controls.button.click();
assert.equal(scheduled.delay,16); scheduled.fn(); assert.equal(controls.img.src,'b');
assert.equal(scheduled.delay,32); controls.button.click(); assert.equal(scheduled,null);
controls.input.value=2;controls.input.input();assert.equal(controls.output.textContent,'48 ms');
controls.button.click();assert.equal(controls.img.src,'a');scheduled.fn();scheduled.fn();
assert.equal(controls.img.src,'c');assert.equal(controls.button.textContent,'Play');
'''
            subprocess.run(["node", "-e", check], input=script, text=True, check=True)

    def test_runner_propagates_failure_and_retains_log(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch("review.subprocess.run") as child:
                child.return_value.returncode = 3
                with self.assertRaisesRegex(RuntimeError, "failed.*3"):
                    review.run(["fake-command"], root, root / "run.log")
            self.assertTrue((root / "run.log").is_file())

    def test_capture_clears_inherited_filters_and_requires_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repo = root / "repo"
            repo.mkdir()
            args = SimpleNamespace(output=root / "output", suite="motion", filter=None)
            with patch.dict("review.os.environ", {"PARCHMINT_MOTION_FILTER": "stale"}), \
                 patch("review.metadata"), patch("review.run") as runner:
                with self.assertRaisesRegex(RuntimeError, "No motion captures"):
                    review.capture(args, repo)
            command, _, _, env = runner.call_args.args
            self.assertNotIn("PARCHMINT_MOTION_FILTER", env)
            self.assertEqual(command[command.index("-j") + 1], "1")
            self.assertIn("--locked", command)

    def test_native_isolates_paths_and_copies_the_fixture(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            repo = root / "repo"
            source = repo / "tests/parchmint-test-support/fixtures/canonical/minimal-project"
            source.mkdir(parents=True)
            (source / "project.toml").write_text("original", encoding="utf-8")
            desktop = repo / "desktop"
            desktop.touch()
            args = SimpleNamespace(output=root / "output", desktop=desktop, target="editor",
                                   appearance="dark", scale=2, width=1280, height=720, interactive=False)
            def complete(command, cwd, log, env):
                Path(command[command.index("--output") + 1]).touch()
            with patch("review.sys.platform", "linux"), patch("review.metadata"), \
                 patch("review.run", side_effect=complete) as runner:
                review.native(args, repo)
            command, _, _, env = runner.call_args.args
            self.assertEqual(Path(command[command.index("--project") + 1]), args.output / "project")
            self.assertEqual(command[command.index("--require-size") + 1], "2560x1440")
            for variable in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"]:
                self.assertTrue(Path(env[variable]).is_relative_to(args.output))
            self.assertEqual((source / "project.toml").read_text(), "original")
            for width, height in [(0, 720), (900, 720), (1280, 450)]:
                args.width, args.height = width, height
                with patch("review.sys.platform", "linux"), patch("review.run") as runner:
                    with self.assertRaises(ValueError):
                        review.native(args, repo)
                    runner.assert_not_called()
            args.interactive = True
            args.output = root / "interactive"
            with patch("review.sys.platform", "linux"), patch("review.metadata"), \
                 patch("review.run") as runner:
                review.native(args, repo)
            self.assertEqual(runner.call_args.args[0], [str(desktop), str(args.output / "project")])
            self.assertTrue(Path(runner.call_args.args[3]["XDG_DATA_HOME"]).is_relative_to(args.output))

    def test_comparison_rejects_missing_frames_and_tolerated_pixel_differences(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before, after = root / "before", root / "after"
            before.mkdir()
            after.mkdir()
            (before / "frame.png").touch()
            args = SimpleNamespace(before=before, after=after, output=root / "output", verifier="verify")
            with self.assertRaisesRegex(ValueError, "Missing actual frames"):
                review.compare(args, root / "repo")
            (after / "frame.png").touch()
            def result(command, **kwargs):
                Path(command[command.index("--report") + 1]).write_text('{"matches": false}')
                return SimpleNamespace(returncode=0)
            with patch("review.subprocess.run", side_effect=result):
                with self.assertRaisesRegex(RuntimeError, "1 frames differ"):
                    review.compare(args, root / "repo")


if __name__ == "__main__":
    unittest.main()
