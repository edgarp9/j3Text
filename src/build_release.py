#!/usr/bin/env python3
"""Build and package the Rust project in release mode."""

from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parent
PROJECT_LICENSE = PROJECT_ROOT / "LICENSE"
THIRD_PARTY_NOTICE = PROJECT_ROOT / "THIRD_PARTY_NOTICES.txt"
PROJECT_ABOUT = PROJECT_ROOT / "about.txt"
THIRD_PARTY_LICENSES = PROJECT_ROOT / "third_party_licenses"
RELEASE_NOTICE_FILES = (PROJECT_LICENSE, THIRD_PARTY_NOTICE, PROJECT_ABOUT)
OBSOLETE_RELEASE_NOTICE_NAMES = ("THIRD-PARTY-NOTICES.md",)
SOURCE_ARCHIVE_EXCLUDED_DIRS = {
    ".git",
    ".my",
    ".idea",
    ".vscode",
    ".codex",
    "target",
    "dist",
    "coverage",
    "criterion",
    "__pycache__",
}
SOURCE_ARCHIVE_EXCLUDED_FILENAMES = {
    ".DS_Store",
    "Thumbs.db",
    "Desktop.ini",
    "cargo-tarpaulin-report.xml",
    "flamegraph.svg",
    "tarpaulin-report.html",
}
SOURCE_ARCHIVE_EXCLUDED_SUFFIXES = (
    ".bak",
    ".ilk",
    ".log",
    ".pdb",
    ".profdata",
    ".profraw",
    ".pyc",
    ".rlib",
    ".rmeta",
    ".swo",
    ".swp",
    ".tmp",
)


def run_command(command: list[str]) -> subprocess.CompletedProcess[str]:
    print(f"$ {' '.join(command)}", flush=True)
    return subprocess.run(command, cwd=PROJECT_ROOT, text=True)


def cargo_metadata() -> dict[str, object]:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=PROJECT_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def cargo_target_directory(metadata: dict[str, object]) -> Path:
    return Path(str(metadata["target_directory"]))


def cargo_package_name_version(metadata: dict[str, object]) -> tuple[str, str]:
    packages = metadata["packages"]
    if not isinstance(packages, list) or not packages:
        raise KeyError("packages")

    package = packages[0]
    if not isinstance(package, dict):
        raise KeyError("packages[0]")

    name = package["name"]
    version = package["version"]
    if not isinstance(name, str) or not isinstance(version, str):
        raise KeyError("package name/version")

    return name, version


def release_target_label() -> str:
    triple = os.environ.get("CARGO_BUILD_TARGET")
    if triple:
        return triple

    system = platform.system().lower() or "unknown"
    machine = platform.machine().lower() or "unknown"
    return f"{system}-{machine}"


def release_binary_directory(metadata: dict[str, object]) -> Path:
    target_dir = cargo_target_directory(metadata)
    triple = os.environ.get("CARGO_BUILD_TARGET")
    if triple:
        return target_dir / triple / "release"
    return target_dir / "release"


