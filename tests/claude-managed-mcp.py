import json
import subprocess
import sys
import unittest


JAQ, FILTER = sys.argv[1:3]
del sys.argv[1:3]
MANAGED = ["browser", "computer_use", "managed"]


def merge(value):
    return subprocess.run(
        [
            JAQ,
            "-nc",
            "--argjson",
            "dynamic",
            json.dumps(value),
            "--argjson",
            "static",
            json.dumps(MANAGED),
            "-f",
            FILTER,
        ],
        text=True,
        capture_output=True,
    )


class ClaudeManagedMcp(unittest.TestCase):
    def assert_preserved(self, value):
        result = merge(value)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), value)

    def test_missing_map(self):
        self.assert_preserved({})
        self.assert_preserved({"session": "preserved", "projects": {"/example": {}}})

    def test_null_map(self):
        self.assert_preserved({"mcpServers": None, "session": "preserved"})

    def test_empty_map(self):
        self.assert_preserved({"mcpServers": {}})

    def test_removes_only_managed_entries(self):
        value = {
            "session": "preserved",
            "projects": {"/example": {"mcpServers": {"browser": {"local": True}}}},
            "mcpServers": {
                "browser": {"command": "managed-browser"},
                "computer_use": {"command": "managed-computer"},
                "managed": {"command": "managed-server"},
                "personal": {"command": "keep"},
            },
        }
        expected = dict(value, mcpServers={"personal": {"command": "keep"}})
        result = merge(value)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), expected)

    def test_rejects_invalid_root(self):
        for value in [None, [], "invalid", 1]:
            with self.subTest(value=value):
                result = merge(value)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("must be a JSON object", result.stderr)

    def test_rejects_invalid_map(self):
        for value in [[], "invalid", 1]:
            with self.subTest(value=value):
                result = merge({"mcpServers": value})
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("must be an object or null", result.stderr)


if __name__ == "__main__":
    unittest.main()
