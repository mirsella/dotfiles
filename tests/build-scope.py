#!/usr/bin/env python3
"""Exercise build wrappers against a live user systemd session, without builds."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
PROBE = """import json, os, pathlib, subprocess, sys
cgroup = pathlib.Path("/proc/self/cgroup").read_text().strip()
print(json.dumps({
    "args": sys.argv[1:],
    "cwd": os.getcwd(),
    "stdin": sys.stdin.read(),
    "env": os.environ["BUILD_SCOPE_TEST"],
    "cgroup": cgroup,
    "oom_score_adj": pathlib.Path("/proc/self/oom_score_adj").read_text().strip(),
    "oom_policy": subprocess.check_output([
        "systemctl", "--user", "show", "--property=OOMPolicy", "--value",
        cgroup.rsplit("/", 1)[1],
    ], text=True).strip(),
}))
"""


class BuildScopeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(
            prefix="build-scope-test-", dir="/tmp/opencode"
        )
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        self.wrappers = self.work / "wrappers"
        self.real = self.work / "real tools"
        self.project = self.work / "-project with spaces"
        for directory in (self.wrappers, self.real, self.project):
            directory.mkdir()
        (self.project / "Cargo.toml").touch()
        self.helper = self.wrappers / "build-scope"
        shutil.copyfile(
            ROOT / "dotfiles/dot_local/bin/executable_build-scope", self.helper
        )
        self.helper.chmod(0o755)
        self.env = dict(os.environ, BUILD_SCOPE_TEST="preserved $value")
        self.env["PATH"] = f"{self.wrappers}:{self.real}:{os.environ['PATH']}"

    def run_command(self, *args, **kwargs):
        return subprocess.run(
            args,
            cwd=self.project,
            env=self.env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=20,
            **kwargs,
        )

    def test_scope_arguments_environment_and_stdio(self):
        arguments = ["two words", "", "$HOME", "--manifest-path", "Cargo.toml"]
        result = self.run_command(
            self.helper,
            sys.executable,
            "-c",
            PROBE,
            *arguments,
            input="input through the scope\n",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data["args"], arguments)
        self.assertEqual(data["cwd"], str(self.project))
        self.assertEqual(data["stdin"], "input through the scope\n")
        self.assertEqual(data["env"], "preserved $value")
        self.assertIn("/cargo.slice/", data["cgroup"])
        self.assertTrue(data["cgroup"].endswith(".scope"))
        self.assertEqual(data["oom_score_adj"], "500")
        self.assertEqual(data["oom_policy"], "continue")

    def test_path_wrappers_and_nested_scope(self):
        shim = self.wrappers / "wasm-opt"
        shim.symlink_to("build-scope")
        executable = self.real / "wasm-opt"
        executable.write_text(f"#!{sys.executable}\n" + PROBE)
        executable.chmod(0o755)
        probe = """import json, pathlib, subprocess
outer = pathlib.Path('/proc/self/cgroup').read_text().strip()
child = subprocess.run(['wasm-opt', '-Oz', 'input with spaces.wasm'],
                       input='', text=True, capture_output=True, check=True)
print(json.dumps({'outer': outer, 'child': json.loads(child.stdout)}))
"""
        result = self.run_command(self.helper, sys.executable, "-c", probe)
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data["child"]["cgroup"], data["outer"])
        self.assertEqual(data["child"]["args"], ["-Oz", "input with spaces.wasm"])
        direct = self.run_command(shim, "--version", input="")
        self.assertEqual(direct.returncode, 0, direct.stderr)
        self.assertIn("/cargo.slice/wasm", json.loads(direct.stdout)["cgroup"])

    def test_cargo_finds_real_tool_through_path(self):
        # Match the deployed cross-directory symlink without assuming /usr/bin/cargo.
        cargo_bin = self.work / "share/cargo/bin"
        cargo_bin.mkdir(parents=True)
        (self.work / "bin").symlink_to(self.wrappers, target_is_directory=True)
        target = (
            (ROOT / "dotfiles/dot_local/share/cargo/bin/symlink_cargo")
            .read_text()
            .strip()
        )
        (cargo_bin / "cargo").symlink_to(target)
        executable = self.real / "cargo"
        executable.write_text(f"#!{sys.executable}\n" + PROBE)
        executable.chmod(0o755)
        self.env["PATH"] = f"{cargo_bin}:{self.env['PATH']}"
        result = self.run_command("cargo", "+nightly", "--version", input="")
        self.assertEqual(result.returncode, 0, result.stderr)
        data = json.loads(result.stdout)
        self.assertEqual(data["args"], ["+nightly", "--version"])
        self.assertIn("/cargo.slice/cargo-", data["cgroup"])

    def test_exit_status_and_missing_command(self):
        result = self.run_command(self.helper, "/bin/sh", "-c", "exit 37")
        self.assertEqual(result.returncode, 37, result.stderr)
        missing = self.run_command(self.helper, "build-scope-test-nonexistent")
        self.assertEqual(missing.returncode, 127)
        self.assertIn("command not found", missing.stderr)

    def test_inherited_jobserver_descriptors(self):
        read_fd, write_fd = os.pipe()
        try:
            os.write(write_fd, b"token")
            probe = f"import os; assert os.read({read_fd}, 5) == b'token'"
            result = self.run_command(
                self.helper,
                sys.executable,
                "-c",
                probe,
                pass_fds=(read_fd, write_fd),
            )
            self.assertEqual(result.returncode, 0, result.stderr)
        finally:
            os.close(read_fd)
            os.close(write_fd)


if __name__ == "__main__":
    unittest.main()
