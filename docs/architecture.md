# Architecture

This document describes the system architecture of the `find` tool. It is the canonical reference for module responsibilities, data flow, persistence model, concurrency model, and extension points. For the mathematical foundations, see [algorithms.md](algorithms.md). For the engineering rationale behind each major decision, see the [ADRs](adr/README.md).

## Design philosophy

The system is built on three core pillars:

1. **Mathematical minimality.** Reducing cryptographic overhead by using projective coordinates, batch normalization, and pre-computed caches.
2. **Strict resilience.** Guaranteeing search state integrity through atomic I/O, integrity-anchored checkpoints, and non-blocking observability.
3. **High-throughput parallelism.** Leveraging work-stealing thread pools (`rayon`) to saturate all available CPU resources with early-exit on first match.

These pillars are operationalized through the layers described below. The decisions behind them are recorded as ADRs (see [ADR-0001](adr/0001-multi-variant-search.md), [ADR-0002](adr/0002-batch-normalization.md), [ADR-0003](adr/0003-atomic-checkpointing.md)).

## System overview

```mermaid
graph TD
    User([User]) -->|SEC1 hex| CLI[CLI Layer<br/>main.rs]
    CLI -->|Config| Orch[Orchestrator<br/>orchestrator.rs]
    Orch -->|target_p| Search[Search Layer<br/>search.rs]
    Orch -->|save / load| Persist[Persistence Layer<br/>persistence.rs]
    Search -->|primitives| ECC[ECC Layer<br/>ecc.rs]
    Search -.->|writes via trait| Persist
    Persist -->|scalar_mul_g / to_hex_x| ECC
    ECC -->|k256| K256[(k256 crate)]
    Persist -->|File I/O| Disk[(Disk)]
    CLI -->|tracing| Log[(Logs)]
    Error[(FindError<br/>error.rs)] -.->|Result&lt;T&gt;| CLI
    Error -.->|Result&lt;T&gt;| Orch
    Error -.->|Result&lt;T&gt;| Search
    Error -.->|Result&lt;T&gt;| Persist
    Error -.->|Result&lt;T&gt;| ECC
```

Notable properties:

- **`error` has no internal dependencies** — every layer returns `Result<T, FindError>`.
- **`search` is pure** — it depends on `ecc` and the `CacheWriter` trait, not on the file system directly. See [ADR-0005](adr/0005-pure-search-module.md).
- **`persistence` implements `search::CacheWriter`** — the only layer that performs I/O.
- **`orchestrator` wires the layers together** — the only place where the lifecycle is owned.
- **`main` is a thin CLI wrapper** — argument parsing, tracing setup, and result rendering only.

## Layer-by-layer reference

### 1. CLI layer (`main.rs`)

**Responsibility:** Parse command-line arguments, initialize the tracing subscriber, delegate to the orchestrator, and render the result.

**Key functions:**

| Function | Purpose |
|---|---|
| `main()` | Process entry point; parses args, sets up Rayon, initializes tracing, runs the orchestrator |
| `init_tracing(&str)` | Configures the `tracing-subscriber` registry with a daily-rolling file appender and a stderr layer |
| `render_success_report(SearchMatch, Duration)` | Formats a `SearchMatch` into the human-readable success block |

**Notable design choices:**

- The `Args` struct is defined with `clap`'s `derive` macros; defaults are `output_dir = "data"`, `log_dir = "logs"`, `cache_points = false`.
- A custom Rayon `panic_handler` is installed that logs the panic via `tracing::error!` and returns rather than aborting. See [ADR-0005](adr/0005-pure-search-module.md) for why this matters.
- The `tracing` subscriber is initialized with a `WorkerGuard` that is held for the lifetime of the process; dropping it at exit flushes buffered log events.

### 2. Orchestrator layer (`orchestrator.rs`)

**Responsibility:** Own the execution loop, manage the checkpoint lifecycle, and select the strategy (cached vs. compute-bound) per chunk.

**Key types and functions:**

| Item | Purpose |
|---|---|
| `Config` | Owned-string configuration: pubkey, output dir, cache flag |
| `Config::validate()` | Rejects empty pubkey |
| `run(&Config) -> Result<Option<SearchMatch>>` | Full session: parse → generate variants → load checkpoint → loop chunks → save checkpoint → return match |

**Internal constants:**

