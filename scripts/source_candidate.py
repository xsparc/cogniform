#!/usr/bin/env python3
"""Prepare and verify one deterministic, tag-bound source candidate."""

from __future__ import annotations

import argparse
import hashlib
import json
import mmap
import os
import stat
import subprocess
import sys
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO

from check_public_repo import content_violations, path_violations


SCHEMA_VERSION = 1
ARCHIVE_PREFIX = b"cogniform-source/"
MAX_ARCHIVE_BYTES = 268_435_456
MAX_FILESYSTEM_MEMBERS = 20_000
COPY_CHUNK_BYTES = 1024 * 1024
TAR_BLOCK_BYTES = 512
TAR_RECORD_BYTES = 10_240
MAX_PAX_BYTES = 4096
MAX_GIT_METADATA_BYTES = 1024 * 1024
MAX_INVENTORY_RECORD_BYTES = 512
MANDATORY_FILES = {
    ".cargo/config.toml",
    "Cargo.lock",
    "Cargo.toml",
    "LICENSE",
    "README.md",
    "rust-toolchain.toml",
}
MANDATORY_TREES = ("docs/", "tests/", "vendor/")
WINDOWS_RESERVED_NAMES = {
    "aux",
    "con",
    "nul",
    "prn",
    *(f"com{number}" for number in range(1, 10)),
    *(f"lpt{number}" for number in range(1, 10)),
}


