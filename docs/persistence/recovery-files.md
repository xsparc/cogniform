# Immutable local recovery files

Status: implemented by CF018 for explicit caller-driven local persistence,
extended by CF032 with read-only offline inspection, and extended by CF033
with an opt-in versioned CLI JSON report.

`cogniform-storage` keeps filesystem authority outside `cogniform-engine`.
It persists the existing `EngineRecoveryPoint` envelope; it does not define a
second replay, snapshot, or restoration format.

## Create-new workflow

Construct `RecoveryFileStore` with the same reviewed `ReplayConfig` used by the
service. Capture an `EngineRecoveryPoint`, then pass a caller-authorized path
whose parent directory already exists:

```rust
let store = RecoveryFileStore::new(config.engine.replay)?;
let point = service.recovery_point()?;
let receipt = store.create_new(Path::new("state/checkpoint-0001.cnfr"), &point)?;
```

The adapter completes envelope encoding and configured-size validation before
opening the path. The final file is opened with create-new semantics, so an
existing file, directory, or final symlink is not overwritten. Every envelope
byte is written before `sync_all` is attempted. `RecoveryFileReceipt` reports
only bounded envelope/replay byte counts and the stored next frame identity; it
does not retain or reveal the path.

Unix creation requests owner read/write mode (`0600`) subject to the process
umask. Windows applies the parent directory's inherited ACL. Operators must
review parent permissions for the sensitivity of scene labels and replay data.

If write or sync fails, the file handle is closed and the exact path created by
the call is removed when possible. `PartialFileCleanup::Removed` confirms it is
absent; `Retained` reports only the cleanup error kind and means the caller must
inspect or remove that path before reuse. The adapter never retries, chooses a
replacement name, or silently accepts a partial file.

## Bounded load and restoration

`load` inspects the final path component and rejects symlinks and non-regular
files. It snapshots file metadata and rejects a length above the configured
envelope maximum or platform-addressable capacity before allocating. The read
accepts at most that snapshotted length through a fixed stack buffer. One
additional-byte probe rejects a file that grew after inspection.

The complete bytes then pass through `EngineRecoveryPoint` header, version,
bound, exact-length, frame, and SHA-256 digest validation. Truncated, extended,
or changed content returns an error and no partial recovery point. Passing the
loaded point to `LocalService::restore` still performs complete replay, world,
hash-chain, renderer-revision, and frame-continuity validation.

## Offline inspection

Inspect a caller-selected file under the declared default local profile without
creating a service or selecting a GPU adapter:

```text
cargo run -p cogniform-cli --locked --offline -- inspect-recovery state/checkpoint-0001.cnfr
```

The command accepts exactly one OS-native path. It loads through
`RecoveryFileStore`, then runs the same configuration, complete-stream,
frame-frontier, and authoritative-world replay preflight used before fresh-
service restoration. A successful report contains only:

- profile name (`default-local-64x64`);
- verified replay entry and encoded-byte counts;
- final scene revision and next frame identity; and
- final logical scene hash and replay-entry hash.

The default report above is human-readable and remains unchanged. Automation
can request the CLI-owned version-one JSON report:

```text
cargo run -p cogniform-cli --locked --offline -- inspect-recovery --json state/checkpoint-0001.cnfr
```

Version one is one compact JSON object followed by exactly one line-feed byte.
Field order, names, and JSON types are part of the tested CLI contract:

| Order | Field | JSON type | Meaning |
|---:|---|---|---|
| 1 | `schema_version` | number | Exact value `1` |
| 2 | `profile` | string | Exact value `default-local-64x64` |
| 3 | `replay_entries` | number | Verified replay entry count |
| 4 | `replay_bytes` | number | Encoded replay byte count |
| 5 | `scene_revision` | number | Final authoritative revision |
| 6 | `next_frame` | number | Next unreserved frame identity |
| 7 | `logical_hash` | string | Lowercase hexadecimal logical scene hash |
| 8 | `final_entry_hash` | string | Lowercase hexadecimal final replay-entry hash |

Use `inspect-recovery --json -- <path>` when the exact path begins with the
reserved filename `--json`. Scripts must select and validate
`schema_version`; do not parse the human output. Any incompatible report
change requires a new schema version.

The command performs no write, directory creation, adapter selection, GPU
initialization, service restoration, asset loading, or network work. Both
success and failure output omit the supplied path and replay payload. A passing
inspection proves only that CPU restoration preflight succeeds under this fixed
profile; GPU compatibility, asset residency, writer authenticity, freshness,
authorization, and a later service initialization remain unproven.

## Security and durability boundary

Filesystem paths are caller authority. Do not accept a path from an untrusted
remote request, and do not place a recovery file in a shared directory whose
writers are not trusted. The final-component symlink check is defense in depth;
parent-directory symlinks and path races are not sandboxed by this API.

Recovery files are plaintext and their digest is not a signature. Protect them
as scene data, authenticate the writer outside Cogniform, and choose freshness
outside Cogniform. File `sync_all` completion does not provide a portable
directory-entry or power-loss guarantee. A production latest-pointer protocol
needs a separately reviewed atomic replacement and directory-synchronization
design.

The adapter provides no directory creation, overwrite, rename, delete, file
discovery, snapshot catalog, retention/rotation, automatic checkpoint,
automatic startup/rollback, encryption, signing, key management, remote or
object storage, bundled asset/transient persistence, or background worker.
The inspection command adds no discovery, profile negotiation, JSON input,
general diagnostic schema, or automatic restore behavior. Aggregate hashes,
counts, revisions, and frame identities can still be sensitive correlation
data even though the report omits the path and payload.
Exact-hash asset sources may be persisted independently through
`AssetFileStore`; Cogniform does not associate those files with a recovery
point or load them automatically.

See [ADR 0018](../adr/0018-immutable-bounded-local-recovery-files.md),
[ADR 0032](../adr/0032-offline-recovery-file-inspection.md),
[ADR 0033](../adr/0033-versioned-recovery-inspection-json.md), the
[asset-file guide](asset-files.md), the
[replay guide](../architecture/determinism-and-replay.md), and the
[failure and recovery guide](../operations/failure-and-recovery.md).