| Constant | Value | Purpose |
|---|---|---|
| `TRILLION` | `1_000_000_000_000` | Human-readable step size for audit boundary logging |
| `CACHE_CHUNK_SIZE` | `1_000_000_000` | Number of scalars per cache chunk (~32 GB cache per chunk) |
| `MAX_SEARCH` | `u64::MAX` | Theoretical upper bound of the search range |
| `MIN_J` | `1` | Minimum non-zero search scalar |

**Lifecycle of a `run` invocation:**

```mermaid
sequenceDiagram
    participant U as User
    participant O as Orchestrator
    participant S as Search
    participant P as Persistence
    participant FS as File System

    U->>O: run(&Config)
    O->>O: validate config
    O->>S: generate_variants(&target_p)
    O->>P: save_variants_to_json
    P->>FS: write points.json
    O->>P: Checkpoint::load
    alt checkpoint valid + pubkey match
        O->>P: cp.verify(pubkey)
        Note over O: current_j = cp.last_j
    else mismatch or missing
        Note over O: current_j = 0
    end
    loop per chunk
        O->>FS: check chunk_<start_j>.bin
        alt cache hit
            O->>P: perform_cached_sweep
        else cache miss + --cache-points
            O->>S: precompute_chunk
            S->>P: CacheWriter::write_block
            O->>P: perform_cached_sweep
        else cache miss + no --cache-points
            O->>S: perform_chunked_sweep
        end
        alt match found
            O-->>U: Ok(Some(m))
        else no match
            O->>P: Checkpoint::save_atomic
            P->>FS: write-then-rename checkpoint.json
        end
    end
    O-->>U: Ok(None)
```

**Key behaviors:**

- The orchestrator never directly computes ECC. All cryptographic work is delegated to `search::perform_chunked_sweep` or `precompute_chunk`.
- The checkpoint is written **only** when a chunk completes without a match. If a match is found, the checkpoint write is skipped.
- The chunk loop uses `saturating_add` for `current_j + CACHE_CHUNK_SIZE`; if saturation is detected, the loop exits with "Search space exhausted (overflow detected)".

### 3. Search layer (`search.rs`)

**Responsibility:** Implement the multi-variant range-splitting engine. Pure: no I/O, no global state, no platform-specific code. All side effects (cache writes, progress updates) are injected via explicit arguments.

**Public items:**

| Item | Purpose |
|---|---|
| `OffsetVariant` | A single shift variant: label, scalar offset, X-coordinate bytes |
| `VariantIndex` | Cache-optimized lookup (flat sorted array, O(log 512) binary search) |
| `SearchMatch` | The result of a successful match (label, offset, `j`, candidates) |
| `Progress` | Thread-safe `AtomicU64` counter for telemetry |
| `CacheWriter` | Object-safe trait for raw block writes |
| `generate_variants(&ProjectivePoint) -> Vec<OffsetVariant>` | Constructs the 512-variant set |
| `perform_chunked_sweep(&VariantIndex, start, end) -> Option<SearchMatch>` | CPU-bound parallel sweep with batch normalization |
| `precompute_chunk(start, end, &W, Option<&VariantIndex>, &Progress) -> Result<Option<SearchMatch>>` | Pre-computes a binary cache chunk while optionally searching for a match |

**Performance characteristics:**

- `BATCH_SIZE = 32` points per batch normalization. See [ADR-0002](adr/0002-batch-normalization.md).
- `MAX_BATCH = 32` is a private constant in the `search` module; the hot-path arrays are stack-allocated.
- Within a batch, the `+ G` increment chain is used (rather than `N` independent scalar multiplications). For batch start `chunk_start`, the first point is `chunk_start · G` and each subsequent point is `+ G`. This is `~N×` faster than per-point scalar multiplication.
- `rayon::find_map_any` provides early-exit on the first match.
- The `VariantIndex` reference is shared immutably across all workers; no locks are required because the index is read-only after construction.

**`precompute_chunk` cross-batch coordination:**

`precompute_chunk` uses a `Mutex<Option<SearchMatch>>` shared across batches. Each batch:

1. Locks the mutex and checks whether another batch has already found a match.
2. If yes, returns immediately (no work done).
3. If no, processes the batch and writes the X-coordinates via the `CacheWriter`.
4. If a match is discovered, locks the mutex and stores the match.
5. The orchestrator's outer loop then sees the match and terminates.

The mutex is unlocked after the early-exit check to minimize contention. Worker panics are tolerated: the final `into_inner()` call on a poisoned mutex logs a warning and returns whatever value was stored.

### 4. Persistence layer (`persistence.rs`)

