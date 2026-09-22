import unittest

from gnome_remote import keysyms, validate_actions


class RemoteInputValidationTests(unittest.TestCase):
    def test_supported_actions(self):
        validate_actions([
            {"click": [10, 20]}, {"right_click": [30, 40]}, {"key": "Ctrl+Shift+End"},
            {"text": "a native test."}, {"key": "Return"},
        ], 1280, 720)

    def test_key_chord_preserves_modifier_order(self):
        self.assertEqual(keysyms("Ctrl+Shift+End"), [0xFFE3, 0xFFE1, 0xFF57])

    def test_invalid_actions_are_rejected_before_input(self):
        for actions in [None, {}, [None], [{}], [{"unknown": 1}],
                        [{"key": "Ctrl+NotAKey"}], [{"key": ""}],
                        [{"text": "line\nbreak"}], [{"text": "é"}],
                        [{"key": "a", "text": "b"}]]:
            with self.subTest(actions=actions), self.assertRaises(ValueError):
                validate_actions(actions, 1280, 720)

    def test_clicks_stay_inside_verified_area(self):
        for point in [[-1, 0], [1280, 0], [0, 720], [0], ["a", 1],
                      [float("nan"), 0], [float("inf"), 0]]:
            for action in ("click", "right_click"):
                with self.subTest(action=action, point=point), self.assertRaises(ValueError):
                    validate_actions([{action: point}], 1280, 720)


if __name__ == "__main__":
    unittest.main()
