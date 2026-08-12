#!/usr/bin/env python3
"""Disposable-repository tests for bounded source-candidate preparation."""

from __future__ import annotations

import hashlib
import importlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from unittest import mock


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
SCRIPTS_ROOT = REPOSITORY_ROOT / "scripts"
SOURCE_TOOL = SCRIPTS_ROOT / "source_candidate.py"
sys.path.insert(0, str(SCRIPTS_ROOT))
source_candidate = importlib.import_module("source_candidate")


def run(
    *arguments: str,
    cwd: Path,
    check: bool = True,
    environment: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [*arguments],
        cwd=cwd,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=environment,
    )


def git(
    test_root: Path,
    *arguments: str,
    environment: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return run("git", *arguments, cwd=test_root, environment=environment)


def write(test_root: Path, relative_path: str, content: bytes | str) -> None:
    path = test_root / relative_path
    path.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(content, bytes):
        path.write_bytes(content)
    else:
        path.write_text(content, encoding="utf-8", newline="\n")


def commit(
    test_root: Path, message: str, *, executable_paths: tuple[str, ...] = ()
) -> None:
    for path in executable_paths:
        executable = test_root / path
        executable.chmod(executable.stat().st_mode | 0o111)
    git(test_root, "add", "--all")
    for path in executable_paths:
        git(test_root, "update-index", "--chmod=+x", path)
    environment = os.environ.copy()
    environment["GIT_AUTHOR_DATE"] = "2001-02-03T04:05:06Z"
    environment["GIT_COMMITTER_DATE"] = "2001-02-03T04:05:06Z"
    git(test_root, "commit", "--quiet", "-m", message, environment=environment)


def annotated_tag(test_root: Path, name: str = "v0.1.0-rc.1") -> str:
    environment = os.environ.copy()
    environment["GIT_COMMITTER_DATE"] = "2001-02-03T04:05:06Z"
    git(test_root, "tag", "-a", name, "-m", "candidate", environment=environment)
    return f"refs/tags/{name}"


def initialize_repository(
    parent: Path,
    name: str,
    *,
    mandatory: bool = True,
    object_format: str = "sha1",
    extra_files: dict[str, bytes | str] | None = None,
) -> tuple[Path, str]:
    test_root = parent / name
    test_root.mkdir()
    init_arguments = ["init", "--quiet", "--initial-branch=main"]
    if object_format != "sha1":
        init_arguments.append(f"--object-format={object_format}")
    git(test_root, *init_arguments)
    git(test_root, "config", "user.name", "Cogniform Test")
    git(test_root, "config", "user.email", "test@example.invalid")
    write(test_root, "src/main.rs", "fn main() {}\n")
    if mandatory:
        fixtures: dict[str, bytes | str] = {
            ".cargo/config.toml": "[net]\noffline = true\n",
            "Cargo.lock": "# fixture lock\n",
            "Cargo.toml": "[workspace]\nresolver = \"3\"\n",
            "LICENSE": "Apache License 2.0 fixture\n",
            "README.md": "# fixture\n",
            "rust-toolchain.toml": "[toolchain]\nchannel = \"stable\"\n",
            "docs/guide.md": "safe documentation\n",
            "tests/fixture.txt": "safe test\n",
            "vendor/package/source.rs": "pub fn fixture() {}\n",
        }
        for path, content in fixtures.items():
            write(test_root, path, content)
    if extra_files:
        for path, content in extra_files.items():
            write(test_root, path, content)
    commit(test_root, "fixture", executable_paths=("src/main.rs",))
    return test_root, annotated_tag(test_root)


def invoke(
    command: str,
    repository: Path,
    tag: str,
    archive: Path,
    checksum: Path,
    *,
    environment: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return run(
        sys.executable,
        str(SOURCE_TOOL),
        command,
        "--repository",
        str(repository),
        "--tag",
        tag,
        "--archive",
        str(archive),
        "--checksum",
        str(checksum),
        cwd=repository,
        check=False,
        environment=environment,
    )


def expect_failure(
    result: subprocess.CompletedProcess[str],
    code: str,
    *prohibited_values: str,
) -> None:
    output = result.stdout + result.stderr
    if result.returncode == 0:
        raise AssertionError(f"expected {code} failure")
    if f"source-candidate: {code}" not in output:
        raise AssertionError(f"expected {code}, received {output!r}")
    for value in prohibited_values:
        if value and value in output:
            raise AssertionError(f"failure {code} disclosed a prohibited value")


def rewrite_sidecar(archive: Path, checksum: Path) -> None:
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum.write_bytes(f"{digest}  {archive.name}\n".encode("utf-8"))


def tar_headers(data: bytes) -> list[tuple[int, int, bytes, bytes]]:
    headers: list[tuple[int, int, bytes, bytes]] = []
    offset = 0
    while offset + 512 <= len(data):
        block = data[offset : offset + 512]
        if block == bytes(512):
            break
        name = block[:100].split(b"\0", maxsplit=1)[0]
        size_raw = block[124:136].rstrip(b"\0 ")
        size = int(size_raw or b"0", 8)
        headers.append((offset, size, block[156:157], name))
        offset += 512 + (((size + 511) // 512) * 512)
    return headers


def update_header_checksum(data: bytearray, offset: int) -> None:
    data[offset + 148 : offset + 156] = b"        "
    checksum = sum(data[offset : offset + 512])
    data[offset + 148 : offset + 156] = f"{checksum:06o}\0 ".encode("ascii")


def append_duplicate_member(data: bytes) -> bytes:
    headers = tar_headers(data)
    regular = next(item for item in headers if item[2] == b"0")
    record_size = 512 + (((regular[1] + 511) // 512) * 512)
    record = data[regular[0] : regular[0] + record_size]
    last = headers[-1]
    insertion = last[0] + 512 + (((last[1] + 511) // 512) * 512)
    populated = data[:insertion] + record
    canonical_size = (
        (
            len(populated)
            + (2 * source_candidate.TAR_BLOCK_BYTES)
            + source_candidate.TAR_RECORD_BYTES
            - 1
        )
        // source_candidate.TAR_RECORD_BYTES
    ) * source_candidate.TAR_RECORD_BYTES
    return populated + bytes(canonical_size - len(populated))


def copy_candidate(
    source_archive: Path,
    source_checksum: Path,
    target_archive: Path,
    target_checksum: Path,
) -> None:
    shutil.copyfile(source_archive, target_archive)
    shutil.copyfile(source_checksum, target_checksum)
    rewrite_sidecar(target_archive, target_checksum)


def test_happy_path_and_limits(temporary: Path) -> tuple[Path, str, Path, Path, dict[str, object]]:
    repository, tag = initialize_repository(temporary, "happy")
    output = temporary / "output"
    output.mkdir()
    archive = output / "candidate.tar"
    checksum = output / "candidate.tar.sha256"
    prepared = invoke("prepare", repository, tag, archive, checksum)
    if prepared.returncode != 0:
        raise AssertionError(prepared.stdout + prepared.stderr)
    report = json.loads(prepared.stdout)
    if report["schema_version"] != 1 or report["tag_ref"] != tag:
        raise AssertionError("candidate report roles changed")
    if report["archive_bytes"] != archive.stat().st_size:
        raise AssertionError("archive size report mismatch")
    context = source_candidate.inspect_repository(repository, tag)
    executable = next(
        member
        for member in context.members
        if member.logical_path == b"src/main.rs"
    )
    if executable.mode != 0o755:
        raise AssertionError("executable Git mode was not mapped to tar 0755")
    expected_sidecar = (
        f"{report['sha256']}  {archive.name}\n".encode("utf-8")
    )
    if checksum.read_bytes() != expected_sidecar:
        raise AssertionError("checksum sidecar bytes changed")

    verified = invoke("verify", repository, tag, archive, checksum)
    if verified.returncode != 0 or json.loads(verified.stdout) != report:
        raise AssertionError("independent candidate verification changed")

    second_archive = output / "candidate-second.tar"
    second_checksum = output / "candidate-second.tar.sha256"
    repeated = invoke(
        "prepare", repository, tag, second_archive, second_checksum
    )
    if repeated.returncode != 0:
        raise AssertionError(repeated.stdout + repeated.stderr)
    if archive.read_bytes() != second_archive.read_bytes():
        raise AssertionError("repeated candidate bytes changed")
    if json.loads(repeated.stdout)["sha256"] != report["sha256"]:
        raise AssertionError("repeated candidate digest changed")

    exact_limits = source_candidate.CandidateLimits(
        max_archive_bytes=int(report["archive_bytes"]),
        max_members=int(report["members"]),
    )
    source_candidate.verify_candidate(
        repository, tag, str(archive), str(checksum), exact_limits
    )
    try:
        source_candidate.verify_candidate(
            repository,
            tag,
            str(archive),
            str(checksum),
            source_candidate.CandidateLimits(
                max_archive_bytes=int(report["archive_bytes"]) - 1,
                max_members=int(report["members"]),
            ),
        )
    except source_candidate.CandidateError as error:
        if error.code != "archive_limit_exceeded":
            raise
    else:
        raise AssertionError("archive limit minus one passed")
    try:
        source_candidate.verify_candidate(
            repository,
            tag,
            str(archive),
            str(checksum),
            source_candidate.CandidateLimits(
                max_archive_bytes=int(report["archive_bytes"]),
                max_members=int(report["members"]) - 1,
            ),
        )
    except source_candidate.CandidateError as error:
        if error.code != "member_limit_exceeded":
            raise
    else:
        raise AssertionError("member limit minus one passed")

    exact_archive = output / "exact-limit.tar"
    exact_checksum = output / "exact-limit.tar.sha256"
    source_candidate.prepare_candidate(
        repository,
        tag,
        str(exact_archive),
        str(exact_checksum),
        exact_limits,
    )
    short_archive = output / "short-limit.tar"
    short_checksum = output / "short-limit.tar.sha256"
    try:
        source_candidate.prepare_candidate(
            repository,
            tag,
            str(short_archive),
            str(short_checksum),
            source_candidate.CandidateLimits(
                max_archive_bytes=int(report["archive_bytes"]) - 1,
                max_members=int(report["members"]),
            ),
        )
    except source_candidate.CandidateError as error:
        if error.code != "archive_limit_exceeded":
            raise
    else:
        raise AssertionError("generation limit minus one passed")
    if short_archive.exists() or short_checksum.exists():
        raise AssertionError("over-limit generation left partial outputs")
    short_members_archive = output / "short-members.tar"
    short_members_checksum = output / "short-members.tar.sha256"
    try:
        source_candidate.prepare_candidate(
            repository,
            tag,
            str(short_members_archive),
            str(short_members_checksum),
            source_candidate.CandidateLimits(
                max_archive_bytes=int(report["archive_bytes"]),
                max_members=int(report["members"]) - 1,
            ),
        )
    except source_candidate.CandidateError as error:
        if error.code != "member_limit_exceeded":
            raise
    else:
        raise AssertionError("generation member limit minus one passed")
    if short_members_archive.exists() or short_members_checksum.exists():
        raise AssertionError("member-limit rejection left partial outputs")
    invalid_limits = (
        source_candidate.CandidateLimits(
            max_archive_bytes=source_candidate.MAX_ARCHIVE_BYTES + 1,
            max_members=source_candidate.MAX_FILESYSTEM_MEMBERS,
        ),
        source_candidate.CandidateLimits(
            max_archive_bytes=source_candidate.MAX_ARCHIVE_BYTES,
            max_members=source_candidate.MAX_FILESYSTEM_MEMBERS + 1,
        ),
    )
    for widened in invalid_limits:
        try:
            widened.validate()
        except source_candidate.CandidateError as error:
            if error.code != "invalid_limits":
                raise
        else:
            raise AssertionError("hard candidate limits were widened")

    sha256_repository, sha256_tag = initialize_repository(
        temporary, "sha256-source", object_format="sha256"
    )
    sha256_archive = output / "sha256.tar"
    sha256_checksum = output / "sha256.tar.sha256"
    sha256_result = invoke(
        "prepare",
        sha256_repository,
        sha256_tag,
        sha256_archive,
        sha256_checksum,
    )
    if sha256_result.returncode != 0:
        raise AssertionError(sha256_result.stdout + sha256_result.stderr)
    sha256_report = json.loads(sha256_result.stdout)
    if sha256_report["object_format"] != "sha256":
        raise AssertionError("SHA-256 repository identity was not retained")
    sha256_verified = invoke(
        "verify",
        sha256_repository,
        sha256_tag,
        sha256_archive,
        sha256_checksum,
    )
    if sha256_verified.returncode != 0:
        raise AssertionError(sha256_verified.stdout + sha256_verified.stderr)
    return repository, tag, archive, checksum, report


def test_admission_failures(temporary: Path) -> None:
    output = temporary / "admission-output"
    output.mkdir()

    lightweight, _ = initialize_repository(temporary, "lightweight-source")
    git(lightweight, "tag", "lightweight")
    result = invoke(
        "prepare",
        lightweight,
        "refs/tags/lightweight",
        output / "light.tar",
        output / "light.sha256",
    )
    expect_failure(result, "tag_not_annotated")

    nested, inner_ref = initialize_repository(temporary, "nested-source")
    inner_name = inner_ref.removeprefix("refs/tags/")
    git(nested, "tag", "-a", "outer", inner_name, "-m", "nested")
    result = invoke(
        "prepare",
        nested,
        "refs/tags/outer",
        output / "nested.tar",
        output / "nested.sha256",
    )
    expect_failure(result, "tag_not_direct_commit")

    branch, tag = initialize_repository(temporary, "branch-source")
    result = invoke(
        "prepare",
        branch,
        "refs/heads/main",
        output / "branch.tar",
        output / "branch.sha256",
    )
    expect_failure(result, "invalid_tag_ref")
    write(branch, "dirty.txt", "dirty\n")
    result = invoke(
        "prepare",
        branch,
        tag,
        output / "dirty.tar",
        output / "dirty.sha256",
    )
    expect_failure(result, "repository_dirty", str(branch))

    mismatch, tag = initialize_repository(temporary, "mismatch-source")
    write(mismatch, "after.txt", "new commit\n")
    commit(mismatch, "after tag")
    result = invoke(
        "prepare",
        mismatch,
        tag,
        output / "mismatch.tar",
        output / "mismatch.sha256",
    )
    expect_failure(result, "head_tag_mismatch")

    missing, tag = initialize_repository(
        temporary, "missing-source", mandatory=False
    )
    result = invoke(
        "prepare",
        missing,
        tag,
        output / "missing.tar",
        output / "missing.sha256",
    )
    expect_failure(result, "mandatory_content_missing")

    unsupported, old_tag = initialize_repository(temporary, "symlink-source")
    git(unsupported, "tag", "--delete", old_tag.removeprefix("refs/tags/"))
    git(unsupported, "config", "core.symlinks", "false")
    write(unsupported, "link-entry", b"target")
    blob = git(unsupported, "hash-object", "-w", "link-entry").stdout.strip()
    git(
        unsupported,
        "update-index",
        "--add",
        "--cacheinfo",
        f"120000,{blob},link-entry",
    )
    git(unsupported, "commit", "--quiet", "-m", "symlink entry")
    tag = annotated_tag(unsupported, "symlink")
    result = invoke(
        "prepare",
        unsupported,
        tag,
        output / "symlink.tar",
        output / "symlink.sha256",
    )
    expect_failure(result, "unsupported_git_entry")


def test_attribute_and_public_safeguards(temporary: Path) -> None:
    output = temporary / "safeguard-output"
    output.mkdir()
    with mock.patch.dict(
        os.environ,
        {
            "GIT_CONFIG_COUNT": "7",
            "GIT_NO_LAZY_FETCH": "0",
            "GIT_NO_REPLACE_OBJECTS": "0",
        },
    ):
        sanitized = source_candidate._sanitized_environment()
    if "GIT_CONFIG_COUNT" in sanitized:
        raise AssertionError("inherited Git configuration injection survived")
    if sanitized["GIT_NO_LAZY_FETCH"] != "1":
        raise AssertionError("lazy Git object fetching was not disabled")
    if sanitized["GIT_NO_REPLACE_OBJECTS"] != "1":
        raise AssertionError("Git replacement objects were not disabled")

    ambient, tag = initialize_repository(temporary, "ambient-source")
    attribute_file = temporary / "ambient-attributes"
    attribute_file.write_text("README.md export-ignore\n", encoding="utf-8")
    config_file = temporary / "ambient-gitconfig"
    config_file.write_text(
        f"[core]\n\tattributesFile = {attribute_file.as_posix()}\n",
        encoding="utf-8",
    )
    environment = os.environ.copy()
    environment["GIT_CONFIG_GLOBAL"] = str(config_file)
    result = invoke(
        "prepare",
        ambient,
        tag,
        output / "ambient.tar",
        output / "ambient.sha256",
        environment=environment,
    )
    if result.returncode != 0:
        raise AssertionError(result.stdout + result.stderr)

    local, tag = initialize_repository(temporary, "local-config-source")
    git(local, "config", "core.attributesFile", str(attribute_file))
    git(local, "config", "core.fsmonitor", "missing-fsmonitor-command")
    git(local, "config", "tar.tar.command", "false")
    result = invoke(
        "prepare",
        local,
        tag,
        output / "local-config.tar",
        output / "local-config.sha256",
    )
    if result.returncode != 0:
        raise AssertionError(result.stdout + result.stderr)

    replaced, tag = initialize_repository(temporary, "replacement-source")
    original_commit = git(replaced, "rev-parse", "HEAD").stdout.strip()
    write(replaced, "replacement-only.txt", "replacement payload\n")
    commit(replaced, "replacement commit")
    replacement_commit = git(replaced, "rev-parse", "HEAD").stdout.strip()
    git(replaced, "reset", "--hard", "--quiet", original_commit)
    git(replaced, "replace", original_commit, replacement_commit)
    replacement_archive = output / "replacement.tar"
    replacement_checksum = output / "replacement.sha256"
    result = invoke(
        "prepare",
        replaced,
        tag,
        replacement_archive,
        replacement_checksum,
    )
    if result.returncode != 0:
        raise AssertionError(result.stdout + result.stderr)
    if b"replacement payload" in replacement_archive.read_bytes():
        raise AssertionError("Git replacement object altered candidate content")

    override, tag = initialize_repository(temporary, "override-source")
    info_attributes = override / ".git" / "info" / "attributes"
    info_attributes.write_text("README.md export-ignore\n", encoding="utf-8")
    result = invoke(
        "prepare",
        override,
        tag,
        output / "override.tar",
        output / "override.sha256",
    )
    expect_failure(result, "attribute_override_present")

    ignored, _tag = initialize_repository(temporary, "ignored-source")
    write(ignored, ".gitattributes", "README.md export-ignore\n")
    commit(ignored, "archive attributes")
    tag = annotated_tag(ignored, "attributes")
    ignored_archive = output / "ignored.tar"
    ignored_checksum = output / "ignored.sha256"
    result = invoke("prepare", ignored, tag, ignored_archive, ignored_checksum)
    expect_failure(result, "archive_inventory_mismatch")
    if ignored_archive.exists() or ignored_checksum.exists():
        raise AssertionError("export-ignore failure left partial outputs")

    substituted, _tag = initialize_repository(temporary, "subst-source")
    write(substituted, "substituted.txt", "$Format:%H$\n")
    write(substituted, ".gitattributes", "substituted.txt export-subst\n")
    commit(substituted, "substitution attributes")
    tag = annotated_tag(substituted, "substitution")
    result = invoke(
        "prepare",
        substituted,
        tag,
        output / "subst.tar",
        output / "subst.sha256",
    )
    expect_failure(result, "archive_inventory_mismatch")

    private, tag = initialize_repository(
        temporary,
        "private-source",
        extra_files={".private/state.txt": "local only\n"},
    )
    result = invoke(
        "prepare",
        private,
        tag,
        output / "private.tar",
        output / "private.sha256",
    )
    expect_failure(result, "public_path_violation", str(private))

    token = ("gh" + "p_" + ("A" * 36)).encode("ascii")
    split_content = b"x" * (source_candidate.COPY_CHUNK_BYTES - 2) + token
    secret, tag = initialize_repository(
        temporary,
        "secret-source",
        extra_files={"tests/boundary.bin": split_content},
    )
    secret_archive = output / "secret.tar"
    secret_checksum = output / "secret.sha256"
    result = invoke(
        "prepare", secret, tag, secret_archive, secret_checksum
    )
    expect_failure(result, "public_content_violation", token.decode("ascii"))
    if secret_archive.exists() or secret_checksum.exists():
        raise AssertionError("content failure left partial outputs")


def test_output_and_movement_rules(temporary: Path) -> None:
    repository, tag = initialize_repository(temporary, "output-source")
    output = temporary / "output-rules"
    output.mkdir()
    archive = output / "existing.tar"
    checksum = output / "existing.sha256"
    archive.write_bytes(b"keep archive")
    checksum.write_bytes(b"keep checksum")
    result = invoke("prepare", repository, tag, archive, checksum)
    expect_failure(result, "output_exists")
    if archive.read_bytes() != b"keep archive" or checksum.read_bytes() != b"keep checksum":
        raise AssertionError("existing output was modified")

    result = invoke(
        "prepare",
        repository,
        tag,
        repository / "inside.tar",
        repository / "inside.sha256",
    )
    expect_failure(result, "output_inside_repository")

    context = source_candidate.inspect_repository(repository, tag)
    git(repository, "tag", "-f", "-a", tag.removeprefix("refs/tags/"), "-m", "moved")
    try:
        source_candidate._recheck_repository(context)
    except source_candidate.CandidateError as error:
        if error.code != "tag_moved":
            raise
    else:
        raise AssertionError("moved tag passed final recheck")

    head_repository, head_tag = initialize_repository(temporary, "head-move-source")
    head_context = source_candidate.inspect_repository(head_repository, head_tag)
    git(head_repository, "commit", "--quiet", "--allow-empty", "-m", "move head")
    try:
        source_candidate._recheck_repository(head_context)
    except source_candidate.CandidateError as error:
        if error.code != "head_moved":
            raise
    else:
        raise AssertionError("moved HEAD passed final recheck")

    cleanup_repository, cleanup_tag = initialize_repository(
        temporary, "final-recheck-cleanup-source"
    )
    cleanup_archive = output / "final-recheck.tar"
    cleanup_checksum = output / "final-recheck.sha256"
    with mock.patch.object(
        source_candidate,
        "_recheck_repository",
        side_effect=source_candidate.CandidateError("tag_moved"),
    ):
        try:
            source_candidate.prepare_candidate(
                cleanup_repository,
                cleanup_tag,
                str(cleanup_archive),
                str(cleanup_checksum),
            )
        except source_candidate.CandidateError as error:
            if error.code != "tag_moved":
                raise
        else:
            raise AssertionError("final recheck failure was reported as success")
    if cleanup_archive.exists() or cleanup_checksum.exists():
        raise AssertionError("final recheck failure left candidate outputs")


def test_corrupt_archive_matrix(
    repository: Path,
    tag: str,
    source_archive: Path,
    source_checksum: Path,
    temporary: Path,
) -> None:
    output = temporary / "corruption-output"
    output.mkdir()

    corrupt_archive = output / "corrupt.tar"
    corrupt_checksum = output / "corrupt.sha256"
    copy_candidate(source_archive, source_checksum, corrupt_archive, corrupt_checksum)
    data = bytearray(corrupt_archive.read_bytes())
    regular = next(
        item for item in tar_headers(data) if item[2] == b"0" and item[1] > 0
    )
    data[regular[0] + 512] ^= 1
    corrupt_archive.write_bytes(data)
    rewrite_sidecar(corrupt_archive, corrupt_checksum)
    result = invoke("verify", repository, tag, corrupt_archive, corrupt_checksum)
    expect_failure(result, "blob_identity_mismatch")

    type_cases = {
        "hard-link": (b"1", "unsupported_tar_member"),
        "symbolic-link": (b"2", "unsupported_tar_member"),
        "character-device": (b"3", "unsupported_tar_member"),
        "block-device": (b"4", "unsupported_tar_member"),
        "fifo": (b"6", "unsupported_tar_member"),
        "unknown": (b"7", "unsupported_tar_member"),
        "member-pax": (b"x", "unexpected_tar_extension"),
        "gnu-long-name": (b"L", "unexpected_tar_extension"),
        "gnu-long-link": (b"K", "unexpected_tar_extension"),
        "gnu-sparse": (b"S", "unexpected_tar_extension"),
    }
    for name, (type_flag, expected_code) in type_cases.items():
        type_archive = output / f"{name}.tar"
        type_checksum = output / f"{name}.sha256"
        copy_candidate(
            source_archive, source_checksum, type_archive, type_checksum
        )
        data = bytearray(type_archive.read_bytes())
        regular = next(item for item in tar_headers(data) if item[2] == b"0")
        data[regular[0] + 156] = type_flag[0]
        update_header_checksum(data, regular[0])
        type_archive.write_bytes(data)
        rewrite_sidecar(type_archive, type_checksum)
        result = invoke("verify", repository, tag, type_archive, type_checksum)
        expect_failure(result, expected_code)

    duplicate_archive = output / "duplicate.tar"
    duplicate_checksum = output / "duplicate.sha256"
    copy_candidate(
        source_archive, source_checksum, duplicate_archive, duplicate_checksum
    )
    duplicate_archive.write_bytes(append_duplicate_member(duplicate_archive.read_bytes()))
    rewrite_sidecar(duplicate_archive, duplicate_checksum)
    result = invoke(
        "verify", repository, tag, duplicate_archive, duplicate_checksum
    )
    expect_failure(result, "archive_inventory_mismatch")

    metadata_cases = (
        ("mode", 100, b"0000666\0", "tar_mode_mismatch"),
        ("owner", 108, b"0000001\0", "tar_owner_mismatch"),
        ("group", 116, b"0000001\0", "tar_owner_mismatch"),
        ("time", 136, b"00000000000\0", "tar_time_mismatch"),
        (
            "owner-name",
            265,
            b"other\0".ljust(32, b"\0"),
            "tar_owner_mismatch",
        ),
        (
            "link-role",
            157,
            b"target\0".ljust(100, b"\0"),
            "tar_role_mismatch",
        ),
        ("device-role", 329, b"0000001\0", "tar_role_mismatch"),
    )
    for name, field_offset, replacement, expected_code in metadata_cases:
        metadata_archive = output / f"{name}.tar"
        metadata_checksum = output / f"{name}.sha256"
        copy_candidate(
            source_archive,
            source_checksum,
            metadata_archive,
            metadata_checksum,
        )
        data = bytearray(metadata_archive.read_bytes())
        regular = next(item for item in tar_headers(data) if item[2] == b"0")
        start = regular[0] + field_offset
        data[start : start + len(replacement)] = replacement
        update_header_checksum(data, regular[0])
        metadata_archive.write_bytes(data)
        rewrite_sidecar(metadata_archive, metadata_checksum)
        result = invoke(
            "verify", repository, tag, metadata_archive, metadata_checksum
        )
        expect_failure(result, expected_code)

    header_archive = output / "header-checksum.tar"
    header_checksum = output / "header-checksum.sha256"
    copy_candidate(source_archive, source_checksum, header_archive, header_checksum)
    data = bytearray(header_archive.read_bytes())
    regular = next(item for item in tar_headers(data) if item[2] == b"0")
    data[regular[0] + 1] ^= 1
    header_archive.write_bytes(data)
    rewrite_sidecar(header_archive, header_checksum)
    result = invoke("verify", repository, tag, header_archive, header_checksum)
    expect_failure(result, "tar_checksum_mismatch")

    padding_archive = output / "padding.tar"
    padding_checksum = output / "padding.sha256"
    copy_candidate(source_archive, source_checksum, padding_archive, padding_checksum)
    data = bytearray(padding_archive.read_bytes())
    padded = next(
        item
        for item in tar_headers(data)
        if item[2] == b"0" and item[1] % 512 != 0
    )
    data[padded[0] + 512 + padded[1]] = 1
    padding_archive.write_bytes(data)
    rewrite_sidecar(padding_archive, padding_checksum)
    result = invoke("verify", repository, tag, padding_archive, padding_checksum)
    expect_failure(result, "nonzero_member_padding")

    path_archive = output / "path.tar"
    path_checksum = output / "path.sha256"
    copy_candidate(source_archive, source_checksum, path_archive, path_checksum)
    data = bytearray(path_archive.read_bytes())
    regular = next(item for item in tar_headers(data) if item[2] == b"0")
    unsafe_name = b"cogniform-source/../unsafe.txt"
    data[regular[0] : regular[0] + 100] = unsafe_name.ljust(100, b"\0")
    update_header_checksum(data, regular[0])
    path_archive.write_bytes(data)
    rewrite_sidecar(path_archive, path_checksum)
    result = invoke("verify", repository, tag, path_archive, path_checksum)
    expect_failure(result, "unsafe_path")

    pax_archive = output / "pax.tar"
    pax_checksum = output / "pax.sha256"
    copy_candidate(source_archive, source_checksum, pax_archive, pax_checksum)
    data = bytearray(pax_archive.read_bytes())
    pax = next(item for item in tar_headers(data) if item[2] == b"g")
    start = pax[0] + 512
    marker = data.find(b"comment=", start, start + pax[1])
    if marker == -1:
        raise AssertionError("expected git PAX comment")
    data[marker : marker + len(b"comment=")] = b"commenx="
    pax_archive.write_bytes(data)
    rewrite_sidecar(pax_archive, pax_checksum)
    result = invoke("verify", repository, tag, pax_archive, pax_checksum)
    expect_failure(result, "unexpected_pax_metadata")

    trailing_archive = output / "trailing.tar"
    trailing_checksum = output / "trailing.sha256"
    copy_candidate(source_archive, source_checksum, trailing_archive, trailing_checksum)
    data = bytearray(trailing_archive.read_bytes())
    data[-1] = 1
    trailing_archive.write_bytes(data)
    rewrite_sidecar(trailing_archive, trailing_checksum)
    result = invoke("verify", repository, tag, trailing_archive, trailing_checksum)
    expect_failure(result, "trailing_archive_data")

    zero_archive = output / "zero-tail.tar"
    zero_checksum = output / "zero-tail.sha256"
    copy_candidate(source_archive, source_checksum, zero_archive, zero_checksum)
    with zero_archive.open("ab") as destination:
        destination.write(bytes(source_candidate.TAR_RECORD_BYTES))
    rewrite_sidecar(zero_archive, zero_checksum)
    result = invoke("verify", repository, tag, zero_archive, zero_checksum)
    expect_failure(result, "noncanonical_tar_termination")

    mismatch_archive = output / "sidecar.tar"
    mismatch_checksum = output / "sidecar.sha256"
    copy_candidate(source_archive, source_checksum, mismatch_archive, mismatch_checksum)
    mismatch_checksum.write_bytes(b"0" * mismatch_checksum.stat().st_size)
    result = invoke(
        "verify", repository, tag, mismatch_archive, mismatch_checksum
    )
    expect_failure(result, "checksum_mismatch")


def test_cleanup_uncertainty(temporary: Path) -> None:
    repository, tag = initialize_repository(temporary, "cleanup-source")
    output = temporary / "cleanup-output"
    output.mkdir()
    archive = output / "cleanup.tar"
    checksum = output / "cleanup.sha256"
    with mock.patch.object(
        source_candidate,
        "inspect_archive",
        side_effect=source_candidate.CandidateError("forced_failure"),
    ), mock.patch.object(Path, "unlink", side_effect=OSError("fixture")):
        try:
            source_candidate.prepare_candidate(
                repository, tag, str(archive), str(checksum)
            )
        except source_candidate.CandidateError as error:
            if error.code != "cleanup_uncertain":
                raise
        else:
            raise AssertionError("cleanup uncertainty was reported as success")


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="cogniform-source-candidate-") as raw:
        temporary = Path(raw).resolve()
        repository, tag, archive, checksum, _report = test_happy_path_and_limits(
            temporary
        )
        test_admission_failures(temporary)
        test_attribute_and_public_safeguards(temporary)
        test_output_and_movement_rules(temporary)
        test_corrupt_archive_matrix(
            repository, tag, archive, checksum, temporary
        )
        test_cleanup_uncertainty(temporary)
    print("source candidate tests: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
