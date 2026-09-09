# Documentation

This directory is the **single source of truth** for the architecture, algorithms, implementation, engineering rationale, and operational guidance of the `find` tool.

If you are new to the project, read these in order:

1. [Overview](overview.md) — project goals, scope, non-goals, supported platforms
2. [Getting Started](../README.md) — install and run a first search (in README)
3. [Architecture](architecture.md) — system design, module responsibilities, data flow
4. [Algorithms](algorithms.md) — mathematical foundation, complexity, pseudocode
5. [CLI](cli.md) — command-line flags and output format
6. [Configuration](configuration.md) — environment variables and runtime constants
7. [Operations](operations.md) — backup, restore, monitoring, scaling
8. [Troubleshooting](troubleshooting.md) — common issues and resolutions
9. [FAQ](faq.md) — conceptual questions

## Reference

| Document | Purpose |
|---|---|
| [Overview](overview.md) | Project goals, scope, non-goals, supported platforms, compatibility matrix |
| [Architecture](architecture.md) | System architecture, module responsibilities, data flow, concurrency model |
| [Algorithms](algorithms.md) | Mathematical derivation, complexity, pseudocode, edge cases |
| [Modules](modules.md) | Module-by-module reference for the `find` crate |
| [CLI](cli.md) | Command-line flags, arguments, output format, examples |
| [Configuration](configuration.md) | Environment variables, constants, run-time behavior |
| [Observability](observability.md) | Tracing, logging, audit boundaries, panic handling |
| [Performance](performance.md) | Performance characteristics and tuning guide |
| [Benchmarks](benchmarks.md) | Criterion suite, how to run, how to interpret results |
| [Testing](testing.md) | Testing strategy, test categories, verification methodology |
| [Deployment](deployment.md) | Docker, systemd, cross-compilation, hardening |
| [Operations](operations.md) | Backup, restore, monitoring, scaling, hot upgrades |
| [Troubleshooting](troubleshooting.md) | Common errors and resolutions |
| [Security](security.md) | Security model, threat model, hardening guidance |
| [FAQ](faq.md) | Frequently asked questions (conceptual) |
| [Roadmap](roadmap.md) | Future work, non-goals, supported versions |
| [Glossary](glossary.md) | Terms, abbreviations, definitions |
| [References](references.md) | External reading and standards |

## Maintenance

| Document | Purpose |
|---|---|
| [Release Process](maintenance/release.md) | Versioning, tagging, building, publishing releases |

## Architecture decision records

| Document | Decision |
|---|---|
| [ADR Index](adr/README.md) | Index of all ADRs |
| [ADR-0001](adr/0001-multi-variant-search.md) | Multi-variant range-splitting search |
| [ADR-0002](adr/0002-batch-normalization.md) | Montgomery simultaneous inversion |
| [ADR-0003](adr/0003-atomic-checkpointing.md) | Write-then-rename atomic checkpoints |
| [ADR-0004](adr/0004-error-hierarchy.md) | Single `FindError` enum |
| [ADR-0005](adr/0005-pure-search-module.md) | Pure `search` module with `CacheWriter` trait |
| [ADR-0006](adr/0006-binary-cache-format.md) | Raw 32-byte X-coordinate binary cache |

## Conventions

- All cross-doc links use **relative paths**.
- Code blocks include **language identifiers** (`rust`, `bash`, `toml`, `text`).
- Diagrams use **Mermaid** (rendered natively on GitHub).
- File names are lowercase with hyphens (e.g. `getting-started.md`).
- Top-level files (e.g. `README.md`, `CHANGELOG.md`, `LICENSE-MIT`) follow the GitHub community convention of being discoverable from the repository root.

## Contributing to documentation

See [CONTRIBUTING.md](../CONTRIBUTING.md) and the [release process](maintenance/release.md). All substantial changes should be reflected in an ADR — see [ADR-0001](adr/0001-multi-variant-search.md) for an example of the expected depth.
