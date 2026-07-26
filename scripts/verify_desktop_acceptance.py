#!/usr/bin/env python3
"""Fail unless the committed v1 desktop acceptance evidence is complete and current.

Evidence model (ADR-0005):
- Windows: full desktop human matrix + performance samples.
- macOS / Linux: GitHub Actions attestation (quality / package smoke), not personal desktops.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[1]
RESULT = ROOT / "docs/testing/results/v1-desktop-acceptance.json"
DESKTOP_PLATFORMS = ("windows",)
CI_PLATFORMS = ("macos", "linux-x11", "linux-wayland")
PLATFORMS = DESKTOP_PLATFORMS + CI_PLATFORMS
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
}
CI_CHECKS = {
    "macos": {"ci-quality"},
    "linux-x11": {"ci-quality", "ci-deb-package-smoke"},
    "linux-wayland": {"ci-quality", "ci-deb-package-smoke"},
}
PERFORMANCE_LIMITS = {
    "cold_start_ms": ("max", 1500.0),
    "open_50mp_ms": ("max", 2000.0),
    "beautify_preview_ms": ("max", 200.0),
    "batch_click_response_ms": ("max", 100.0),
    "batch_progress_hz": ("min", 1.0),
}
ENV_FIELDS = ("system_version", "gpu", "display", "tester", "date")


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def require_ci_run_url(platform: str, value: object, errors: list[str]) -> None:
    if not isinstance(value, str) or not value.strip():
        errors.append(f"{platform}: ci_run_url is required for github-actions evidence")
        return
    parsed = urlparse(value)
    if parsed.scheme not in {"http", "https"} or "github.com" not in parsed.netloc:
        errors.append(f"{platform}: ci_run_url must be a github.com Actions run URL")
        return
    if "/actions/runs/" not in parsed.path:
        errors.append(f"{platform}: ci_run_url must point at an Actions run")


def validate_performance(platform_key: str, record: object, errors: list[str]) -> None:
    if not isinstance(record, dict):
        errors.append(f"{platform_key}: performance record must be an object")
        return
    for metric, (mode, limit) in PERFORMANCE_LIMITS.items():
        samples = record.get(metric, [])
        if not isinstance(samples, list) or len(samples) < 3:
            errors.append(f"{platform_key}: {metric} requires at least three samples")
            continue
        if not all(isinstance(value, (int, float)) for value in samples):
            errors.append(f"{platform_key}: {metric} samples must be numeric")
            continue
        observed = max(samples) if mode == "max" else min(samples)
        passed = observed <= limit if mode == "max" else observed >= limit
        if not passed:
            relation = "<=" if mode == "max" else ">="
            errors.append(
                f"{platform_key}: {metric} observed {observed}, requires {relation} {limit}"
            )


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
    if not isinstance(platform_results, dict):
        errors.append("platforms must be an object")
        platform_results = {}

    for platform in PLATFORMS:
        result = platform_results.get(platform, {})
        if not isinstance(result, dict):
            errors.append(f"{platform}: platform record must be an object")
            continue
        if result.get("result") != "pass":
            errors.append(f"{platform}: result must be 'pass'")
        for field in ENV_FIELDS:
            if not result.get(field):
                errors.append(f"{platform}: {field} is required")

        evidence = result.get("evidence", "desktop" if platform == "windows" else "")
        checks = set(result.get("checks", []))

        if platform in DESKTOP_PLATFORMS:
            if evidence != "desktop":
                errors.append(f"{platform}: evidence must be 'desktop'")
            missing = sorted((COMMON_CHECKS | {PLATFORM_CHECK[platform]}) - checks)
            if missing:
                errors.append(f"{platform}: missing checks: {', '.join(missing)}")
        elif platform in CI_PLATFORMS:
            if evidence != "github-actions":
                errors.append(f"{platform}: evidence must be 'github-actions' (ADR-0005)")
            require_ci_run_url(platform, result.get("ci_run_url"), errors)
            missing = sorted(CI_CHECKS[platform] - checks)
            if missing:
                errors.append(f"{platform}: missing checks: {', '.join(missing)}")
        else:
            errors.append(f"{platform}: unsupported platform")

    performance = data.get("performance", {})
    if not isinstance(performance, dict):
        errors.append("performance must be an object")
        performance = {}

    # Windows performance is the hard release gate. Optional macOS/Linux samples
    # are validated only when present with at least one metric list filled.
    validate_performance("windows", performance.get("windows", {}), errors)
    for platform_key in ("macos", "linux"):
        record = performance.get(platform_key, {})
        if not isinstance(record, dict):
            errors.append(f"{platform_key}: performance record must be an object")
            continue
        if any(record.get(metric) for metric in PERFORMANCE_LIMITS):
            validate_performance(platform_key, record, errors)

    packages = data.get("package_checks", {})
    if not isinstance(packages, dict):
        errors.append("package_checks must be an object")
        packages = {}
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
