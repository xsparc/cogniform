#!/usr/bin/env python3
"""Disposable tests for the exact first-party package policy."""

from __future__ import annotations

import importlib
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_ROOT = REPOSITORY_ROOT / "scripts"
POLICY_TOOL = SCRIPTS_ROOT / "check_package_policy.py"
EXPECTED_VERSION = "0.1.0-rc.1"
FIXTURE_MEMBER_PACKAGES = {
    "apps/cogniform-cli": "cogniform-cli",
    "crates/cogniform-core": "cogniform-core",
}
sys.path.insert(0, str(SCRIPTS_ROOT))
package_policy = importlib.import_module("check_package_policy")


ROOT_MANIFEST = """[workspace]
resolver = "3"
members = ["apps/cogniform-cli", "crates/cogniform-core"]

[workspace.package]
version = "0.1.0-rc.1"

[workspace.dependencies]
cogniform-core = { version = "=0.1.0-rc.1", path = "crates/cogniform-core" }
"""

CORE_MANIFEST = """[package]
name = "cogniform-core"
version.workspace = true
publish = false
"""

CLI_MANIFEST = """[package]
name = "cogniform-cli"
version.workspace = true
publish = false

[dependencies]
cogniform-core.workspace = true
"""

LOCKFILE = """version = 4

[[package]]
name = "cogniform-cli"
version = "0.1.0-rc.1"
dependencies = ["cogniform-core"]

[[package]]
name = "cogniform-core"
version = "0.1.0-rc.1"
"""

LOCKED_CORE = """
[[package]]
name = "cogniform-core"
version = "0.1.0-rc.1"
"""


def write(root: Path, relative_path: str, content: str) -> None:
    path = root / relative_path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8", newline="\n")


def fixture(root: Path) -> None:
    write(root, "Cargo.toml", ROOT_MANIFEST)
    write(root, "Cargo.lock", LOCKFILE)
    write(root, "crates/cogniform-core/Cargo.toml", CORE_MANIFEST)
    write(root, "apps/cogniform-cli/Cargo.toml", CLI_MANIFEST)


def expect_failure(
    parent: Path,
    name: str,
    code: str,
    mutate: Callable[[Path], None],
) -> None:
    root = parent / name
    root.mkdir()
    fixture(root)
    mutate(root)
    try:
        package_policy.check_repository(root, EXPECTED_VERSION, FIXTURE_MEMBER_PACKAGES)
    except package_policy.PackagePolicyError as error:
        if error.code != code:
            raise AssertionError(f"expected {code}, received {error.code}") from error
    else:
        raise AssertionError(f"expected {code} rejection")