**Responsibility:** All I/O side effects — atomic checkpoints, binary cache files, JSON variant exports.

**Public items:**

| Item | Purpose |
|---|---|
| `Checkpoint` | Durable state: `last_j`, `pubkey`, `last_x` (integrity anchor) |
| `Checkpoint::load(&Path) -> Result<Self>` | Read a checkpoint from a JSON file |
| `Checkpoint::verify(&str) -> Result<()>` | Verify the integrity anchor against the recalculated X-coordinate |
| `Checkpoint::save_atomic(&Path) -> Result<()>` | Write the checkpoint via write-then-rename with parent-dir `fsync` on Unix |
| `FileCacheWriter` | Cross-platform binary cache writer (implements `CacheWriter`) |
| `FileCacheWriter::create(&Path) -> Result<Self>` | Create a new cache file (and parent directories) |
| `FileCacheWriter::preallocate(u64) -> Result<()>` | Pre-allocate the file to the given length |
| `perform_cached_sweep(&VariantIndex, &Path, start_j) -> Result<Option<SearchMatch>>` | I/O-bound search against a pre-computed cache |
| `save_variants_to_json(&[OffsetVariant], &str) -> Result<String>` | Export variant metadata to `points.json` |

**Atomic persistence pattern (`save_atomic`):**

```mermaid
graph TD
    A[Start] --> B[Write JSON to .tmp file]
    B --> C[sync_all to flush data]
    C --> D[rename .tmp to checkpoint.json]
    D --> E{Unix?}
    E -->|Yes| F[fsync parent directory]
    E -->|No| G[Skip]
    F --> H[Done]
    G --> H[Done]
```

The rename is atomic on POSIX-compliant file systems (ext4, XFS, APFS, NTFS). The parent-directory `fsync` on Unix closes a subtle durability gap: most file systems require the directory entry to be flushed for the rename to survive a crash.

**Cross-platform `FileCacheWriter::write_block`:**

| Platform | Mechanism | Notes |
|---|---|---|
| Unix | `pwrite_at(data, offset)` via `std::os::unix::fs::FileExt` | Atomic at any offset; no locking required |
| Other | `seek(SeekFrom::Start(offset))` + `write_all(data)` under `Mutex<File>` | Lock contention is negligible (one ~1 KB write per batch) |

**Cache file format** (see [ADR-0006](adr/0006-binary-cache-format.md) for full details):

```
+----------------+----------------+----------------+-----+----------------+
| x(start·G)[..]  | x((start+1)·G) | x((start+2)·G) | ... | x(end·G)[..]  |
+----------------+----------------+----------------+-----+----------------+
       32 bytes      32 bytes         32 bytes             32 bytes
```

- Total size = `(end - start + 1) × 32` bytes.
- No header or footer; metadata is in the filename (`chunk_<start_j>.bin`).
- Big-endian X-coordinate encoding matches SEC1.
- File size must be a multiple of 32 bytes; otherwise `CacheCorrupted` is raised.

### 5. ECC layer (`ecc.rs`)

**Responsibility:** Thin wrapper over `k256` that exposes a search-oriented API. All operations enforce SEC1 compliance and strict scalar range validation.

**Public items:**

| Item | Purpose |
|---|---|
| `parse_pubkey(&str) -> Result<ProjectivePoint>` | Parse a hex-encoded SEC1 public key |
| `generator() -> ProjectivePoint` | Return the secp256k1 generator point `G` |
| `hex_to_scalar(&str) -> Result<Scalar>` | Convert hex to `Scalar`, reducing modulo `n` |
| `scalar_mul_g(&Scalar) -> ProjectivePoint` | Compute `d·G` via fixed-base scalar multiplication |
| `subtract(&ProjectivePoint, &ProjectivePoint) -> ProjectivePoint` | Compute `P - Q` in projective coordinates |
| `to_hex_x(&ProjectivePoint) -> String` | Extract the 32-byte hex X-coordinate (identity-safe) |

**Coordinate system:** points are stored in **projective coordinates** during arithmetic and only converted to affine form during batch normalization. The `to_hex_x` function performs the per-point conversion (used by the orchestrator for the checkpoint integrity anchor).

### 6. Error model (`error.rs`)

**Responsibility:** Single error type returned by every fallible function. See [ADR-0004](adr/0004-error-hierarchy.md).

