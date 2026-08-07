# Immutable exact-hash asset source files

Status: implemented by CF019 for explicit caller-driven local persistence.

`cogniform-storage::AssetFileStore` retains the exact source bytes for one
already known `cogniform-protocol::ContentHash`. It is a companion to, not part
of, `RecoveryFileStore`: recovery files contain replay and frame-continuity
state, while asset files contain only bulk source bytes.

## Create-new workflow

Construct the store with a reviewed non-zero per-file source limit. The
default matches `AssetLimits::default().max_source_bytes`. Supply a
caller-authorized path, the expected hash, and the exact source:

```rust
let store = AssetFileStore::default();
let receipt = store.create_new(
    Path::new("assets/triangle.glb"),
    expected_hash,
    &source_bytes,
)?;
```

The adapter rejects an oversized source or hash mismatch before touching the
filesystem. It opens the final path with create-new semantics, writes every
byte, and calls `sync_all`; an existing file, directory, or final symlink is
not overwritten. `AssetFileReceipt` contains only the verified content hash
and bounded source byte count.

Unix creation requests owner read/write mode (`0600`) subject to the process
umask. Windows applies the parent directory's inherited ACL. If write or sync
fails, `PartialFileCleanup::Removed` confirms the exact new path is absent;
`Retained` means the caller must inspect or remove it before reuse. Cogniform
does not choose another name or treat file existence as success.

## Bounded load and explicit rehydration

Load uses the expected logical identity rather than discovering content from
a path:

```rust
let source_bytes = store.load(Path::new("assets/triangle.glb"), expected_hash)?;
let key = AssetMeshKey {
    content_hash: expected_hash,
    mesh_index,
};
service.enqueue_asset_source(expected_hash, source_bytes)?;
service.process_next_asset_import()?;
service.enqueue_asset_upload(key)?;
service.process_next_asset_upload()?;
```

`load` rejects a final-component symlink or non-regular file. It bounds
metadata before allocation, reads no more than the snapshotted length through
a fixed buffer, probes for growth, and computes SHA-256 over the complete
bounded source. A mismatch returns no bytes. Truncation, extension, or any
other content change is therefore rejected against the expected hash.

Loading alone performs no GLB decoding, service mutation, network fetch, or
GPU upload. A caller restoring a service first loads the separate recovery
file, then loads each authorized source required by retained logical
references and drives ordinary import/upload. Successful rehydration changes
neither scene revision, logical hash, nor replay bytes.

`LocalService::evict_asset` affects only in-memory CPU and renderer state. It
never deletes or modifies this independently mapped source file. A caller may
later load the same approved file and explicitly rehydrate the exact hash.

## Security and durability boundary

Paths and hash-to-path associations are caller authority. Do not derive paths
from an untrusted remote request or scan an untrusted directory as a content
catalog. The final-component symlink check is defense in depth; parent
symlinks and path races are not sandboxed.

Asset files are plaintext. A successful expected-hash check proves byte
identity, not writer authenticity, freshness, confidentiality, or format
safety; the ordinary bounded asset importer still validates the source before
it can become decoded or GPU-resident state. File `sync_all` does not provide
a portable directory-entry or power-loss guarantee.

The adapter provides no directory creation, overwrite, rename, delete,
discovery, bundle/manifest, snapshot catalog, automatic retention or eviction,
automatic checkpoint/startup/rehydration, encryption, signing, key management,
remote/object storage, background worker, or wider asset-format support.

See [ADR 0019](../adr/0019-immutable-exact-hash-asset-source-files.md), the
[recovery-file guide](recovery-files.md), the
[GLB asset guide](../assets/glb-subset.md), and the
[failure and recovery guide](../operations/failure-and-recovery.md).
