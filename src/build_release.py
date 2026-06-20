#!/usr/bin/env python3
"""Build the Rust project in release mode and open the binary folder."""

from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parent


def run_command(command: list[str]) -> subprocess.CompletedProcess[str]:
    print(f"$ {' '.join(command)}", flush=True)
    return subprocess.run(command, cwd=PROJECT_ROOT, text=True)


def cargo_target_directory() -> Path:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=PROJECT_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    return Path(metadata["target_directory"])


def release_binary_directory() -> Path:
    target_dir = cargo_target_directory()
    triple = os.environ.get("CARGO_BUILD_TARGET")
    if triple:
        return target_dir / triple / "release"
    return target_dir / "release"


def open_directory(path: Path) -> None:
    system = platform.system()
    if system == "Windows":
        os.startfile(path)  # type: ignore[attr-defined]
        return

    if system == "Linux":
        for opener in ("xdg-open", "gio"):
            executable = shutil.which(opener)
            if executable is None:
                continue

            command = [executable, str(path)]
            if opener == "gio":
                command.insert(1, "open")
            subprocess.Popen(
                command,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            return

    raise RuntimeError(f"unsupported or unavailable file manager: {system}")


def main() -> int:
    if shutil.which("cargo") is None:
        print("error: cargo executable was not found in PATH", file=sys.stderr)
        return 1

    build = run_command(["cargo", "build", "--release"])
    if build.returncode != 0:
        return build.returncode

    try:
        release_dir = release_binary_directory()
    except (subprocess.CalledProcessError, json.JSONDecodeError, KeyError) as error:
        print(f"error: failed to locate Cargo target directory: {error}", file=sys.stderr)
        return 1

    if not release_dir.is_dir():
        print(f"error: release binary directory was not found: {release_dir}", file=sys.stderr)
        return 1

    print(f"release binary directory: {release_dir}")
    try:
        open_directory(release_dir)
    except OSError as error:
        print(f"warning: failed to open release directory: {error}", file=sys.stderr)
    except RuntimeError as error:
        print(f"warning: {error}", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