| Variant | Subsystem |
|---|---|
| `EccError(String)` | ECC arithmetic failures |
| `ResearchIntegrityError(String)` | Checkpoint anchor mismatch |
| `InvalidPublicKey(String)` | SEC1 parsing failures |
| `Io(std::io::Error)` | File-system operations |
| `HexError(hex::FromHexError)` | Hex decoding |
| `SerializationError(serde_json::Error)` | JSON serialization |
| `CacheCorrupted(String)` | Binary cache validation |

The `Result<T>` type alias is used throughout the crate: `pub type Result<T> = std::result::Result<T, FindError>;`.

## Data flow

### Search pipeline (CPU-bound path)

```mermaid
graph LR
    A[User: --pubkey HEX] --> B[ecc::parse_pubkey]
    B --> C[search::generate_variants<br/>512 scalars + normalizations]
    C --> D[VariantIndex::new<br/>sort by X]
    D --> E[Search loop per chunk]
    E --> F[perform_chunked_sweep]
    F --> G[Batch: 32x scalar mul G]
    G --> H[batch_normalize<br/>1 inversion + 31 muls]
    H --> I[VariantIndex::match_x<br/>O log 512 binary search]
    I --> J{Match?}
    J -->|Yes| K[Compute V + j, V - j mod n]
    K --> L[Return SearchMatch]
    J -->|No| M[Save checkpoint]
    M --> E
```

### Search pipeline (cached path)

```mermaid
graph LR
    A[Chunk start: current_j] --> B{Cache file exists?}
    B -->|Yes| C[perform_cached_sweep]
    C --> D[read 32 bytes]
    D --> E[VariantIndex::match_x]
    E --> F{Match?}
    F -->|Yes| G[Return SearchMatch]
    F -->|No| H{More data?}
    H -->|Yes| D
    H -->|No| I[Return None]
    B -->|No + --cache-points| J[precompute_chunk]
    J --> K[Batch: scalar mul G + normalize]
    K --> L[Match? + Write to cache]
    L --> M[perform_cached_sweep on new file]
    B -->|No + no cache| N[perform_chunked_sweep]
```

### Checkpoint flow

```mermaid
sequenceDiagram
    participant O as Orchestrator
    participant P as Persistence
    participant FS as File System

    Note over O: Chunk complete, no match
    O->>O: current_j = chunk_end
    O->>O: boundary_p = scalar_mul_g(current_j)
    O->>O: boundary_x = to_hex_x(boundary_p)
    O->>P: Checkpoint { last_j, pubkey, last_x }
    P->>P: Write JSON to checkpoint.json.tmp
    P->>FS: sync_all on .tmp
    P->>FS: rename .tmp to checkpoint.json
    alt Unix
        P->>FS: fsync parent dir
    end
```

On resume:

```mermaid
sequenceDiagram
    participant O as Orchestrator
    participant P as Persistence
    participant FS as File System

    O->>P: Checkpoint::load
    P->>FS: read checkpoint.json
    P-->>O: Checkpoint { last_j, pubkey, last_x }
    alt pubkey matches
        O->>P: cp.verify(current_pubkey)
        P->>O: recompute last_j · G X-coordinate
        P->>P: compare to last_x
        alt match
            Note over O: Resume from last_j + 1
        else mismatch
            O-->>O: ResearchIntegrityError → refuse to proceed
        end
    else pubkey differs
        Note over O: Different search; start fresh
    end
```

## Concurrency model

### Thread topology

```mermaid
graph TD
    Main[Main thread<br/>main.rs] -->|orchestrator::run| Orch
    Orch[Orchestrator thread] -->|into_par_iter| Rayon[Rayon thread pool<br/>N physical cores]
    Rayon -->|batch| W0[Worker 0]
    Rayon -->|batch| W1[Worker 1]
    Rayon -->|batch| Wn[Worker N-1]
    Main -->|non_blocking| LogWriter[Log writer thread]
    LogWriter -->|daily rotation| Disk[(Log files)]
```

- The main thread is the orchestrator; it is blocked on `orchestrator::run` for the duration of the search.
- The Rayon thread pool is sized to the number of physical cores by default. Each worker processes one batch at a time.
- The `tracing` non-blocking writer runs on a dedicated OS thread owned by `tracing_appender`. It is decoupled from the orchestrator and worker threads.

### Synchronization primitives

