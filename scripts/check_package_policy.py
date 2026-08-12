#!/usr/bin/env python3
"""Validate the first-party workspace release-version policy."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, Mapping


MAX_MANIFEST_BYTES = 1_048_576
MAX_LOCK_BYTES = 16_777_216
MAX_WORKSPACE_MEMBERS = 64
FIRST_PARTY_PREFIX = "cogniform-"
DEPENDENCY_TABLES = {"dependencies", "dev-dependencies", "build-dependencies"}
EXPECTED_MEMBER_PACKAGES = {
    "apps/cogniform-cli": "cogniform-cli",
    "crates/cogniform-assets": "cogniform-assets",
    "crates/cogniform-compilation": "cogniform-compilation",
    "crates/cogniform-compiler": "cogniform-compiler",
    "crates/cogniform-engine": "cogniform-engine",
    "crates/cogniform-local-executor": "cogniform-local-executor",
    "crates/cogniform-local-session": "cogniform-local-session",
    "crates/cogniform-local-transport": "cogniform-local-transport",
    "crates/cogniform-mcp": "cogniform-mcp",
    "crates/cogniform-observation": "cogniform-observation",
    "crates/cogniform-procedural": "cogniform-procedural",
    "crates/cogniform-protocol": "cogniform-protocol",
    "crates/cogniform-renderer": "cogniform-renderer",
    "crates/cogniform-replay": "cogniform-replay",
    "crates/cogniform-storage": "cogniform-storage",
    "crates/cogniform-world": "cogniform-world",
}
SEMVER_PATTERN = re.compile(
    r"(?:0|[1-9][0-9]*)\."
    r"(?:0|[1-9][0-9]*)\."
    r"(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?\Z"
)


class PackagePolicyError(Exception):
    """Stable package-policy rejection."""

    def __init__(self, code: str, path: str) -> None:
        super().__init__(code)
        self.code = code
        self.path = path


@dataclass(frozen=True)
class PackagePolicySummary:
    """Bounded successful policy summary."""

    version: str
    package_count: int
    workspace_dependency_count: int


def reject(code: str, path: str) -> None:
    """Raise one stable rejection without including file contents."""

    raise PackagePolicyError(code, path)


def safe_relative_path(value: object) -> str:
    """Return one normalized portable repository-relative path."""

    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > 512:
        reject("invalid-member-path", "Cargo.toml")
    if "\\" in value:
        reject("invalid-member-path", "Cargo.toml")
    candidate = PurePosixPath(value)
    if candidate.is_absolute() or any(part in {"", ".", ".."} for part in candidate.parts):
        reject("invalid-member-path", "Cargo.toml")
    normalized = candidate.as_posix()
    if normalized != value:
        reject("invalid-member-path", "Cargo.toml")
    return normalized


def read_toml(
    repository: Path,
    relative_path: str,
    byte_limit: int,
) -> dict[str, Any]:
    """Read one bounded regular TOML file within the repository."""

    path = repository.joinpath(*PurePosixPath(relative_path).parts)
    try:
        resolved = path.resolve(strict=True)
        resolved.relative_to(repository)
    except (OSError, ValueError):
        reject("missing-file", relative_path)
    if path.is_symlink() or not resolved.is_file():
        reject("invalid-file-type", relative_path)
    try:
        size = resolved.stat().st_size
    except OSError:
        reject("unreadable-file", relative_path)
    if size > byte_limit:
        reject("file-size-exceeded", relative_path)
    try:
        with resolved.open("rb") as file:
            data = file.read(byte_limit + 1)
            has_trailing_data = bool(file.read(1))
    except OSError:
        reject("unreadable-file", relative_path)
    if len(data) > byte_limit or has_trailing_data:
        reject("file-size-exceeded", relative_path)
    if len(data) != size:
        reject("file-size-changed", relative_path)
    try:
        decoded = data.decode("utf-8")
        document = tomllib.loads(decoded)
    except (UnicodeDecodeError, tomllib.TOMLDecodeError):
        reject("invalid-toml", relative_path)
    if not isinstance(document, dict):
        reject("invalid-toml", relative_path)
    return document


def required_table(document: dict[str, Any], key: str, path: str) -> dict[str, Any]:
    value = document.get(key)
    if not isinstance(value, dict):
        reject("missing-table", path)
    return value


def inherited_workspace_value(value: object) -> bool:
    return isinstance(value, dict) and value == {"workspace": True}


def validate_member_dependencies(
    manifest: dict[str, Any],
    member_names: set[str],
    workspace_dependency_names: set[str],
    manifest_path: str,
) -> None:
    """Reject first-party dependency declarations outside workspace policy."""

    scopes: list[dict[str, Any]] = [manifest]
    targets = manifest.get("target")
    if isinstance(targets, dict):
        scopes.extend(value for value in targets.values() if isinstance(value, dict))
    for scope in scopes:
        for table_name in DEPENDENCY_TABLES:
            table = scope.get(table_name)
            if table is None:
                continue
            if not isinstance(table, dict):
                reject("invalid-dependency-table", manifest_path)
            for dependency_name, declaration in table.items():
                package_name = (
                    declaration.get("package", dependency_name)
                    if isinstance(declaration, dict)
                    else dependency_name
                )
                if not (
                    dependency_name.startswith(FIRST_PARTY_PREFIX)
                    or isinstance(package_name, str)
                    and package_name.startswith(FIRST_PARTY_PREFIX)
                ):
                    continue
                if package_name not in member_names:
                    reject("unknown-first-party-dependency", manifest_path)
                if package_name not in workspace_dependency_names:
                    reject("missing-workspace-dependency", manifest_path)
                if not isinstance(declaration, dict) or declaration.get("workspace") is not True:
                    reject("dependency-not-inherited", manifest_path)
                if dependency_name != package_name or any(
                    key in declaration for key in ("path", "version", "package")
                ):
                    reject("dependency-overrides-workspace", manifest_path)


def check_repository(
    repository: Path,
    expected_version: str,
    expected_member_packages: Mapping[str, str] | None = None,
) -> PackagePolicySummary:
    """Validate package and lock policy for one candidate workspace."""

    if not SEMVER_PATTERN.fullmatch(expected_version):
        reject("invalid-expected-version", "<argument>")
    try:
        root = repository.resolve(strict=True)
    except OSError:
        reject("invalid-repository", "<repository>")
    if not root.is_dir():
        reject("invalid-repository", "<repository>")

    root_manifest = read_toml(root, "Cargo.toml", MAX_MANIFEST_BYTES)
    workspace = required_table(root_manifest, "workspace", "Cargo.toml")
    workspace_package = required_table(workspace, "package", "Cargo.toml")
    if workspace_package.get("version") != expected_version:
        reject("workspace-version-mismatch", "Cargo.toml")

    raw_members = workspace.get("members")
    if not isinstance(raw_members, list) or not raw_members:
        reject("invalid-members", "Cargo.toml")
    if len(raw_members) > MAX_WORKSPACE_MEMBERS:
        reject("member-limit-exceeded", "Cargo.toml")
    members = [safe_relative_path(value) for value in raw_members]
    if len(set(members)) != len(members):
        reject("duplicate-member", "Cargo.toml")
    expected_members = dict(
        EXPECTED_MEMBER_PACKAGES
        if expected_member_packages is None
        else expected_member_packages
    )
    if set(members) != set(expected_members):
        reject("workspace-member-set-mismatch", "Cargo.toml")

    member_documents: dict[str, dict[str, Any]] = {}
    member_paths_by_name: dict[str, str] = {}
    for member in members:
        manifest_path = f"{member}/Cargo.toml"
        manifest = read_toml(root, manifest_path, MAX_MANIFEST_BYTES)
        package = required_table(manifest, "package", manifest_path)
        name = package.get("name")
        if not isinstance(name, str) or not name.startswith(FIRST_PARTY_PREFIX):
            reject("invalid-package-name", manifest_path)
        if name != expected_members[member]:
            reject("package-name-mismatch", manifest_path)
        if name in member_paths_by_name:
            reject("duplicate-package-name", manifest_path)
        if not inherited_workspace_value(package.get("version")):
            reject("member-version-not-inherited", manifest_path)
        if package.get("publish") is not False:
            reject("package-publishable", manifest_path)
        member_documents[member] = manifest
        member_paths_by_name[name] = member

    member_names = set(member_paths_by_name)
    workspace_dependencies = required_table(workspace, "dependencies", "Cargo.toml")
    crate_members = {
        name: path for name, path in member_paths_by_name.items() if path.startswith("crates/")
    }
    declared_first_party: dict[str, object] = {}
    for dependency_name, declaration in workspace_dependencies.items():
        package_name = (
            declaration.get("package", dependency_name)
            if isinstance(declaration, dict)
            else dependency_name
        )
        if not (
            dependency_name.startswith(FIRST_PARTY_PREFIX)
            or isinstance(package_name, str)
            and package_name.startswith(FIRST_PARTY_PREFIX)
        ):
            continue
        if dependency_name != package_name:
            reject("workspace-dependency-package-mismatch", "Cargo.toml")
        declared_first_party[dependency_name] = declaration
    if set(declared_first_party) != set(crate_members):
        reject("workspace-dependency-set-mismatch", "Cargo.toml")
    for name, member in crate_members.items():
        declaration = declared_first_party[name]
        if not isinstance(declaration, dict):
            reject("invalid-workspace-dependency", "Cargo.toml")
        if declaration.get("version") != f"={expected_version}":
            reject("workspace-dependency-version-mismatch", "Cargo.toml")
        if declaration.get("path") != member:
            reject("workspace-dependency-path-mismatch", "Cargo.toml")
        if declaration.get("package", name) != name:
            reject("workspace-dependency-package-mismatch", "Cargo.toml")

    for member, manifest in member_documents.items():
        validate_member_dependencies(
            manifest,
            member_names,
            set(declared_first_party),
            f"{member}/Cargo.toml",
        )

    lock = read_toml(root, "Cargo.lock", MAX_LOCK_BYTES)
    packages = lock.get("package")
    if not isinstance(packages, list):
        reject("invalid-lockfile", "Cargo.lock")
    locked_by_name: dict[str, list[dict[str, Any]]] = {}
    for package in packages:
        if not isinstance(package, dict):
            reject("invalid-lockfile", "Cargo.lock")
        name = package.get("name")
        if isinstance(name, str) and name.startswith(FIRST_PARTY_PREFIX):
            locked_by_name.setdefault(name, []).append(package)
    if set(locked_by_name) != member_names:
        reject("lock-package-set-mismatch", "Cargo.lock")
    for name, locked_packages in locked_by_name.items():
        if len(locked_packages) != 1:
            reject("duplicate-lock-package", "Cargo.lock")
        package = locked_packages[0]
        if package.get("version") != expected_version:
            reject("lock-version-mismatch", "Cargo.lock")
        if "source" in package or "checksum" in package:
            reject("first-party-lock-source", "Cargo.lock")

    return PackagePolicySummary(
        version=expected_version,
        package_count=len(member_names),
        workspace_dependency_count=len(crate_members),
    )


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate exact first-party candidate package policy."
    )
    parser.add_argument("--repository", required=True, type=Path)
    parser.add_argument("--expected-version", required=True)
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        summary = check_repository(arguments.repository, arguments.expected_version)
    except PackagePolicyError as error:
        print(f"package-policy: {error.code}: {error.path}", file=sys.stderr)
        return 1
    print(
        "package-policy: PASS: "
        f"{summary.package_count} packages at {summary.version}; "
        f"{summary.workspace_dependency_count} exact workspace dependencies"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
