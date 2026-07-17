#!/usr/bin/env python3
"""Fail unless the committed v1 desktop acceptance evidence is complete and current."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RESULT = ROOT / "docs/testing/results/v1-desktop-acceptance.json"
PLATFORMS = ("windows", "macos", "linux-x11", "linux-wayland")
COMMON_CHECKS = {
    "navigation-placeholders",
    "runtime-language-switch",
    "config-persistence-recovery",
    "file-dialog-drop-clipboard",
    "background-responsiveness",
    "open-output-directory",
    "localized-error-categories",
    "edit-page",
    "collage-page",
    "batch-page",
    "slice-page",
    "qr-page",
    "exif-page",
    "beautify-page",
    "gif-page",
}
PLATFORM_CHECK = {
    "windows": "windows-dx11-directwrite-installer",
    "macos": "macos-metal-gatekeeper",
    "linux-x11": "linux-x11-integration",
    "linux-wayland": "linux-wayland-integration",
}
PERFORMANCE_LIMITS = {
    "cold_start_ms": ("max", 1500.0),
    "open_50mp_ms": ("max", 2000.0),
    "beautify_preview_ms": ("max", 200.0),
    "batch_click_response_ms": ("max", 100.0),
    "batch_progress_hz": ("min", 1.0),
}


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def main() -> int:
    errors: list[str] = []
    if not RESULT.is_file():
        print(f"missing desktop acceptance evidence: {RESULT}", file=sys.stderr)
        return 1

    try:
        data = json.loads(RESULT.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"invalid desktop acceptance evidence: {error}", file=sys.stderr)
        return 1

    source_commit = data.get("source_commit", "")
    if not isinstance(source_commit, str) or len(source_commit) != 40:
        errors.append("source_commit must be a full 40-character Git SHA")
    else:
        try:
            git("merge-base", "--is-ancestor", source_commit, "HEAD")
            changed = git(
                "diff",
                "--name-only",
                source_commit,
                "HEAD",
                "--",
                "Cargo.toml",
                "Cargo.lock",
                "crates",
                "packaging",
                "vendor",
                ".github",
            )
            if changed:
                errors.append(
                    "acceptance evidence is stale; release-affecting files changed after "
                    f"{source_commit}: {changed.replace(chr(10), ', ')}"
                )
        except subprocess.CalledProcessError:
            errors.append(f"source_commit {source_commit!r} is not an ancestor of HEAD")

    resources = data.get("resources", {})
    for field in ("generated_sha256", "exif_photo_sha256"):
        value = resources.get(field, "")
        if not isinstance(value, str) or len(value) != 64:
            errors.append(f"resources.{field} must be a 64-character SHA-256 digest")

    platform_results = data.get("platforms", {})
    for platform in PLATFORMS:
        result = platform_results.get(platform, {})
        if result.get("result") != "pass":
            errors.append(f"{platform}: result must be 'pass'")
        for field in ("system_version", "gpu", "display", "tester", "date"):
            if not result.get(field):
                errors.append(f"{platform}: {field} is required")
        checks = set(result.get("checks", []))
        missing = sorted((COMMON_CHECKS | {PLATFORM_CHECK[platform]}) - checks)
        if missing:
            errors.append(f"{platform}: missing checks: {', '.join(missing)}")

    performance = data.get("performance", {})
    for platform in ("windows", "macos", "linux"):
        record = performance.get(platform, {})
        for metric, (mode, limit) in PERFORMANCE_LIMITS.items():
            samples = record.get(metric, [])
            if not isinstance(samples, list) or len(samples) < 3:
                errors.append(f"{platform}: {metric} requires at least three samples")
                continue
            if not all(isinstance(value, (int, float)) for value in samples):
                errors.append(f"{platform}: {metric} samples must be numeric")
                continue
            observed = max(samples) if mode == "max" else min(samples)
            passed = observed <= limit if mode == "max" else observed >= limit
            if not passed:
                relation = "<=" if mode == "max" else ">="
                errors.append(
                    f"{platform}: {metric} observed {observed}, requires {relation} {limit}"
                )

    packages = data.get("package_checks", {})
    for platform in ("windows", "macos", "linux"):
        if packages.get(platform) is not True:
            errors.append(f"{platform}: package_checks must be true")

    if errors:
        print("desktop acceptance evidence is incomplete:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"desktop acceptance evidence verified: {RESULT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