| Primitive | Location | Purpose |
|---|---|---|
| `AtomicU64` (Relaxed) | `search::Progress` | Telemetry counter; relaxed ordering is sufficient because the value is informational |
| `Mutex<Option<SearchMatch>>` | `search::precompute_chunk` | Cross-batch early-exit coordination |
| `Mutex<File>` | `persistence::FileCacheWriter` (non-Unix only) | Serializes `seek + write_all` on platforms without `pwrite_at` |
| `tracing` non-blocking writer | `main::init_tracing` | Decouples log I/O from the CPU path |

### Early-exit semantics

`rayon::find_map_any` provides a clean early-exit: when any worker returns `Some(_)`, the remaining workers' batches are abandoned and the result is returned. The behavior is documented in the [`rayon` documentation](https://docs.rs/rayon).

The custom panic handler in `main.rs` allows a worker panic to be recovered: the panic is logged, and the search continues with the remaining workers. The `Mutex::into_inner()` pattern in `precompute_chunk` extracts the match from a potentially poisoned lock.

## Persistence architecture

### Filesystem layout

```
data/
├── checkpoint.json              # Single durable checkpoint
├── points.json                  # Variant metadata (X → offset)
└── checkpoints/
    ├── chunk_1.bin              # Cache for j ∈ [1, 1_000_000_000]
    ├── chunk_1000000001.bin     # Cache for j ∈ [1_000_000_001, 2_000_000_000]
    └── ...

logs/
├── find.log.2026-04-12          # Daily-rolling log file
├── find.log.2026-04-13
└── ...
```

### Write durability

| File | Durability mechanism | Crash behavior |
|---|---|---|
| `checkpoint.json` | Write-then-rename with parent-dir `fsync` (Unix) | A crash before the rename leaves the previous valid checkpoint intact |
| `chunk_*.bin` | `pwrite_at` (Unix) or seek+write (other) per batch | A crash mid-chunk leaves a partial file; the size check rejects it on next read |
| `points.json` | `fs::write` (not atomic) | A crash mid-write may leave a partial JSON; the tool re-generates on each run |
| `find.log.*` | `tracing_appender::non_blocking` buffered writer | A crash may lose up to one buffer's worth of events |

### Recovery procedure

On startup, the orchestrator:

1. Reads `checkpoint.json` if it exists.
2. If the pubkey matches the current run, verifies the integrity anchor.
3. Resumes from `last_j + 1` if valid; starts from `1` if the pubkey differs or the file is missing.

If verification fails, the process exits with `ResearchIntegrityError`. The user must delete `checkpoint.json` and restart.

## Security architecture

See [security.md](security.md) for the full security model. The architecture-level points:

- **One reviewed `unsafe` call** in application code (`libc::fsync` in `persistence.rs`); no other application-code `unsafe`.
- **Atomic state persistence** via write-then-rename and parent-directory `fsync`.
- **Input validation** at the boundary (`parse_pubkey`).
- **No network I/O** — the tool does not require or use the network.
- **Dependency auditing** via `cargo audit` and `cargo deny` in CI.

## Extension points

| Extension | Mechanism |
|---|---|
| Custom cache storage | Implement `search::CacheWriter` and pass it to `precompute_chunk` |
| Custom progress reporting | Pass a custom `search::Progress` (or any type with the same `add`/`get` shape) |
| Custom variant generation | Construct `search::OffsetVariant` instances directly and build a `VariantIndex` |
| Custom session control | Call `search::perform_chunked_sweep` directly; bypass the orchestrator entirely |
| Custom error handling | Match on `FindError` variants in the `Result` return type |

The `CacheWriter` trait is **object-safe** (`Send + Sync` supertraits, no generics), enabling dynamic dispatch and runtime writer selection.

## Performance characteristics

| Operation | Complexity | Notes |
|---|---|---|
| Variant generation | O(512) | One-time per target pubkey |
| Index lookup | O(log 512) = O(1) | Flat array, L1-resident |
| Sweep (CPU) | O(R) | Linear over range `R` |
| Sweep (cached) | O(R) | Disk-bandwidth bound |
| Batch normalization | 1 inversion + 31 multiplications per 32 points | See [ADR-0002](adr/0002-batch-normalization.md) |

See [performance.md](performance.md) for the full performance guide and [benchmarks.md](benchmarks.md) for benchmark instructions.

## See also

- [algorithms.md](algorithms.md) — Mathematical foundation
- [modules.md](modules.md) — Module-by-module reference
- [cli.md](cli.md) — Command-line interface
- [observability.md](observability.md) — Tracing and logging
- [security.md](security.md) — Security model
- [ADRs](adr/README.md) — Architectural decision records
