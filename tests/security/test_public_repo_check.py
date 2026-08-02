#!/usr/bin/env python3
"""Deterministic behavior tests for the public-repository safeguard."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
CHECKER = REPOSITORY_ROOT / "scripts" / "check_public_repo.py"


def run(*arguments: str, cwd: Path, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [*arguments],
        cwd=cwd,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def git(test_root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return run("git", *arguments, cwd=test_root)


def scan(test_root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return run(sys.executable, "check_public_repo.py", *arguments, cwd=test_root, check=False)


def expect_failure(
    test_root: Path, expected_rule: str, prohibited_value: str = ""
) -> None:
    result = scan(test_root, "--staged")
    output = result.stdout + result.stderr
    if result.returncode == 0:
        raise AssertionError(f"expected rule {expected_rule} to fail")
    if expected_rule not in output:
        raise AssertionError(f"expected rule {expected_rule} was not reported")
    if prohibited_value and prohibited_value in output:
        raise AssertionError(f"matched content escaped redaction for {expected_rule}")


def reset_fixture(test_root: Path, relative_path: str) -> None:
    git(test_root, "restore", "--staged", "--", relative_path)
    path = test_root / relative_path
    if path.is_dir():
        shutil.rmtree(path)
    elif path.exists():
        path.unlink()


def stage_fixture(
    test_root: Path,
    relative_path: str,
    content: str,
    expected_rule: str,
    *,
    force: bool = False,
) -> None:
    path = test_root / relative_path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    add_arguments = ["add"]
    if force:
        add_arguments.append("--force")
    git(test_root, *add_arguments, "--", relative_path)
    expect_failure(test_root, expected_rule, content.strip())
    reset_fixture(test_root, relative_path.split("/", maxsplit=1)[0])


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="cogniform-public-check-") as temporary:
        test_root = Path(temporary).resolve()
        shutil.copy2(CHECKER, test_root / "check_public_repo.py")
        (test_root / "README.md").write_text("safe fixture\n", encoding="utf-8")

        git(test_root, "init", "--quiet")
        git(test_root, "config", "user.name", "Cogniform Test")
        git(test_root, "config", "user.email", "test@example.invalid")
        git(test_root, "add", "check_public_repo.py", "README.md")
        git(test_root, "commit", "--quiet", "-m", "test baseline")

        if scan(test_root, "--all").returncode != 0:
            raise AssertionError("safe baseline did not pass")

        stage_fixture(test_root, ".env", "placeholder only\n", "secret-file", force=True)
        stage_fixture(
            test_root,
            ".private/cache.txt",
            "local state\n",
            "unapproved-hidden-root",
            force=True,
        )
        stage_fixture(
            test_root,
            "AGENTS.md",
            "local operating notes\n",
            "private-orchestration",
            force=True,
        )
        stage_fixture(
            test_root,
            "scripts/agent_workflow.py",
            "# local workflow helper\n",
            "private-orchestration",
            force=True,
        )

        key_marker = "-----BEGIN RSA PRIVATE" + " KEY-----"
        stage_fixture(test_root, "key.txt", f"{key_marker}\n", "private-key")

        github_token = "ghp_" + ("A" * 36)
        stage_fixture(test_root, "token.txt", f"{github_token}\n", "github-token")

        aws_key = "AK" + "IA" + ("0" * 16)
        stage_fixture(test_root, "aws.txt", f"{aws_key}\n", "aws-access-key")

        slack_token = "xox" + "b-" + ("A" * 20)
        stage_fixture(test_root, "slack.txt", f"{slack_token}\n", "slack-token")

        stripe_key = "sk_" + "live_" + ("A" * 24)
        stage_fixture(test_root, "stripe.txt", f"{stripe_key}\n", "stripe-live-key")

        google_key = "AI" + "za" + ("A" * 35)
        stage_fixture(test_root, "google.txt", f"{google_key}\n", "google-api-key")

        generic_assignment = "password = \"" + ("S" * 24) + "\""
        stage_fixture(
            test_root,
            "settings.txt",
            f"{generic_assignment}\n",
            "generic-secret-assignment",
        )

        jwt_token = ".".join(("eyJ" + ("A" * 8), "eyJ" + ("B" * 8), "C" * 8))
        stage_fixture(test_root, "jwt.txt", f"{jwt_token}\n", "jwt-token")

        credential_url = "https://" + "user:password@example.invalid/path"
        stage_fixture(
            test_root,
            "url.txt",
            f"{credential_url}\n",
            "credential-url",
        )

        home_path = "C:\\" + "Users" + "\\alice\\private.txt"
        stage_fixture(test_root, "path.txt", f"{home_path}\n", "personal-home-path")

        private_endpoint = "https://service." + "internal/api"
        stage_fixture(
            test_root,
            "endpoint.txt",
            f"{private_endpoint}\n",
            "private-endpoint",
        )

        stage_fixture(
            test_root,
            "certificate.pem",
            "placeholder only\n",
            "private-key-file",
        )
        stage_fixture(
            test_root,
            "snapshot.sqlite",
            "placeholder only\n",
            "private-data-file",
        )

        (test_root / ".env.example").write_text("TOKEN=replace-me\n", encoding="utf-8")
        git(test_root, "add", ".env.example")
        if scan(test_root, "--staged").returncode != 0:
            raise AssertionError("documented environment template did not pass")
        reset_fixture(test_root, ".env.example")

    print("public repository safeguard tests: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
