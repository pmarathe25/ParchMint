#!/usr/bin/env python3
"""Drive a disposable native window through GNOME and capture its screen area.

Requires a graphical GNOME session, dbus-python, PyGObject, GStreamer,
pipewiresrc, and pngenc. Uses private Mutter APIs; see ../USABILITY.md.
"""

import argparse
import json
from pathlib import Path
import time


KEYS = {
    "Ctrl": 0xFFE3, "Shift": 0xFFE1, "Alt": 0xFFE9,
    "Return": 0xFF0D, "Tab": 0xFF09, "Escape": 0xFF1B,
    "Home": 0xFF50, "End": 0xFF57, "BackSpace": 0xFF08,
    "Delete": 0xFFFF, "Left": 0xFF51, "Up": 0xFF52,
    "Right": 0xFF53, "Down": 0xFF54,
}
KEYS.update({f"F{number}": 0xFFBE + number - 1 for number in range(1, 25)})


def keysyms(chord):
    return [KEYS[key] if key in KEYS else ord(key) if len(key) == 1 else 0
            for key in chord.split("+")]


def validate_actions(actions, width, height):
    if not isinstance(actions, list):
        raise ValueError("Actions must be a JSON list")
    for action in actions:
        if not isinstance(action, dict) or len(action) != 1:
            raise ValueError("Each action must contain one of click, right_click, move, drag, key, text")
        kind, value = next(iter(action.items()))
        if kind in ("click", "right_click", "move"):
            if (not isinstance(value, list) or len(value) != 2
                    or not all(isinstance(v, (int, float)) for v in value)
                    or not (0 <= value[0] < width and 0 <= value[1] < height)):
                raise ValueError("Pointer position must be [x, y] inside the capture area")
        elif kind == "drag":
            if (not isinstance(value, list) or len(value) < 2
                    or any(not isinstance(point, list) or len(point) != 2
                           or not all(isinstance(v, (int, float)) for v in point)
                           or not (0 <= point[0] < width and 0 <= point[1] < height)
                           for point in value)):
                raise ValueError("Drag must be at least two [x, y] points inside the capture area")
        elif kind == "key":
            if not isinstance(value, str) or not all(keysyms(value)):
                raise ValueError("Unknown key chord")
        elif kind == "text":
            if not isinstance(value, str) or any(not 32 <= ord(c) <= 126 for c in value):
                raise ValueError("Text supports printable ASCII; use Return for a newline")
        else:
            raise ValueError(f"Unknown action: {kind}")


def run(area, actions, output):
    import dbus
    from dbus.mainloop.glib import DBusGMainLoop
    import gi
    gi.require_version("Gst", "1.0")
    from gi.repository import GLib, Gst

    DBusGMainLoop(set_as_default=True)
    Gst.init(None)
    for plugin in ("pipewiresrc", "videoconvert", "pngenc", "appsink"):
        if not Gst.ElementFactory.find(plugin):
            raise RuntimeError(f"Missing GStreamer element: {plugin}")
    bus = dbus.SessionBus()
    remote_name = "org.gnome.Mutter.RemoteDesktop"
    cast_name = "org.gnome.Mutter.ScreenCast"
    remote_path = dbus.Interface(bus.get_object(
        remote_name, "/org/gnome/Mutter/RemoteDesktop"), remote_name).CreateSession()
    remote_object = bus.get_object(remote_name, remote_path)
    remote = dbus.Interface(remote_object, remote_name + ".Session")
    pipeline = None
    try:
        session_id = dbus.Interface(remote_object, "org.freedesktop.DBus.Properties").Get(
            remote_name + ".Session", "SessionId")
        cast_path = dbus.Interface(bus.get_object(
            cast_name, "/org/gnome/Mutter/ScreenCast"), cast_name).CreateSession(
                {"remote-desktop-session-id": session_id})
        cast = dbus.Interface(bus.get_object(cast_name, cast_path), cast_name + ".Session")
        stream = cast.RecordArea(*area, {"cursor-mode": dbus.UInt32(1)})
        nodes = []
        loop = GLib.MainLoop()

        def added(node):
            nodes.append(int(node))
            loop.quit()

        bus.add_signal_receiver(added, signal_name="PipeWireStreamAdded",
                                dbus_interface=cast_name + ".Stream", path=stream)
        remote.Start()
        timeout = GLib.timeout_add_seconds(5, lambda: (loop.quit(), False)[1])
        loop.run()
        if not nodes:
            raise RuntimeError("No PipeWire stream within five seconds")
        GLib.source_remove(timeout)

        for action in actions:
            kind, value = next(iter(action.items()))
            if kind in ("click", "right_click", "move"):
                remote.NotifyPointerMotionAbsolute(str(stream), *value)
                time.sleep(0.1)
                if kind != "move":
                    button = 272 if kind == "click" else 273
                    try:
                        remote.NotifyPointerButton(button, True)
                    finally:
                        remote.NotifyPointerButton(button, False)
            elif kind == "drag":
                remote.NotifyPointerMotionAbsolute(str(stream), *value[0])
                time.sleep(0.1)
                remote.NotifyPointerButton(272, True)
                try:
                    for point in value[1:]:
                        remote.NotifyPointerMotionAbsolute(str(stream), *point)
                        time.sleep(0.05)
                finally:
                    remote.NotifyPointerButton(272, False)
            else:
                chords = [keysyms(value)] if kind == "key" else [[ord(c)] for c in value]
                for chord in chords:
                    try:
                        for key in chord:
                            remote.NotifyKeyboardKeysym(key, True)
                            time.sleep(0.03)
                    finally:
                        for key in reversed(chord):
                            remote.NotifyKeyboardKeysym(key, False)
                            time.sleep(0.03)
                    time.sleep(0.03)
            time.sleep(0.2)

        pipeline = Gst.parse_launch(
            f"pipewiresrc path={nodes[0]} ! videoconvert ! pngenc ! "
            "appsink name=shot max-buffers=1 drop=true")
        pipeline.set_state(Gst.State.PLAYING)
        sample = pipeline.get_by_name("shot").emit("try-pull-sample", 5 * Gst.SECOND)
        if sample is None:
            raise RuntimeError("No captured frame within five seconds")
        buffer = sample.get_buffer()
        with output.open("xb") as file:
            file.write(buffer.extract_dup(0, buffer.get_size()))
    finally:
        if pipeline is not None:
            pipeline.set_state(Gst.State.NULL)
        remote.Stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--area", type=int, nargs=4, required=True,
                        metavar=("X", "Y", "WIDTH", "HEIGHT"))
    parser.add_argument("--actions", default="[]", help='JSON list of click, right_click, move, drag, key, or text actions')
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.area[2] <= 0 or args.area[3] <= 0:
            raise ValueError("Area dimensions must be positive")
        if args.output.exists():
            raise ValueError("Output already exists; use a new filename")
        if not args.output.parent.is_dir():
            raise ValueError("Output parent directory must exist")
        actions = json.loads(args.actions)
        validate_actions(actions, *args.area[2:])
        run(args.area, actions, args.output)
    except Exception as error:
        parser.exit(1, f"Native remote review failed: {error}\n")
    print(args.output)


if __name__ == "__main__":
    main()
