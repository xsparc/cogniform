#!/usr/bin/env python3
"""Reject common secret and private-information patterns in Git objects."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from collections.abc import Iterable


APPROVED_HIDDEN_ROOTS = {".cargo", ".github"}
PRIVATE_ORCHESTRATION_PATHS = {"agents.md", "scripts/agent_workflow.py"}
ALLOWED_ENV_FILES = {".env.example", ".env.sample", ".env.template"}
SECRET_FILENAMES = {
    ".env",
    ".npmrc",
    ".pypirc",
    ".netrc",
    ".envrc",
    ".git-credentials",
    "auth.json",
    "credentials.json",
    "secrets.json",
    "id_rsa",
    "id_ed25519",
}
PRIVATE_KEY_SUFFIXES = (".pem", ".key", ".p12", ".pfx", ".jks", ".keystore", ".kdbx")
PRIVATE_DATA_SUFFIXES = (".sqlite", ".sqlite3", ".db", ".dump", ".dmp", ".bak", ".log")

PRIVATE_KEY_PATTERN = rb"-----BEGIN ([A-Z0-9][A-Z0-9 ]* )?PRIVATE" rb" KEY-----"
CONTENT_RULES = (
    ("private-key", PRIVATE_KEY_PATTERN),
    ("github-token", rb"gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}"),
    ("aws-access-key", rb"(AKIA|ASIA)[0-9A-Z]{16}"),
    ("slack-token", rb"xox[baprs]-[0-9A-Za-z-]{10,}"),
    ("stripe-live-key", rb"sk_live_[0-9A-Za-z]{16,}"),
    ("google-api-key", rb"AIza[0-9A-Za-z_-]{30,}"),
    (
        "generic-secret-assignment",
        rb"(?i)(api[_-]?key|client[_-]?secret|password|passwd|secret|access[_-]?token|auth[_-]?token)\s*[:=]\s*[\"']?[A-Za-z0-9_./+=-]{16,}",
    ),
    ("jwt-token", rb"eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}"),
    ("credential-url", rb"[A-Za-z][A-Za-z0-9+.-]*://[^/\s:]+:[^/@\s]+@"),
    ("personal-home-path", rb"[A-Za-z]:\\Users\\[^\\/:\s]+\\|/(Users|home)/[^/\s]+/"),
    ("private-endpoint", rb"https?://[A-Za-z0-9._-]+\.internal([/:]|$)"),
)
COMPILED_CONTENT_RULES = tuple(
    (rule_id, re.compile(pattern)) for rule_id, pattern in CONTENT_RULES
)


def run_git(*arguments: str) -> bytes:
    try:
        return subprocess.run(
            ["git", *arguments],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        raise RuntimeError("Git could not provide the requested repository objects") from error


def decode_paths(raw_paths: bytes) -> list[str]:
    return [
        item.decode("utf-8", errors="surrogateescape")
        for item in raw_paths.split(b"\0")
        if item
    ]


def safe_path(path: str) -> str:
    return path.encode("unicode_escape", errors="backslashreplace").decode("ascii")


def report(rule_id: str, path: str) -> None:
    print(f"public-repo-check: {rule_id}: {safe_path(path)}", file=sys.stderr)


def path_violations(path: str) -> Iterable[str]:
    lower_path = path.lower()
    segments = lower_path.split("/")
    first_segment = segments[0]
    basename = segments[-1]

    if lower_path in PRIVATE_ORCHESTRATION_PATHS:
        yield "private-orchestration"

    if len(segments) > 1 and first_segment.startswith("."):
        if first_segment not in APPROVED_HIDDEN_ROOTS:
            yield "unapproved-hidden-root"

    if basename not in ALLOWED_ENV_FILES:
        if (
            basename in SECRET_FILENAMES
            or basename.startswith(".env.")
            or basename.endswith(".env")
        ):
            yield "secret-file"

    if lower_path.endswith(PRIVATE_KEY_SUFFIXES):
        yield "private-key-file"
    if lower_path.endswith(PRIVATE_DATA_SUFFIXES):
        yield "private-data-file"


def read_blob(source: str, path: str) -> bytes:
    object_spec = f":{path}" if source == ":index:" else f"{source}:{path}"
    return run_git("show", object_spec)


def scan(source: str, paths: Iterable[str]) -> int:
    violations = 0
    for path in paths:
        for rule_id in path_violations(path):
            report(rule_id, path)
            violations += 1

        if path.lower().startswith("vendor/"):
            continue

        content = read_blob(source, path)
        for rule_id, pattern in COMPILED_CONTENT_RULES:
            if pattern.search(content):
                report(rule_id, path)
                violations += 1

    if violations:
        print(
            f"public-repo-check: failed with {violations} violation(s)",
            file=sys.stderr,
        )
        return 1

    print("public-repo-check: PASS")
    return 0


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Scan Git objects without printing matched content."
    )
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--all", action="store_true", help="scan every path at HEAD")
    modes.add_argument("--staged", action="store_true", help="scan staged changes")
    modes.add_argument(
        "--changed",
        nargs=2,
        metavar=("BASE", "HEAD"),
        help="scan paths changed between two revisions, reading content from HEAD",
    )
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        run_git("rev-parse", "--is-inside-work-tree")
        if arguments.all:
            source = "HEAD"
            paths = decode_paths(run_git("ls-tree", "-r", "--name-only", "-z", source))
        elif arguments.staged:
            source = ":index:"
            paths = decode_paths(
                run_git("diff", "--cached", "--name-only", "--diff-filter=ACMRTUXB", "-z")
            )
        else:
            base, source = arguments.changed
            run_git("rev-parse", "--verify", f"{base}^{{commit}}")
            run_git("rev-parse", "--verify", f"{source}^{{commit}}")
            paths = decode_paths(
                run_git("diff", "--name-only", "--diff-filter=ACMRTUXB", "-z", base, source)
            )
        return scan(source, paths)
    except RuntimeError as error:
        print(f"public-repo-check: error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