def release_binary_path(release_dir: Path, package_name: str) -> Path:
    candidates = [
        release_dir / f"{package_name}.exe",
        release_dir / package_name,
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise FileNotFoundError(f"release binary was not found for package {package_name}")


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


def copy_license_notices(release_dir: Path) -> bool:
    for obsolete_name in OBSOLETE_RELEASE_NOTICE_NAMES:
        obsolete_path = release_dir / obsolete_name
        if obsolete_path.is_file():
            obsolete_path.unlink()
            print(f"removed obsolete {obsolete_name} from release directory")

    missing = False
    for notice_file in RELEASE_NOTICE_FILES:
        if notice_file.is_file():
            shutil.copy2(notice_file, release_dir / notice_file.name)
            print(f"copied {notice_file.name} to release directory")
        else:
            missing = True
            print(f"error: missing {notice_file.name}", file=sys.stderr)

    if THIRD_PARTY_LICENSES.is_dir():
        destination = release_dir / THIRD_PARTY_LICENSES.name
        shutil.copytree(THIRD_PARTY_LICENSES, destination, dirs_exist_ok=True)
        print(f"copied {THIRD_PARTY_LICENSES.name} to release directory")
    else:
        missing = True
        print(f"error: missing {THIRD_PARTY_LICENSES.name}", file=sys.stderr)

    return not missing


def should_exclude_source_file(path: Path) -> bool:
    if path.is_symlink():
        return True

    relative = path.relative_to(PROJECT_ROOT)
    if any(part in SOURCE_ARCHIVE_EXCLUDED_DIRS for part in relative.parts):
        return True

    name = path.name
    if name in SOURCE_ARCHIVE_EXCLUDED_FILENAMES or name.endswith("~"):
        return True

    return name.endswith(SOURCE_ARCHIVE_EXCLUDED_SUFFIXES)


def create_corresponding_source_archive(
    release_dir: Path, package_name: str, package_version: str
) -> Path:
    archive_path = release_dir / f"{package_name}-{package_version}-corresponding-source.zip"
    archive_root = f"{package_name}-{package_version}"

    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for current_dir, dirnames, filenames in os.walk(PROJECT_ROOT):
            dirnames[:] = sorted(
                dirname
                for dirname in dirnames
                if dirname not in SOURCE_ARCHIVE_EXCLUDED_DIRS
                and not dirname.startswith(".codex")
                and not dirname.startswith("codex")
            )

            current_path = Path(current_dir)
            for filename in sorted(filenames):
                path = current_path / filename
                if should_exclude_source_file(path):
                    continue

                relative = path.relative_to(PROJECT_ROOT)
                archive.write(path, (Path(archive_root) / relative).as_posix())

    print(f"created GPL corresponding source archive: {archive_path.name}")
    return archive_path


def add_file_to_archive(archive: zipfile.ZipFile, path: Path, archive_name: Path) -> None:
    archive.write(path, archive_name.as_posix())


def add_directory_to_archive(archive: zipfile.ZipFile, directory: Path, archive_root: Path) -> None:
    for current_dir, dirnames, filenames in os.walk(directory):
        dirnames[:] = sorted(dirnames)
        current_path = Path(current_dir)
        for filename in sorted(filenames):
            path = current_path / filename
            relative = path.relative_to(directory)
            add_file_to_archive(archive, path, archive_root / relative)


def create_binary_archive(
    release_dir: Path, package_name: str, package_version: str
) -> Path:
    binary_path = release_binary_path(release_dir, package_name)
    archive_path = release_dir / (
        f"{package_name}-{package_version}-{release_target_label()}-binary.zip"
    )

    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        add_file_to_archive(archive, binary_path, Path(binary_path.name))
        for notice_file in RELEASE_NOTICE_FILES:
            add_file_to_archive(archive, release_dir / notice_file.name, Path(notice_file.name))
        add_directory_to_archive(
            archive,
            release_dir / THIRD_PARTY_LICENSES.name,
            Path(THIRD_PARTY_LICENSES.name),
        )

    print(f"created binary archive: {archive_path.name}")
    return archive_path


def main() -> int:
    if shutil.which("cargo") is None:
        print("error: cargo executable was not found in PATH", file=sys.stderr)
        return 1

    build = run_command(["cargo", "build", "--release"])
    if build.returncode != 0:
        return build.returncode

    try:
        metadata = cargo_metadata()
        release_dir = release_binary_directory(metadata)
        package_name, package_version = cargo_package_name_version(metadata)
    except (subprocess.CalledProcessError, json.JSONDecodeError, KeyError) as error:
        print(f"error: failed to read Cargo metadata: {error}", file=sys.stderr)
        return 1

    if not release_dir.is_dir():
        print(f"error: release binary directory was not found: {release_dir}", file=sys.stderr)
        return 1

    if not copy_license_notices(release_dir):
        return 1
    try:
        create_corresponding_source_archive(release_dir, package_name, package_version)
        create_binary_archive(release_dir, package_name, package_version)
    except OSError as error:
        print(f"error: failed to create release archive: {error}", file=sys.stderr)
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