class CandidateError(Exception):
    """Stable payload- and path-redacted candidate failure."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class CandidateLimits:
    max_archive_bytes: int = MAX_ARCHIVE_BYTES
    max_members: int = MAX_FILESYSTEM_MEMBERS

    def validate(self) -> None:
        if (
            self.max_archive_bytes <= 0
            or self.max_archive_bytes > MAX_ARCHIVE_BYTES
            or self.max_members <= 0
            or self.max_members > MAX_FILESYSTEM_MEMBERS
        ):
            raise CandidateError("invalid_limits")


@dataclass(frozen=True)
class OutputPaths:
    archive: Path
    checksum: Path


@dataclass(frozen=True)
class ExpectedMember:
    logical_path: bytes
    archive_path: bytes
    kind: str
    mode: int
    object_id: str | None
    size: int


@dataclass(frozen=True)
class RepositoryContext:
    root: Path
    git_dir: Path
    git_common_dir: Path
    tag_ref: str
    tag_object: str
    commit: str
    object_format: str
    commit_time: int
    git_version: str
    members: tuple[ExpectedMember, ...]


@dataclass(frozen=True)
class ArchiveInspection:
    digest: str
    size: int
    members: int


@dataclass(frozen=True)
class CandidateReport:
    schema_version: int
    tag_ref: str
    tag_object: str
    commit: str
    object_format: str
    git_version: str
    archive_bytes: int
    members: int
    sha256: str

    def canonical_json(self) -> str:
        return json.dumps(
            self.__dict__,
            ensure_ascii=True,
            separators=(",", ":"),
            sort_keys=True,
        )


def _sanitized_environment() -> dict[str, str]:
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.upper().startswith("GIT_")
    }
    environment.update(
        {
            "GIT_ATTR_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_SYSTEM": os.devnull,
            "GIT_NO_LAZY_FETCH": "1",
            "GIT_NO_REPLACE_OBJECTS": "1",
            "GIT_OPTIONAL_LOCKS": "0",
            "GIT_TERMINAL_PROMPT": "0",
        }
    )
    return environment


def _run_git(
    repository: Path,
    *arguments: str,
    max_output_bytes: int = MAX_GIT_METADATA_BYTES,
) -> bytes:
    if max_output_bytes <= 0:
        raise CandidateError("invalid_limits")
    try:
        process = subprocess.Popen(
            ["git", *arguments],
            cwd=repository,
            env=_sanitized_environment(),
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except FileNotFoundError as error:
        raise CandidateError("git_unavailable") from error
    except OSError as error:
        raise CandidateError("git_failed") from error

    assert process.stdout is not None
    try:
        output = process.stdout.read(max_output_bytes + 1)
        if len(output) > max_output_bytes:
            process.kill()
            process.wait()
            raise CandidateError("git_output_limit_exceeded")
        return_code = process.wait()
    except BaseException:
        if process.poll() is None:
            process.kill()
        process.wait()
        raise
    finally:
        process.stdout.close()
    if return_code != 0:
        raise CandidateError("git_failed")
    return output


def _git_has_output(repository: Path, *arguments: str) -> bool:
    try:
        process = subprocess.Popen(
            ["git", *arguments],
            cwd=repository,
            env=_sanitized_environment(),
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except FileNotFoundError as error:
        raise CandidateError("git_unavailable") from error
    except OSError as error:
        raise CandidateError("git_failed") from error

    assert process.stdout is not None
    try:
        first_byte = process.stdout.read(1)
        if first_byte:
            process.kill()
            process.wait()
            return True
        return_code = process.wait()
    except BaseException:
        if process.poll() is None:
            process.kill()
        process.wait()
        raise
    finally:
        process.stdout.close()
    if return_code != 0:
        raise CandidateError("git_failed")
    return False


def _ascii_line(value: bytes, code: str) -> str:
    try:
        decoded = value.decode("ascii").strip()
    except UnicodeDecodeError as error:
        raise CandidateError(code) from error
    if not decoded or "\n" in decoded or "\r" in decoded:
        raise CandidateError(code)
    return decoded


def _object_id(value: str, object_format: str) -> str:
    expected_length = 40 if object_format == "sha1" else 64
    if len(value) != expected_length or any(
        character not in "0123456789abcdef" for character in value
    ):
        raise CandidateError("invalid_object_id")
    return value


def _portable_path(path: bytes) -> str:
    try:
        decoded = path.decode("utf-8")
    except UnicodeDecodeError as error:
        raise CandidateError("unsafe_path") from error
    if unicodedata.normalize("NFC", decoded) != decoded:
        raise CandidateError("unsafe_path")
    if not decoded or decoded.startswith(("/", "\\")):
        raise CandidateError("unsafe_path")
    if "\\" in decoded or ":" in decoded:
        raise CandidateError("unsafe_path")
    components = decoded.split("/")
    for component in components:
        if component in {"", ".", ".."} or component.endswith((" ", ".")):
            raise CandidateError("unsafe_path")
        if any(ord(character) < 32 or ord(character) == 127 for character in component):
            raise CandidateError("unsafe_path")
        stem = component.split(".", maxsplit=1)[0].casefold()
        if stem in WINDOWS_RESERVED_NAMES:
            raise CandidateError("unsafe_path")
    return decoded


def _fits_ustar_path(path: bytes) -> bool:
    if len(path) <= 100:
        return True
    return any(
        separator <= 155 and 0 < len(path) - separator - 1 <= 100
        for separator, value in enumerate(path)
        if value == ord("/")
    )


def _portable_output_name(name: str) -> None:
    try:
        encoded = name.encode("utf-8")
    except UnicodeEncodeError as error:
        raise CandidateError("unsafe_output_name") from error
    try:
        _portable_path(encoded)
    except CandidateError as error:
        raise CandidateError("unsafe_output_name") from error
    if "/" in name:
        raise CandidateError("unsafe_output_name")


def _is_within(candidate: Path, parent: Path) -> bool:
    try:
        common = os.path.commonpath(
            (os.path.normcase(str(candidate)), os.path.normcase(str(parent)))
        )
    except ValueError:
        return False
    return common == os.path.normcase(str(parent))


def _resolve_outputs(
    archive_argument: str,
    checksum_argument: str,
    context: RepositoryContext,
    *,
    require_existing: bool,
) -> OutputPaths:
    archive_input = Path(archive_argument)
    checksum_input = Path(checksum_argument)
    _portable_output_name(archive_input.name)
    _portable_output_name(checksum_input.name)
    if archive_input.name == checksum_input.name:
        raise CandidateError("output_paths_conflict")

    try:
        archive_parent = archive_input.parent.resolve(strict=True)
        checksum_parent = checksum_input.parent.resolve(strict=True)
    except OSError as error:
        raise CandidateError("output_parent_invalid") from error
    if archive_parent != checksum_parent or not archive_parent.is_dir():
        raise CandidateError("output_parent_invalid")

    protected_roots = (context.root, context.git_dir, context.git_common_dir)
    if any(_is_within(archive_parent, root) for root in protected_roots):
        raise CandidateError("output_inside_repository")

    outputs = OutputPaths(
        archive=archive_parent / archive_input.name,
        checksum=archive_parent / checksum_input.name,
    )
    for path in (outputs.archive, outputs.checksum):
        try:
            metadata = path.lstat()
        except FileNotFoundError:
            if require_existing:
                raise CandidateError("output_missing")
            continue
        except OSError as error:
            raise CandidateError("output_unavailable") from error
        if not require_existing:
            raise CandidateError("output_exists")
        if path.is_symlink() or not stat.S_ISREG(metadata.st_mode):
            raise CandidateError("output_not_regular")
    return outputs


def _parse_tag_object(raw: bytes, tag_name: str, object_format: str) -> str:
    header, separator, _message = raw.partition(b"\n\n")
    if not separator:
        raise CandidateError("invalid_tag_object")
    fields: dict[bytes, bytes] = {}
    for line in header.splitlines():
        if line.startswith(b" "):
            continue
        key, separator, value = line.partition(b" ")
        if not separator or key in fields:
            raise CandidateError("invalid_tag_object")
        fields[key] = value
    if fields.get(b"type") != b"commit":
        raise CandidateError("tag_not_direct_commit")
    try:
        embedded_name = fields[b"tag"].decode("utf-8")
        target = fields[b"object"].decode("ascii")
    except (KeyError, UnicodeDecodeError) as error:
        raise CandidateError("invalid_tag_object") from error
    if embedded_name != tag_name:
        raise CandidateError("tag_identity_mismatch")
    return _object_id(target, object_format)


def _read_expected_members(
    root: Path,
    commit: str,
    object_format: str,
    limits: CandidateLimits,
) -> tuple[ExpectedMember, ...]:
    raw = _run_git(
        root,
        "ls-tree",
        "-r",
        "-t",
        "-z",
        "--full-tree",
        "--long",
        commit,
        max_output_bytes=limits.max_members * MAX_INVENTORY_RECORD_BYTES,
    )
    members = [
        ExpectedMember(b"", ARCHIVE_PREFIX, "directory", 0o755, None, 0)
    ]
    portable_names: set[str] = set()
    mandatory_paths: set[str] = set()

    for record in raw.split(b"\0"):
        if not record:
            continue
        metadata, separator, path = record.partition(b"\t")
        if not separator:
            raise CandidateError("invalid_git_inventory")
        fields = metadata.split()
        if len(fields) != 4:
            raise CandidateError("invalid_git_inventory")
        mode_raw, kind_raw, object_raw, size_raw = fields
        decoded_path = _portable_path(path)
        folded = decoded_path.casefold()
        if folded in portable_names:
            raise CandidateError("path_collision")
        portable_names.add(folded)

        try:
            mode = int(mode_raw, 8)
            object_id = _object_id(object_raw.decode("ascii"), object_format)
        except (UnicodeDecodeError, ValueError) as error:
            raise CandidateError("invalid_git_inventory") from error
        if kind_raw == b"tree" and mode == 0o40000:
            archive_path = ARCHIVE_PREFIX + path + b"/"
            if not _fits_ustar_path(archive_path):
                raise CandidateError("unsafe_path")
            members.append(
                ExpectedMember(
                    path,
                    archive_path,
                    "directory",
                    0o755,
                    None,
                    0,
                )
            )
        elif kind_raw == b"blob" and mode in {0o100644, 0o100755}:
            try:
                size = int(size_raw)
            except ValueError as error:
                raise CandidateError("invalid_git_inventory") from error
            if size < 0:
                raise CandidateError("invalid_git_inventory")
            archive_path = ARCHIVE_PREFIX + path
            if not _fits_ustar_path(archive_path):
                raise CandidateError("unsafe_path")
            members.append(
                ExpectedMember(
                    path,
                    archive_path,
                    "file",
                    0o644 if mode == 0o100644 else 0o755,
                    object_id,
                    size,
                )
            )
            mandatory_paths.add(decoded_path)
        else:
            raise CandidateError("unsupported_git_entry")

    if len(members) > limits.max_members:
        raise CandidateError("member_limit_exceeded")
    if not MANDATORY_FILES.issubset(mandatory_paths):
        raise CandidateError("mandatory_content_missing")
    if any(
        not any(path.startswith(prefix) for path in mandatory_paths)
        for prefix in MANDATORY_TREES
    ):
        raise CandidateError("mandatory_content_missing")
    return tuple(members)


def inspect_repository(
    repository: Path,
    tag_ref: str,
    limits: CandidateLimits = CandidateLimits(),
) -> RepositoryContext:
    limits.validate()
    repository = repository.resolve(strict=True)
    if _ascii_line(
        _run_git(repository, "rev-parse", "--is-inside-work-tree"),
        "repository_unavailable",
    ) != "true":
        raise CandidateError("repository_unavailable")
    root = Path(
        _ascii_line(
            _run_git(repository, "rev-parse", "--path-format=absolute", "--show-toplevel"),
            "repository_unavailable",
        )
    ).resolve(strict=True)
    git_dir = Path(
        _ascii_line(
            _run_git(root, "rev-parse", "--absolute-git-dir"),
            "repository_unavailable",
        )
    ).resolve(strict=True)
    common_raw = _ascii_line(
        _run_git(root, "rev-parse", "--path-format=absolute", "--git-common-dir"),
        "repository_unavailable",
    )
    git_common_dir = Path(common_raw)
    if not git_common_dir.is_absolute():
        git_common_dir = root / git_common_dir
    git_common_dir = git_common_dir.resolve(strict=True)

    if not tag_ref.startswith("refs/tags/") or tag_ref == "refs/tags/":
        raise CandidateError("invalid_tag_ref")
    tag_name = tag_ref.removeprefix("refs/tags/")
    try:
        _run_git(root, "check-ref-format", tag_ref)
    except CandidateError as error:
        raise CandidateError("invalid_tag_ref") from error

    object_format = _ascii_line(
        _run_git(root, "rev-parse", "--show-object-format"), "unsupported_object_format"
    )
    if object_format not in {"sha1", "sha256"}:
        raise CandidateError("unsupported_object_format")
    tag_object = _object_id(
        _ascii_line(
            _run_git(root, "show-ref", "--verify", "--hash", tag_ref),
            "tag_unavailable",
        ),
        object_format,
    )
    if _ascii_line(
        _run_git(root, "cat-file", "-t", tag_object), "tag_unavailable"
    ) != "tag":
        raise CandidateError("tag_not_annotated")
    commit = _parse_tag_object(
        _run_git(root, "cat-file", "tag", tag_object), tag_name, object_format
    )
    if _ascii_line(
        _run_git(root, "cat-file", "-t", commit), "tag_unavailable"
    ) != "commit":
        raise CandidateError("tag_not_direct_commit")
    head = _object_id(
        _ascii_line(_run_git(root, "rev-parse", "HEAD"), "head_unavailable"),
        object_format,
    )
    if head != commit:
        raise CandidateError("head_tag_mismatch")
    if _git_has_output(
        root,
        "-c",
        "core.fsmonitor=false",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--ignore-submodules=all",
    ):
        raise CandidateError("repository_dirty")

    info_attributes = Path(
        _ascii_line(
            _run_git(
                root,
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "info/attributes",
            ),
            "attribute_override_unavailable",
        )
    )
    if not info_attributes.is_absolute():
        info_attributes = root / info_attributes
    try:
        attributes_metadata = info_attributes.lstat()
    except FileNotFoundError:
        pass
    except OSError as error:
        raise CandidateError("attribute_override_unavailable") from error
    else:
        if info_attributes.is_symlink() or not stat.S_ISREG(attributes_metadata.st_mode):
            raise CandidateError("attribute_override_present")
        if attributes_metadata.st_size != 0:
            raise CandidateError("attribute_override_present")

    try:
        commit_time = int(
            _ascii_line(
                _run_git(root, "show", "-s", "--format=%ct", commit),
                "invalid_commit_time",
            )
        )
    except ValueError as error:
        raise CandidateError("invalid_commit_time") from error
    if commit_time < 0:
        raise CandidateError("invalid_commit_time")
    git_version = _ascii_line(_run_git(root, "--version"), "git_unavailable")
    members = _read_expected_members(root, commit, object_format, limits)
    return RepositoryContext(
        root=root,
        git_dir=git_dir,
        git_common_dir=git_common_dir,
        tag_ref=tag_ref,
        tag_object=tag_object,
        commit=commit,
        object_format=object_format,
        commit_time=commit_time,
        git_version=git_version,
        members=members,
    )


def _recheck_repository(context: RepositoryContext) -> None:
    tag_object = _ascii_line(
        _run_git(context.root, "show-ref", "--verify", "--hash", context.tag_ref),
        "tag_moved",
    )
    if tag_object != context.tag_object:
        raise CandidateError("tag_moved")
    head = _ascii_line(
        _run_git(context.root, "rev-parse", "HEAD"), "head_moved"
    )
    if head != context.commit:
        raise CandidateError("head_moved")
    if _git_has_output(
        context.root,
        "-c",
        "core.fsmonitor=false",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--ignore-submodules=all",
    ):
        raise CandidateError("repository_changed")
    if _ascii_line(_run_git(context.root, "--version"), "git_unavailable") != context.git_version:
        raise CandidateError("git_version_changed")


def _create_archive(
    context: RepositoryContext,
    archive: Path,
    limits: CandidateLimits,
) -> str:
    command = [
        "git",
        "-c",
        f"core.attributesFile={os.devnull}",
        "-c",
        "tar.umask=0022",
        "archive",
        "--format=tar",
        f"--prefix={ARCHIVE_PREFIX.decode('ascii')}",
        context.tag_object,
    ]
    created = False
    try:
        process = subprocess.Popen(
            command,
            cwd=context.root,
            env=_sanitized_environment(),
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except FileNotFoundError as error:
        raise CandidateError("git_unavailable") from error
    except OSError as error:
        raise CandidateError("archive_generation_failed") from error

    digest = hashlib.sha256()
    written = 0
    assert process.stdout is not None
    try:
        with archive.open("xb") as destination:
            created = True
            while True:
                chunk = process.stdout.read(COPY_CHUNK_BYTES)
                if not chunk:
                    break
                written += len(chunk)
                if written > limits.max_archive_bytes:
                    process.kill()
                    raise CandidateError("archive_limit_exceeded")
                destination.write(chunk)
                digest.update(chunk)
            destination.flush()
            os.fsync(destination.fileno())
    except FileExistsError as error:
        process.kill()
        process.wait()
        raise CandidateError("output_exists") from error
    except BaseException as error:
        process.kill()
        process.wait()
        if created:
            try:
                archive.unlink()
            except OSError as cleanup_error:
                raise CandidateError("cleanup_uncertain") from cleanup_error
        if isinstance(error, CandidateError):
            raise
        if isinstance(error, OSError):
            raise CandidateError("archive_write_failed") from error
        raise
    finally:
        process.stdout.close()
    return_code = process.wait()
    if return_code != 0 or written == 0:
        try:
            archive.unlink()
        except OSError as error:
            raise CandidateError("cleanup_uncertain") from error
        raise CandidateError("archive_generation_failed")
    return digest.hexdigest()


def _read_exact(stream: BinaryIO, size: int, digest: hashlib._Hash) -> bytes:
    value = stream.read(size)
    if len(value) != size:
        raise CandidateError("archive_truncated")
    digest.update(value)
    return value


def _tar_string(field: bytes) -> bytes:
    value, separator, padding = field.partition(b"\0")
    if separator and any(padding):
        raise CandidateError("noncanonical_tar_header")
    return value if separator else field


def _tar_octal(field: bytes, *, allow_empty: bool = False) -> int:
    stripped = field.rstrip(b"\0 ")
    if not stripped:
        if allow_empty:
            return 0
        raise CandidateError("noncanonical_tar_header")
    if any(character not in b"01234567" for character in stripped):
        raise CandidateError("noncanonical_tar_header")
    try:
        return int(stripped, 8)
    except ValueError as error:
        raise CandidateError("noncanonical_tar_header") from error


def _tar_header(block: bytes) -> dict[str, object]:
    if len(block) != TAR_BLOCK_BYTES:
        raise CandidateError("archive_truncated")
    expected_checksum = _tar_octal(block[148:156])
    actual_checksum = sum(block[:148]) + (8 * ord(" ")) + sum(block[156:])
    if expected_checksum != actual_checksum:
        raise CandidateError("tar_checksum_mismatch")
    if block[257:263] != b"ustar\0" or block[263:265] != b"00":
        raise CandidateError("unsupported_tar_format")
    name = _tar_string(block[0:100])
    prefix = _tar_string(block[345:500])
    if prefix:
        name = prefix + b"/" + name
    return {
        "name": name,
        "mode": _tar_octal(block[100:108]),
        "uid": _tar_octal(block[108:116]),
        "gid": _tar_octal(block[116:124]),
        "size": _tar_octal(block[124:136]),
        "mtime": _tar_octal(block[136:148]),
        "type": block[156:157],
        "linkname": _tar_string(block[157:257]),
        "uname": _tar_string(block[265:297]),
        "gname": _tar_string(block[297:329]),
        "devmajor": _tar_octal(block[329:337], allow_empty=True),
        "devminor": _tar_octal(block[337:345], allow_empty=True),
    }


def _validate_common_metadata(
    header: dict[str, object], expected_mode: int, commit_time: int
) -> None:
    if header["mode"] != expected_mode:
        raise CandidateError("tar_mode_mismatch")
    if header["uid"] != 0 or header["gid"] != 0:
        raise CandidateError("tar_owner_mismatch")
    if header["mtime"] != commit_time:
        raise CandidateError("tar_time_mismatch")
    if header["uname"] != b"root" or header["gname"] != b"root":
        raise CandidateError("tar_owner_mismatch")
    if (
        header["linkname"] != b""
        or header["devmajor"] != 0
        or header["devminor"] != 0
    ):
        raise CandidateError("tar_role_mismatch")


def _parse_pax(data: bytes, commit: str) -> None:
    offset = 0
    records: list[tuple[bytes, bytes]] = []
    while offset < len(data):
        space = data.find(b" ", offset)
        if space == -1:
            raise CandidateError("invalid_pax_header")
        length_raw = data[offset:space]
        if not length_raw.isdigit() or length_raw.startswith(b"0"):
            raise CandidateError("invalid_pax_header")
        length = int(length_raw)
        end = offset + length
        if end > len(data) or data[end - 1 : end] != b"\n":
            raise CandidateError("invalid_pax_header")
        record = data[space + 1 : end - 1]
        key, separator, value = record.partition(b"=")
        if not separator or not key:
            raise CandidateError("invalid_pax_header")
        records.append((key, value))
        offset = end
    if records != [(b"comment", commit.encode("ascii"))]:
        raise CandidateError("unexpected_pax_metadata")


def _read_member_data(
    stream: BinaryIO,
    size: int,
    archive_digest: hashlib._Hash,
    blob_digest: hashlib._Hash | None,
) -> None:
    remaining = size
    while remaining:
        chunk = _read_exact(stream, min(remaining, COPY_CHUNK_BYTES), archive_digest)
        if blob_digest is not None:
            blob_digest.update(chunk)
        remaining -= len(chunk)
    padding_size = (-size) % TAR_BLOCK_BYTES
    if padding_size:
        padding = _read_exact(stream, padding_size, archive_digest)
        if any(padding):
            raise CandidateError("nonzero_member_padding")


def inspect_archive(
    context: RepositoryContext,
    archive: Path,
    limits: CandidateLimits = CandidateLimits(),
) -> ArchiveInspection:
    limits.validate()
    try:
        path_metadata = archive.lstat()
    except OSError as error:
        raise CandidateError("archive_unavailable") from error
    if archive.is_symlink() or not stat.S_ISREG(path_metadata.st_mode):
        raise CandidateError("output_not_regular")
    if path_metadata.st_size <= 0 or path_metadata.st_size > limits.max_archive_bytes:
        raise CandidateError("archive_limit_exceeded")
    if path_metadata.st_size % TAR_RECORD_BYTES != 0:
        raise CandidateError("noncanonical_tar_termination")

    archive_digest = hashlib.sha256()
    content_ranges: list[tuple[str, int, int]] = []
    expected_index = 0
    global_headers = 0
    position = 0
    try:
        with archive.open("rb") as stream:
            opened_metadata = os.fstat(stream.fileno())
            if (
                opened_metadata.st_size != path_metadata.st_size
                or opened_metadata.st_mtime_ns != path_metadata.st_mtime_ns
            ):
                raise CandidateError("archive_changed")
            while position < path_metadata.st_size:
                block = _read_exact(stream, TAR_BLOCK_BYTES, archive_digest)
                position += TAR_BLOCK_BYTES
                if block == bytes(TAR_BLOCK_BYTES):
                    first_zero_offset = position - TAR_BLOCK_BYTES
                    expected_size = (
                        (
                            first_zero_offset
                            + (2 * TAR_BLOCK_BYTES)
                            + TAR_RECORD_BYTES
                            - 1
                        )
                        // TAR_RECORD_BYTES
                    ) * TAR_RECORD_BYTES
                    if path_metadata.st_size != expected_size:
                        raise CandidateError("noncanonical_tar_termination")
                    remaining = path_metadata.st_size - position
                    if remaining < TAR_BLOCK_BYTES:
                        raise CandidateError("noncanonical_tar_termination")
                    while remaining:
                        chunk = _read_exact(
                            stream, min(remaining, COPY_CHUNK_BYTES), archive_digest
                        )
                        if any(chunk):
                            raise CandidateError("trailing_archive_data")
                        position += len(chunk)
                        remaining -= len(chunk)
                    break

                header = _tar_header(block)
                size = int(header["size"])
                type_flag = header["type"]
                if type_flag == b"g":
                    if global_headers != 0 or expected_index != 0:
                        raise CandidateError("unexpected_pax_metadata")
                    global_headers += 1
                    if size > MAX_PAX_BYTES or header["name"] != b"pax_global_header":
                        raise CandidateError("invalid_pax_header")
                    # Git's synthetic global PAX record is not a filesystem
                    # member and retains its fixed 0666 mode under tar.umask.
                    _validate_common_metadata(header, 0o666, context.commit_time)
                    pax_data = _read_exact(stream, size, archive_digest)
                    position += size
                    _parse_pax(pax_data, context.commit)
                    padding_size = (-size) % TAR_BLOCK_BYTES
                    if padding_size:
                        padding = _read_exact(stream, padding_size, archive_digest)
                        position += padding_size
                        if any(padding):
                            raise CandidateError("nonzero_member_padding")
                    continue
                if type_flag in {b"x", b"L", b"K", b"S"}:
                    raise CandidateError("unexpected_tar_extension")
                if type_flag not in {b"0", b"5"}:
                    raise CandidateError("unsupported_tar_member")
                if expected_index >= len(context.members):
                    raise CandidateError("archive_inventory_mismatch")
                expected = context.members[expected_index]
                expected_index += 1
                if expected_index > limits.max_members:
                    raise CandidateError("member_limit_exceeded")
                actual_archive_path = header["name"]
                if not isinstance(actual_archive_path, bytes):
                    raise CandidateError("unsafe_path")
                if actual_archive_path == ARCHIVE_PREFIX:
                    actual_logical_path = b""
                elif actual_archive_path.startswith(ARCHIVE_PREFIX):
                    actual_logical_path = actual_archive_path[len(ARCHIVE_PREFIX) :]
                    if type_flag == b"5" and actual_logical_path.endswith(b"/"):
                        actual_logical_path = actual_logical_path[:-1]
                    _portable_path(actual_logical_path)
                else:
                    raise CandidateError("unsafe_path")
                if header["name"] != expected.archive_path:
                    raise CandidateError("archive_inventory_mismatch")
                decoded_path = (
                    "" if not expected.logical_path else _portable_path(expected.logical_path)
                )
                if decoded_path:
                    if any(path_violations(decoded_path)):
                        raise CandidateError("public_path_violation")
                expected_type = b"0" if expected.kind == "file" else b"5"
                if type_flag != expected_type or size != expected.size:
                    raise CandidateError("archive_inventory_mismatch")
                _validate_common_metadata(header, expected.mode, context.commit_time)

                data_offset = position
                blob_digest = None
                if expected.kind == "file":
                    blob_digest = hashlib.new(context.object_format)
                    blob_digest.update(f"blob {size}\0".encode("ascii"))
                    if not decoded_path.casefold().startswith("vendor/"):
                        content_ranges.append((decoded_path, data_offset, size))
                _read_member_data(stream, size, archive_digest, blob_digest)
                consumed = size + ((-size) % TAR_BLOCK_BYTES)
                position += consumed
                if blob_digest is not None and blob_digest.hexdigest() != expected.object_id:
                    raise CandidateError("blob_identity_mismatch")
            else:
                raise CandidateError("archive_truncated")

            if global_headers != 1:
                raise CandidateError("invalid_pax_header")
            if expected_index != len(context.members):
                raise CandidateError("archive_inventory_mismatch")
            final_metadata = os.fstat(stream.fileno())
            if (
                final_metadata.st_size != opened_metadata.st_size
                or final_metadata.st_mtime_ns != opened_metadata.st_mtime_ns
            ):
                raise CandidateError("archive_changed")
            with mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_READ) as mapped:
                for _path, start, size in content_ranges:
                    if any(content_violations(mapped, start, start + size)):
                        raise CandidateError("public_content_violation")
            mapped_metadata = os.fstat(stream.fileno())
            if (
                mapped_metadata.st_size != opened_metadata.st_size
                or mapped_metadata.st_mtime_ns != opened_metadata.st_mtime_ns
            ):
                raise CandidateError("archive_changed")
    except CandidateError:
        raise
    except (OSError, ValueError) as error:
        raise CandidateError("archive_read_failed") from error

    return ArchiveInspection(
        digest=archive_digest.hexdigest(),
        size=path_metadata.st_size,
        members=len(context.members),
    )


def _sidecar_bytes(digest: str, archive_name: str) -> bytes:
    return f"{digest}  {archive_name}\n".encode("utf-8")


def _write_checksum(checksum: Path, content: bytes) -> None:
    created = False
    try:
        with checksum.open("xb") as destination:
            created = True
            destination.write(content)
            destination.flush()
            os.fsync(destination.fileno())
    except FileExistsError as error:
        raise CandidateError("output_exists") from error
    except BaseException as error:
        if created:
            try:
                checksum.unlink()
            except OSError as cleanup_error:
                raise CandidateError("cleanup_uncertain") from cleanup_error
        if isinstance(error, OSError):
            raise CandidateError("checksum_write_failed") from error
        raise


def _verify_checksum(checksum: Path, expected: bytes) -> None:
    try:
        metadata = checksum.lstat()
        if checksum.is_symlink() or not stat.S_ISREG(metadata.st_mode):
            raise CandidateError("output_not_regular")
        if metadata.st_size != len(expected):
            raise CandidateError("checksum_mismatch")
        with checksum.open("rb") as source:
            actual = source.read(len(expected) + 1)
    except CandidateError:
        raise
    except OSError as error:
        raise CandidateError("checksum_unavailable") from error
    if actual != expected:
        raise CandidateError("checksum_mismatch")


def _report(context: RepositoryContext, inspection: ArchiveInspection) -> CandidateReport:
    return CandidateReport(
        schema_version=SCHEMA_VERSION,
        tag_ref=context.tag_ref,
        tag_object=context.tag_object,
        commit=context.commit,
        object_format=context.object_format,
        git_version=context.git_version,
        archive_bytes=inspection.size,
        members=inspection.members,
        sha256=inspection.digest,
    )


def verify_candidate(
    repository: Path,
    tag_ref: str,
    archive_argument: str,
    checksum_argument: str,
    limits: CandidateLimits = CandidateLimits(),
) -> CandidateReport:
    context = inspect_repository(repository, tag_ref, limits)
    outputs = _resolve_outputs(
        archive_argument,
        checksum_argument,
        context,
        require_existing=True,
    )
    inspection = inspect_archive(context, outputs.archive, limits)
    _verify_checksum(
        outputs.checksum,
        _sidecar_bytes(inspection.digest, outputs.archive.name),
    )
    _recheck_repository(context)
    return _report(context, inspection)


def _cleanup_created(paths: list[Path]) -> None:
    uncertain = False
    for path in reversed(paths):
        try:
            path.unlink(missing_ok=True)
        except OSError:
            uncertain = True
    if uncertain:
        raise CandidateError("cleanup_uncertain")


def prepare_candidate(
    repository: Path,
    tag_ref: str,
    archive_argument: str,
    checksum_argument: str,
    limits: CandidateLimits = CandidateLimits(),
) -> CandidateReport:
    context = inspect_repository(repository, tag_ref, limits)
    outputs = _resolve_outputs(
        archive_argument,
        checksum_argument,
        context,
        require_existing=False,
    )
    created: list[Path] = []
    try:
        generated_digest = _create_archive(context, outputs.archive, limits)
        created.append(outputs.archive)
        _write_checksum(
            outputs.checksum,
            _sidecar_bytes(generated_digest, outputs.archive.name),
        )
        created.append(outputs.checksum)
        inspection = inspect_archive(context, outputs.archive, limits)
        _verify_checksum(
            outputs.checksum,
            _sidecar_bytes(inspection.digest, outputs.archive.name),
        )
        if inspection.digest != generated_digest:
            raise CandidateError("archive_changed")
        _recheck_repository(context)
        return _report(context, inspection)
    except BaseException as error:
        try:
            _cleanup_created(created)
        except CandidateError as cleanup_error:
            raise cleanup_error from error
        raise


def _arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Prepare or verify one bounded annotated-tag source candidate."
    )
    subcommands = parser.add_subparsers(dest="command", required=True)
    for command in ("prepare", "verify"):
        subparser = subcommands.add_parser(command)
        subparser.add_argument("--repository", default=".")
        subparser.add_argument("--tag", required=True)
        subparser.add_argument("--archive", required=True)
        subparser.add_argument("--checksum", required=True)
    return parser.parse_args()


def main() -> int:
    arguments = _arguments()
    operation = prepare_candidate if arguments.command == "prepare" else verify_candidate
    try:
        report = operation(
            Path(arguments.repository),
            arguments.tag,
            arguments.archive,
            arguments.checksum,
        )
    except CandidateError as error:
        print(f"source-candidate: {error.code}", file=sys.stderr)
        return 1
    except (MemoryError, OverflowError):
        print("source-candidate: allocation_failed", file=sys.stderr)
        return 2
    except Exception:
        print("source-candidate: internal_error", file=sys.stderr)
        return 2
    print(report.canonical_json())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
