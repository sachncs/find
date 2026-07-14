# Contributing to the Find Tool

Thank you for your interest in contributing to the `find` tool! This project is dedicated to high-performance cryptographic research and educational exploration of secp256k1 mathematics.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [How to Contribute](#how-to-contribute)
- [Development Setup](#development-setup)
- [Branch Naming](#branch-naming)
- [Commit Conventions](#commit-conventions)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Testing Requirements](#testing-requirements)
- [Documentation](#documentation)

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold its terms.

## Getting Started

1. **Fork** the repository on GitHub
2. **Clone** your fork locally:
   ```bash
   git clone https://github.com/your-username/find.git
   cd find
   ```
3. **Add upstream remote**:
   ```bash
   git remote add upstream https://github.com/sachncs/find.git
   ```
4. **Setup git hooks** (recommended):
   ```bash
   ./scripts/setup-hooks.sh
   ```
5. **Install dependencies**:
   ```bash
   make build
   make test
   ```

## How to Contribute

### Reporting Bugs

- Check [existing issues](https://github.com/sachncs/find/issues) first
- Use the **Bug Report** template
- Include reproduction steps, expected vs actual behavior
- Provide environment details (OS, Rust version, project version)

### Suggesting Features

- Use the **Feature Request** template
- Explain the research or educational value
- Consider alignment with project mission (see below)

### Research Alignment

We accept contributions that:

- Advance cryptographic education
- Improve performance for research workloads
- Enhance documentation or tooling
- Fix bugs or improve reliability

We do **not** accept:

- Features designed for non-educational or non-research use cases
- Changes that compromise mathematical correctness
- Optimizations that sacrifice code clarity without clear benefit

## Development Setup

### Prerequisites

- Rust 1.70 or later (install via [rustup](https://rustup.rs/))
- `cargo-tarpaulin` for coverage (optional)
- `cargo-audit` for security checks (optional)

### Building

```bash
# Development build
cargo build

# Release build with optimizations
cargo build --release

# Or use the Makefile
make build
```

### Running Tests

```bash
# Full test suite
make test

# Run specific test
cargo test test_name

# Run with increased property-test cases
PROPTEST_CASES=1000 cargo test --release
```

### Linting and Formatting

```bash
# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Or use the Makefile
make lint
```

### Benchmarks

```bash
# Run microbenchmarks
make bench

# Run specific benchmark
cargo bench --bench bench -- benchmark_name
```

## Branch Naming

Use descriptive branch names with prefixes:

| Prefix | Use Case |
|--------|----------|
| `feat/` | New features |
| `fix/` | Bug fixes |
| `docs/` | Documentation changes |
| `refactor/` | Code refactoring |
| `test/` | Adding or updating tests |
| `chore/` | Maintenance tasks |

Examples:
- `feat/gpu-acceleration`
- `fix/checkpoint-corruption`
- `docs/update-algorithm-description`

## Commit Conventions

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation changes |
| `style` | Code style changes (formatting, etc.) |
| `refactor` | Code refactoring |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |
| `perf` | Performance improvements |
| `ci` | CI/CD changes |

### Examples

```
feat(search): add GPU acceleration support
fix(ecc): handle identity point in scalar multiplication
docs(readme): update installation instructions
test(integration): add edge case for zero scalar
chore(deps): update k256 to 0.14
```

## Pull Request Process

1. **Create a branch** from `master`:
   ```bash
   git checkout -b feat/my-feature master
   ```

2. **Make your changes** following coding standards

3. **Run all checks**:
   ```bash
   make all  # Runs lint, test, build
   ```

4. **Commit** with conventional commit message

5. **Push** to your fork:
   ```bash
   git push origin feat/my-feature
   ```

6. **Open a Pull Request** against `master`

7. **Fill out the PR template** completely

### PR Review Checklist

Before requesting review, ensure:

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes (with the curated `pedantic + nursery` lint config)
- [ ] `cargo test --all-targets --all-features` passes (and `cargo test --doc`)
- [ ] `cargo +nightly miri test --workspace --all-features` passes (only required if you touched `unsafe`)
- [ ] New tests added for changed behavior
- [ ] Documentation updated (README, [docs/](docs/README.md), or inline docs)
- [ ] CHANGELOG.md updated for user-facing changes
- [ ] ADR added/updated for substantial design changes
- [ ] If you tuned a hot path, `cargo bench --bench bench -- --baseline current -- --threshold 5` shows <5% regression

### Review Process

- All PRs require at least one approval
- Maintainer may request changes or additional tests
- PRs are squash-merged to keep history clean

## Coding Standards

### Rust Style

- Follow idiomatic Rust conventions
- Use `cargo clippy` warnings as guidance
- Prefer `thiserror` for error types
- Use `anyhow` for application-level errors
- Document public items with `///` comments

### Local pre-commit gate

Before pushing a branch or opening a pull request, run the full
verification suite (mirrors CI):

```bash
# Formatting
cargo fmt --all -- --check

# Strict clippy with the curated pedantic + nursery lint config
cargo clippy --all-targets --all-features -- -D warnings

# Unit + integration + doc + benchmark tests
cargo test --all-targets --all-features
cargo test --doc

# Doc build must be warning-clean
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

### Unsafe-code changes must pass miri

PRs that add or modify `unsafe` code MUST pass [`miri`][miri] under
the [nightly toolchain](https://rust-lang.github.io/rustup/concepts/toolchains.html):

```bash
rustup component add miri --toolchain nightly
cargo +nightly miri setup
cargo +nightly miri test --workspace --all-features
```

The CI workflow's `miri` job runs this on every PR; a local run
before pushing the branch catches the failure earlier. See
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) for the
canonical CI invocation.

[miri]: https://github.com/rust-lang/miri

### Performance regression policy

Tunables that affect hot-path cycle counts (`perform_chunked_sweep`,
`precompute_chunk`, `to_hex_x`, `generate_variants`,
`compute_variant_x_bytes`) MUST not regress by more than **5%** in
the Criterion benchmark suite.

```bash
# Record a baseline (run once before your change set).
cargo bench --bench bench -- --save-baseline current

# Re-record under your change.
cargo bench --bench bench -- --save-baseline your-branch

# Compare. Anything >5% regresses the change and must be justified
# in the commit message or revised.
cargo bench --bench bench -- --baseline current -- --threshold 5
```

The Criterion `target/criterion/` directory is gitignored; baselines
are local-only and machine-dependent.

## Testing Requirements

See [docs/testing.md](docs/testing.md) for the full testing strategy, test categories, and writing guidelines. The summary:

1. **Unit Tests** — In `src/` files, test individual functions
2. **Integration Tests** — In `tests/`, test component interactions
3. **Property-Based Tests** — Use `proptest` for invariant verification
4. **Benchmarks** — In `benches/`, track performance

### Writing Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        // Arrange
        let input = "test";

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected);
    }

    #[test]
    fn test_edge_case_empty_input() {
        // Test edge cases explicitly
    }
}
```

### Test Coverage

- Aim for >80% coverage on new code
- Run `make coverage` to generate reports
- Critical paths require 100% coverage

## Documentation

Documentation lives under [`docs/`](docs/README.md). New substantial design decisions should be captured as an ADR (see [docs/adr/README.md](docs/adr/README.md) for the process and template).

### Required Documentation

- **Public functions**: Add `///` doc comments
- **Complex algorithms**: Reference mathematical proofs in [docs/algorithms.md](docs/algorithms.md)
- **Configuration options**: Document in [docs/configuration.md](docs/configuration.md)
- **CLI flags**: Document in [docs/cli.md](docs/cli.md)
- **Breaking changes**: Update [CHANGELOG.md](CHANGELOG.md) and, if applicable, write an ADR

### Documentation Style

```rust
/// Parses a SEC1 v2.0 encoded public key.
///
/// # Arguments
///
/// * `hex_str` - Hex-encoded public key with optional 0x prefix
///
/// # Returns
///
/// Returns `Ok(EncodedPoint)` on success, or `Err(EccError)` if:
/// - The hex string is invalid
/// - The prefix byte is not 0x02 or 0x03
/// - The point is not on the secp256k1 curve
///
/// # Examples
///
/// ```
/// let pubkey = parse_pubkey("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")?;
/// ```
pub fn parse_pubkey(hex_str: &str) -> Result<EncodedPoint, EccError> {
    // Implementation
}
```

## Questions?

Feel free to open an issue for questions about contributing. We're happy to help!

---

**Principled contributions that advance cryptographic education are always welcome.**
