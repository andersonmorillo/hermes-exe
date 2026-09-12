#!/usr/bin/env python3
"""Python tooling for Hermes operations and release workflow."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
REQUIRED_RUNTIME_FILES = (
    "whisper-cli.exe",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-cpu.dll",
)
MODEL_VARIANTS = {
    "tiny.en": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
    "base.en": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
    "small.en": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
    "medium.en": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
    "large-v3": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
}
WHISPER_RELEASES_API = "https://api.github.com/repos/ggml-org/whisper.cpp/releases"
RUNTIME_ARCHIVE_BY_MACHINE = {
    "amd64": "whisper-bin-x64.zip",
    "x86_64": "whisper-bin-x64.zip",
    "x64": "whisper-bin-x64.zip",
    "i386": "whisper-bin-Win32.zip",
    "i686": "whisper-bin-Win32.zip",
    "x86": "whisper-bin-Win32.zip",
}


class ToolingError(RuntimeError):
    """Raised for user-fixable tooling failures."""


def _repo_path(path_text: str) -> Path:
    path = Path(path_text)
    if path.is_absolute():
        return path
    return (REPO_ROOT / path).resolve()


def _default_model_path() -> Path:
    local_app_data = os.environ.get("LOCALAPPDATA")
    if not local_app_data:
        raise ToolingError("LOCALAPPDATA is not set; cannot resolve default model path.")
    return Path(local_app_data) / "Hermes" / "Hermes" / "data" / "models" / "ggml-medium.en.bin"


def _default_startup_shortcut() -> Path:
    app_data = os.environ.get("APPDATA")
    if not app_data:
        raise ToolingError("APPDATA is not set; cannot resolve Startup shortcut path.")
    return (
        Path(app_data)
        / "Microsoft"
        / "Windows"
        / "Start Menu"
        / "Programs"
        / "Startup"
        / "Hermes.lnk"
    )


def _default_runtime_asset_name() -> str:
    machine = platform.machine().lower()
    return RUNTIME_ARCHIVE_BY_MACHINE.get(machine, "whisper-bin-x64.zip")


def _optional_requests():
    try:
        import requests
    except ImportError:
        return None
    return requests


def _read_json(url: str, headers: dict[str, str] | None = None) -> object:
    try:
        requests = _optional_requests()
        if requests is not None:
            response = requests.get(url, headers=headers, timeout=60)
            response.raise_for_status()
            return response.json()

        request = urllib.request.Request(url, headers=headers or {})
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.loads(response.read().decode("utf-8"))
    except Exception as error:
        raise ToolingError(f"Failed to fetch JSON from {url}: {error}") from error


def _run_command(command: list[str], cwd: Path | None = None) -> None:
    command_text = " ".join(command)
    print(f"$ {command_text}")
    subprocess.run(command, cwd=str(cwd or REPO_ROOT), check=True)


def _run_powershell_script(script_text: str, args: list[str]) -> None:
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".ps1", encoding="utf-8", delete=False
    ) as script_file:
        script_file.write(script_text)
        script_path = Path(script_file.name)
    try:
        _run_command(
            [
                "powershell.exe",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                str(script_path),
                *args,
            ],
            cwd=REPO_ROOT,
        )
    finally:
        script_path.unlink(missing_ok=True)


def _assert_runtime_dir(runtime_dir: Path) -> None:
    missing = [name for name in REQUIRED_RUNTIME_FILES if not (runtime_dir / name).exists()]
    if missing:
        missing_text = ", ".join(missing)
        raise ToolingError(
            f"Missing runtime files in {runtime_dir}: {missing_text}. "
            "Expected whisper runtime DLLs next to whisper-cli.exe. "
            "Use `ensure-runtime` to fetch the official whisper.cpp Windows runtime."
        )


def _copy_if_exists(source: Path, target: Path) -> None:
    if source.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def cmd_build(args: argparse.Namespace) -> None:
    _run_command(["cargo", "build", "--release"])
    print("Release build complete.")


def cmd_verify_runtime(args: argparse.Namespace) -> None:
    runtime_dir = _repo_path(args.runtime_dir)
    _assert_runtime_dir(runtime_dir)
    print(f"Runtime verification passed: {runtime_dir}")

    model_path = _repo_path(args.model_path) if args.model_path else _default_model_path()
    if model_path.exists():
        print(f"Model check passed: {model_path}")
    else:
        print(f"Model not found: {model_path}")
        print("Use `download-model` to fetch one.")


def _download_with_progress(
    url: str, destination: Path, headers: dict[str, str] | None = None
) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temp_path: Path | None = None
    try:
        requests = _optional_requests()
        if requests is not None:
            with requests.get(url, stream=True, headers=headers, timeout=60) as response:
                response.raise_for_status()
                total_bytes = int(response.headers.get("Content-Length", "0"))
                with tempfile.NamedTemporaryFile(
                    mode="wb", delete=False, dir=destination.parent
                ) as tmp_file:
                    downloaded = 0
                    for chunk in response.iter_content(chunk_size=1024 * 1024):
                        if not chunk:
                            continue
                        tmp_file.write(chunk)
                        downloaded += len(chunk)
                        _print_download_progress(downloaded, total_bytes)
                    temp_path = Path(tmp_file.name)
        else:
            request = urllib.request.Request(url, headers=headers or {})
            with urllib.request.urlopen(request, timeout=60) as response:
                total_bytes = int(response.headers.get("Content-Length", "0"))
                with tempfile.NamedTemporaryFile(
                    mode="wb", delete=False, dir=destination.parent
                ) as tmp_file:
                    downloaded = 0
                    while True:
                        chunk = response.read(1024 * 1024)
                        if not chunk:
                            break
                        tmp_file.write(chunk)
                        downloaded += len(chunk)
                        _print_download_progress(downloaded, total_bytes)
                    temp_path = Path(tmp_file.name)
    except Exception as error:
        if temp_path is not None:
            temp_path.unlink(missing_ok=True)
        raise ToolingError(f"Failed to download {url}: {error}") from error

    print()
    if temp_path is None:
        raise ToolingError(f"Download did not produce a file for {url}.")
    temp_path.replace(destination)


def _print_download_progress(downloaded: int, total_bytes: int) -> None:
    if total_bytes > 0:
        percent = downloaded * 100 / total_bytes
        print(
            f"\rDownloaded {downloaded:,} / {total_bytes:,} bytes ({percent:0.1f}%)",
            end="",
        )
    else:
        print(f"\rDownloaded {downloaded:,} bytes", end="")


def _fetch_whisper_release(tag: str | None) -> dict[str, object]:
    if tag:
        url = f"{WHISPER_RELEASES_API}/tags/{tag}"
    else:
        url = f"{WHISPER_RELEASES_API}/latest"

    payload = _read_json(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "Hermes Runtime Bootstrap",
        },
    )
    if not isinstance(payload, dict):
        raise ToolingError(f"Unexpected GitHub API response from {url}.")
    return payload


def _select_runtime_asset(
    release: dict[str, object], asset_name: str
) -> dict[str, object]:
    assets = release.get("assets")
    if not isinstance(assets, list):
        raise ToolingError("GitHub release response did not include an asset list.")

    for asset in assets:
        if isinstance(asset, dict) and asset.get("name") == asset_name:
            return asset

    available_assets = []
    for asset in assets:
        if isinstance(asset, dict):
            name = asset.get("name")
            if isinstance(name, str):
                available_assets.append(name)

    tag_name = release.get("tag_name", "unknown")
    available_text = ", ".join(sorted(available_assets)) or "(none)"
    raise ToolingError(
        f"Release {tag_name} does not contain asset {asset_name}. "
        f"Available assets: {available_text}"
    )


def _locate_runtime_payload_dir(root: Path) -> Path:
    matches = []
    for whisper_cli in root.rglob("whisper-cli.exe"):
        candidate = whisper_cli.parent
        if all((candidate / name).exists() for name in REQUIRED_RUNTIME_FILES[1:]):
            matches.append(candidate)

    if not matches:
        raise ToolingError(
            f"Downloaded archive did not contain a complete whisper runtime under {root}."
        )

    matches.sort(key=lambda path: (len(path.parts), str(path).lower()))
    return matches[0]


def _copy_runtime_payload(source_dir: Path, runtime_dir: Path) -> None:
    runtime_dir.mkdir(parents=True, exist_ok=True)
    for source in source_dir.iterdir():
        if source.is_file():
            shutil.copy2(source, runtime_dir / source.name)


def cmd_download_model(args: argparse.Namespace) -> None:
    if args.url:
        url = args.url
        filename = Path(urllib.parse.urlparse(url).path).name or "model.bin"
    else:
        url = MODEL_VARIANTS[args.variant]
        filename = Path(url).name

    output_path = _repo_path(args.output) if args.output else (_default_model_path().with_name(filename))

    if output_path.exists() and not args.force:
        raise ToolingError(
            f"Model already exists at {output_path}. Use --force to overwrite."
        )

    print(f"Downloading model from: {url}")
    _download_with_progress(url, output_path)
    print(f"Model saved to: {output_path}")


def cmd_ensure_runtime(args: argparse.Namespace) -> None:
    runtime_dir = _repo_path(args.runtime_dir)
    asset_name = args.asset_name or _default_runtime_asset_name()

    if not args.force:
        try:
            _assert_runtime_dir(runtime_dir)
            print(f"Runtime already available: {runtime_dir}")
            return
        except ToolingError:
            pass

    release = _fetch_whisper_release(args.tag)
    asset = _select_runtime_asset(release, asset_name)
    tag_name = release.get("tag_name", args.tag or "latest")
    download_url = asset.get("browser_download_url")
    if not isinstance(download_url, str) or not download_url:
        raise ToolingError(
            f"Release {tag_name} asset {asset_name} does not include a download URL."
        )

    print(f"Downloading whisper runtime from release {tag_name}: {asset_name}")
    with tempfile.TemporaryDirectory(prefix="hermes-whisper-runtime-") as temp_dir_text:
        temp_dir = Path(temp_dir_text)
        archive_path = temp_dir / asset_name
        extract_dir = temp_dir / "extract"

        _download_with_progress(
            download_url,
            archive_path,
            headers={
                "Accept": "application/octet-stream",
                "User-Agent": "Hermes Runtime Bootstrap",
            },
        )

        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(extract_dir)

        payload_dir = _locate_runtime_payload_dir(extract_dir)
        _copy_runtime_payload(payload_dir, runtime_dir)

    _assert_runtime_dir(runtime_dir)
    print(f"Runtime saved to: {runtime_dir}")


def cmd_install_startup(args: argparse.Namespace) -> None:
    exe_path = _repo_path(args.exe)
    if not exe_path.exists():
        raise ToolingError(f"Executable not found: {exe_path}")

    shortcut_path = _repo_path(args.shortcut) if args.shortcut else _default_startup_shortcut()
    shortcut_path.parent.mkdir(parents=True, exist_ok=True)

    script = """\