def replace(path: Path, old: str, new: str) -> None:
    content = path.read_text(encoding="utf-8")
    if old not in content:
        raise AssertionError(f"fixture does not contain {old!r}")
    path.write_text(content.replace(old, new, 1), encoding="utf-8", newline="\n")


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="cogniform-package-policy-") as temporary:
        parent = Path(temporary).resolve()
        happy = parent / "happy"
        happy.mkdir()
        fixture(happy)
        summary = package_policy.check_repository(
            happy, EXPECTED_VERSION, FIXTURE_MEMBER_PACKAGES
        )
        if summary != package_policy.PackagePolicySummary(EXPECTED_VERSION, 2, 1):
            raise AssertionError("unexpected happy-path summary")

        exact_manifest_limit = parent / "exact-manifest-limit"
        exact_manifest_limit.mkdir()
        fixture(exact_manifest_limit)
        write(
            exact_manifest_limit,
            "Cargo.toml",
            ROOT_MANIFEST
            + "#"
            * (package_policy.MAX_MANIFEST_BYTES - len(ROOT_MANIFEST.encode("utf-8"))),
        )
        package_policy.check_repository(
            exact_manifest_limit, EXPECTED_VERSION, FIXTURE_MEMBER_PACKAGES
        )

        expect_failure(
            parent,
            "workspace-version",
            "workspace-version-mismatch",
            lambda root: replace(root / "Cargo.toml", EXPECTED_VERSION, "0.1.0-rc.2"),
        )
        expect_failure(
            parent,
            "member-set",
            "workspace-member-set-mismatch",
            lambda root: replace(
                root / "Cargo.toml", ', "crates/cogniform-core"', ""
            ),
        )
        expect_failure(
            parent,
            "package-name",
            "package-name-mismatch",
            lambda root: replace(
                root / "crates/cogniform-core/Cargo.toml",
                'name = "cogniform-core"',
                'name = "cogniform-renamed"',
            ),
        )
        expect_failure(
            parent,
            "member-version",
            "member-version-not-inherited",
            lambda root: replace(
                root / "crates/cogniform-core/Cargo.toml",
                "version.workspace = true",
                f'version = "{EXPECTED_VERSION}"',
            ),
        )
        expect_failure(
            parent,
            "publishable",
            "package-publishable",
            lambda root: replace(
                root / "apps/cogniform-cli/Cargo.toml", "publish = false", "publish = true"
            ),
        )
        expect_failure(
            parent,
            "dependency-version",
            "workspace-dependency-version-mismatch",
            lambda root: replace(root / "Cargo.toml", "=0.1.0-rc.1", "0.1.0-rc.1"),
        )
        expect_failure(
            parent,
            "dependency-path",
            "workspace-dependency-path-mismatch",
            lambda root: replace(
                root / "Cargo.toml",
                'cogniform-core = { version = "=0.1.0-rc.1", path = "crates/cogniform-core" }',
                'cogniform-core = { version = "=0.1.0-rc.1", path = "apps/cogniform-cli" }',
            ),
        )
        expect_failure(
            parent,
            "dependency-set",
            "workspace-dependency-set-mismatch",
            lambda root: replace(
                root / "Cargo.toml",
                'cogniform-core = { version = "=0.1.0-rc.1", path = "crates/cogniform-core" }\n',
                "",
            ),
        )
        expect_failure(
            parent,
            "direct-member-dependency",
            "dependency-not-inherited",
            lambda root: replace(
                root / "apps/cogniform-cli/Cargo.toml",
                "cogniform-core.workspace = true",
                'cogniform-core = { path = "../../crates/cogniform-core" }',
            ),
        )
        expect_failure(
            parent,
            "aliased-member-dependency",
            "dependency-overrides-workspace",
            lambda root: replace(
                root / "apps/cogniform-cli/Cargo.toml",
                "cogniform-core.workspace = true",
                'core-alias = { workspace = true, package = "cogniform-core" }',
            ),
        )
        expect_failure(
            parent,
            "member-depends-on-app",
            "missing-workspace-dependency",
            lambda root: replace(
                root / "crates/cogniform-core/Cargo.toml",
                "publish = false",
                "publish = false\n\n[dependencies]\ncogniform-cli.workspace = true",
            ),
        )
        expect_failure(
            parent,
            "aliased-workspace-dependency",
            "workspace-dependency-package-mismatch",
            lambda root: replace(
                root / "Cargo.toml",
                "cogniform-core = {",
                'core-alias = { package = "cogniform-core",',
            ),
        )
        expect_failure(
            parent,
            "lock-version",
            "lock-version-mismatch",
            lambda root: replace(root / "Cargo.lock", EXPECTED_VERSION, "0.1.0-rc.2"),
        )
        expect_failure(
            parent,
            "lock-set",
            "lock-package-set-mismatch",
            lambda root: replace(
                root / "Cargo.lock",
                '[[package]]\nname = "cogniform-core"\nversion = "0.1.0-rc.1"\n',
                "",
            ),
        )
        expect_failure(
            parent,
            "lock-duplicate",
            "duplicate-lock-package",
            lambda root: write(root, "Cargo.lock", LOCKFILE + LOCKED_CORE),
        )
        expect_failure(
            parent,
            "lock-source",
            "first-party-lock-source",
            lambda root: replace(
                root / "Cargo.lock",
                'name = "cogniform-core"\nversion = "0.1.0-rc.1"',
                'name = "cogniform-core"\nversion = "0.1.0-rc.1"\nsource = "registry+https://example.invalid"',
            ),
        )
        expect_failure(
            parent,
            "unsafe-member",
            "invalid-member-path",
            lambda root: replace(root / "Cargo.toml", "crates/cogniform-core", "../outside"),
        )
        expect_failure(
            parent,
            "oversized-manifest",
            "file-size-exceeded",
            lambda root: write(
                root,
                "Cargo.toml",
                ROOT_MANIFEST
                + "#"
                * (package_policy.MAX_MANIFEST_BYTES - len(ROOT_MANIFEST.encode("utf-8")) + 1),
            ),
        )

    live_summary = package_policy.check_repository(REPOSITORY_ROOT, EXPECTED_VERSION)
    if live_summary.package_count != 16 or live_summary.workspace_dependency_count != 15:
        raise AssertionError("unexpected live workspace package inventory")
    result = subprocess.run(
        [
            sys.executable,
            str(POLICY_TOOL),
            "--repository",
            str(REPOSITORY_ROOT),
            "--expected-version",
            EXPECTED_VERSION,
        ],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0 or "package-policy: PASS" not in result.stdout:
        raise AssertionError(f"live package policy failed: {result.stdout}{result.stderr}")

    print("package policy tests: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