param(
    [Parameter(Mandatory = $true)][string]$ExePath,
    [Parameter(Mandatory = $true)][string]$ShortcutPath,
    [Parameter(Mandatory = $true)][string]$Arguments
)
$ErrorActionPreference = "Stop"
$resolvedExe = (Resolve-Path $ExePath).Path
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($ShortcutPath)
$shortcut.TargetPath = $resolvedExe
$shortcut.Arguments = $Arguments
$shortcut.WorkingDirectory = Split-Path $resolvedExe -Parent
$shortcut.IconLocation = "$resolvedExe,0"
$shortcut.WindowStyle = 7
$shortcut.Description = "Hermes startup launcher"
$shortcut.Save()
Write-Host "Startup shortcut installed at:"
Write-Host "  $ShortcutPath"
Write-Host "Launch target:"
Write-Host "  $resolvedExe $Arguments"
"""
    _run_powershell_script(script, [str(exe_path), str(shortcut_path), args.arguments])


def cmd_remove_startup(args: argparse.Namespace) -> None:
    shortcut_path = _repo_path(args.shortcut) if args.shortcut else _default_startup_shortcut()
    if not shortcut_path.exists():
        print(f"No startup shortcut found at: {shortcut_path}")
        return

    shortcut_path.unlink()
    print(f"Removed startup shortcut: {shortcut_path}")


def cmd_diagnose(args: argparse.Namespace) -> None:
    exe_path = _repo_path(args.exe)
    if not exe_path.exists():
        raise ToolingError(
            f"Executable not found: {exe_path}. Build first with `python scripts/ptt_tooling.py build`."
        )
    _run_command([str(exe_path), "--diagnose"], cwd=REPO_ROOT)


def cmd_package(args: argparse.Namespace) -> None:
    runtime_dir = _repo_path(args.runtime_dir)
    exe_path = _repo_path(args.exe)
    output_dir = _repo_path(args.output_dir)

    if not args.skip_build:
        _run_command(["cargo", "build", "--release"])

    if not exe_path.exists():
        raise ToolingError(f"Executable not found: {exe_path}")
    _assert_runtime_dir(runtime_dir)

    if output_dir.exists() and not args.no_clean:
        shutil.rmtree(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    shipped_exe = output_dir / exe_path.name
    shutil.copy2(exe_path, shipped_exe)
    shutil.copytree(runtime_dir, output_dir / runtime_dir.name, dirs_exist_ok=True)

    _copy_if_exists(REPO_ROOT / "README.md", output_dir / "README.md")
    _copy_if_exists(REPO_ROOT / "scripts" / "ptt_tooling.py", output_dir / "scripts" / "ptt_tooling.py")
    _copy_if_exists(REPO_ROOT / "scripts" / "install-startup.ps1", output_dir / "install-startup.ps1")
    _copy_if_exists(REPO_ROOT / "scripts" / "remove-startup.ps1", output_dir / "remove-startup.ps1")

    if args.model:
        model_path = _repo_path(args.model)
        if not model_path.exists():
            raise ToolingError(f"Model file not found: {model_path}")
        (output_dir / "models").mkdir(parents=True, exist_ok=True)
        shutil.copy2(model_path, output_dir / "models" / model_path.name)

    manifest = output_dir / "PACKAGE_CONTENTS.txt"
    lines = [
        f"Executable: {shipped_exe.name}",
        f"Runtime dir: {runtime_dir.name}",
        "Includes tooling: scripts/ptt_tooling.py",
        "Includes startup helper scripts: install-startup.ps1, remove-startup.ps1",
    ]
    if args.model:
        lines.append(f"Bundled model: models/{Path(args.model).name}")
    manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")

    print(f"Package folder ready: {output_dir}")

    if args.zip:
        archive_path = output_dir.with_suffix(".zip")
        if archive_path.exists():
            archive_path.unlink()
        shutil.make_archive(
            base_name=str(output_dir),
            format="zip",
            root_dir=str(output_dir.parent),
            base_dir=output_dir.name,
        )
        print(f"Package archive ready: {archive_path}")


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Hermes Python tooling for build, packaging, and operations."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    parser_build = subparsers.add_parser("build", help="Build the Rust runtime in release mode.")
    parser_build.set_defaults(func=cmd_build)

    parser_verify = subparsers.add_parser("verify-runtime", help="Verify whisper runtime files and model path.")
    parser_verify.add_argument("--runtime-dir", default="whisper-runtime", help="Runtime directory path.")
    parser_verify.add_argument("--model-path", help="Model file path (defaults to LOCALAPPDATA model path).")
    parser_verify.set_defaults(func=cmd_verify_runtime)

    parser_download = subparsers.add_parser("download-model", help="Download a whisper.cpp model file.")
    parser_download.add_argument(
        "--variant",
        choices=sorted(MODEL_VARIANTS.keys()),
        default="medium.en",
        help="Known model variant to download.",
    )
    parser_download.add_argument("--url", help="Direct model URL (overrides --variant).")
    parser_download.add_argument(
        "--output",
        help="Output path (defaults to LOCALAPPDATA model path with the downloaded filename).",
    )
    parser_download.add_argument("--force", action="store_true", help="Overwrite existing model file.")
    parser_download.set_defaults(func=cmd_download_model)

    parser_runtime = subparsers.add_parser(
        "ensure-runtime",
        help="Download and extract the whisper.cpp Windows runtime when it is missing.",
    )
    parser_runtime.add_argument(
        "--runtime-dir",
        default="whisper-runtime",
        help="Directory where whisper-cli.exe and runtime DLLs should be placed.",
    )
    parser_runtime.add_argument(
        "--tag",
        help="Specific whisper.cpp release tag to download (defaults to the latest release).",
    )
    parser_runtime.add_argument(
        "--asset-name",
        help="Specific release asset name to download (defaults to whisper-bin-x64.zip on 64-bit Windows).",
    )
    parser_runtime.add_argument(
        "--force",
        action="store_true",
        help="Redownload the runtime even if the required files already exist.",
    )
    parser_runtime.set_defaults(func=cmd_ensure_runtime)

    parser_install = subparsers.add_parser("install-startup", help="Create a Startup shortcut for the executable.")
    parser_install.add_argument(
        "--exe",
        default="target/release/hermes.exe",
        help="Executable path to launch at startup.",
    )
    parser_install.add_argument("--shortcut", help="Shortcut path (defaults to user Startup folder).")
    parser_install.add_argument(
        "--arguments",
        default="--background",
        help="Arguments passed when launched from Startup.",
    )
    parser_install.set_defaults(func=cmd_install_startup)

    parser_remove = subparsers.add_parser("remove-startup", help="Remove a Startup shortcut.")
    parser_remove.add_argument("--shortcut", help="Shortcut path (defaults to user Startup folder).")
    parser_remove.set_defaults(func=cmd_remove_startup)

    parser_diagnose = subparsers.add_parser("diagnose", help="Run Hermes diagnostics via the built executable.")
    parser_diagnose.add_argument(
        "--exe",
        default="target/release/hermes.exe",
        help="Executable path used for diagnostics.",
    )
    parser_diagnose.set_defaults(func=cmd_diagnose)

    parser_package = subparsers.add_parser("package", help="Build and assemble a distributable folder (and optional zip).")
    parser_package.add_argument("--runtime-dir", default="whisper-runtime", help="Runtime directory path.")
    parser_package.add_argument(
        "--exe",
        default="target/release/hermes.exe",
        help="Executable path to package.",
    )
    parser_package.add_argument(
        "--output-dir",
        default="dist/hermes",
        help="Output directory for packaged files.",
    )
    parser_package.add_argument("--model", help="Optional model file to bundle.")
    parser_package.add_argument("--skip-build", action="store_true", help="Skip cargo release build.")
    parser_package.add_argument("--zip", action="store_true", help="Create a zip archive from output-dir.")
    parser_package.add_argument(
        "--no-clean",
        action="store_true",
        help="Do not remove output-dir before copying files.",
    )
    parser_package.set_defaults(func=cmd_package)

    return parser


def main() -> int:
    parser = _build_parser()
    args = parser.parse_args()
    try:
        args.func(args)
    except ToolingError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    except subprocess.CalledProcessError as error:
        print(f"error: command failed with exit code {error.returncode}", file=sys.stderr)
        return error.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
