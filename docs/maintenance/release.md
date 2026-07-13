# Release Process

This document describes the release process for the `find` tool. For the versioning policy and supported-versions matrix, see [roadmap.md](../roadmap.md).

## Versioning

This project follows [Semantic Versioning](https://semver.org/):

- **MAJOR** (X.0.0): Incompatible API changes.
- **MINOR** (0.X.0): New functionality in a backwards-compatible manner.
- **PATCH** (0.0.X): Backwards-compatible bug fixes.

### Version location

The version is defined in `Cargo.toml`:

```toml
[package]
version = "1.1.0"  # Updated by the release process
```

## Pre-release checklist

Before creating a release, verify:

- [ ] All tests pass: `make test`
- [ ] Linting passes: `make lint`
- [ ] Benchmarks are healthy: `make bench` shows no regressions
- [ ] Documentation is up to date: `docs/README.md` is the index, and all cross-doc links resolve
- [ ] [CHANGELOG.md](../../CHANGELOG.md) is updated with the new version's release notes
- [ ] Version in `Cargo.toml` is correct
- [ ] No breaking changes unless a major version bump
- [ ] `make all` runs cleanly (lint + test + build)

## Release steps

### 1. Update the version

Edit `Cargo.toml`:

```toml
[package]
version = "1.1.0"  # New version
```

### 2. Update CHANGELOG.md

Add a new section at the top of [CHANGELOG.md](../../CHANGELOG.md) following [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
## [1.1.0] - 2026-05-01

### Added
- New feature X

### Changed
- Improved Y

### Fixed
- Fixed Z

### Removed
- Removed deprecated feature W
```

### 3. Commit the changes

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore(release): bump version to 1.1.0"
```

### 4. Create the git tag

```bash
git tag -a v1.1.0 -m "Release v1.1.0"
```

The `v*` pattern triggers the release workflow — see [the release workflow](../../.github/workflows/release.yml).

### 5. Push to GitHub

```bash
git push origin master --tags
```

The tag push triggers the GitHub Actions release workflow (`.github/workflows/release.yml`).

### 6. Automated release

The GitHub Actions release workflow automatically:

1. Builds binaries for all supported platforms (see [build matrix](#build-matrix)).
2. Generates SHA256 checksums.
3. Creates a GitHub Release with auto-generated release notes.
4. Uploads all artifacts.

## Build matrix

When a tag matching `v*` is pushed, `.github/workflows/release.yml` triggers. The build matrix:

| Target | OS | Artifact name |
|---|---|---|
| `x86_64-unknown-linux-gnu` | Ubuntu | `find-x86_64-linux` |
| `aarch64-unknown-linux-gnu` | Ubuntu | `find-aarch64-linux` |
| `x86_64-apple-darwin` | macOS | `find-x86_64-macos` |
| `aarch64-apple-darwin` | macOS | `find-aarch64-macos` |
| `x86_64-pc-windows-msvc` | Windows | `find-x86_64-windows.exe` |

### Cross-compilation notes

- **Linux aarch64** uses the `gcc-aarch64-linux-gnu` cross-compiler; the linker environment variable is set automatically.
- **macOS universal binaries** are not built; each architecture is shipped as a separate artifact.
- **Windows MSVC** uses the `stable-x86_64-pc-windows-msvc` toolchain.

## Artifacts

Each release includes:

- Platform-specific binaries (one per matrix entry).
- SHA256 checksums in `checksums.txt`.
- Auto-generated release notes from commits (via `softprops/action-gh-release`).

The release is published as a non-draft, non-prerelease unless the tag name contains a hyphen (e.g. `v1.1.0-rc.1`).

## Manual release (fallback)

If the GitHub Actions release workflow fails, a manual release is possible.

### 1. Build release binaries

```bash
# Build for current platform
cargo build --release

# Cross-compile for other platforms
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-pc-windows-msvc
```

### 2. Create archives

```bash
# Linux x86_64
tar -czf find-x86_64-linux.tar.gz -C target/x86_64-unknown-linux-gnu/release find

# Linux aarch64
tar -czf find-aarch64-linux.tar.gz -C target/aarch64-unknown-linux-gnu/release find

# macOS x86_64
tar -czf find-x86_64-macos.tar.gz -C target/x86_64-apple-darwin/release find

# macOS aarch64
tar -czf find-aarch64-macos.tar.gz -C target/aarch64-apple-darwin/release find

# Windows (using zip)
zip find-x86_64-windows.zip target/x86_64-pc-windows-msvc/release/find.exe
```

### 3. Generate checksums

```bash
sha256sum find-*.tar.gz find-*.zip > checksums.txt
```

### 4. Create the GitHub release

1. Go to <https://github.com/sachncs/find/releases/new>.
2. Select the tag you created.
3. Enter release title: `v1.1.0`.
4. Paste release notes from [CHANGELOG.md](../../CHANGELOG.md).
5. Upload all binaries and `checksums.txt`.
6. Click **Publish release**.

## Post-release

After a release is published:

- [ ] Verify the release is visible on the [Releases page](https://github.com/sachncs/find/releases).
- [ ] Test the binaries on at least one platform.
- [ ] Announce the release in any project-relevant channels.
- [ ] Update any documentation that references the version number.

## Hotfix releases

For critical bug fixes:

1. Create a branch from the release tag:
   ```bash
   git checkout -b hotfix/v1.1.1 v1.1.0
   ```

2. Apply the fix and commit:
   ```bash
   git commit -m "fix: critical issue description"
   ```

3. Update the version in `Cargo.toml` to the patch version.

4. Update [CHANGELOG.md](../../CHANGELOG.md).

5. Tag and push:
   ```bash
   git tag -a v1.1.1 -m "Hotfix v1.1.1"
   git push origin hotfix/v1.1.1 --tags
   ```

6. Merge back to `master`:
   ```bash
   git checkout master
   git merge hotfix/v1.1.1
   git push origin master
   ```

## Yanked releases

If a release needs to be retracted (e.g. a critical bug was found after publication):

1. Go to the release on GitHub.
2. Click **Edit**.
3. Check **This is a pre-release** to hide it from the default release list.
4. Optionally delete the release entirely.

**Note:** Yanking a release does not delete the tag. Users with the tag checked out will still have the code. To force a re-publish, increment the patch version (e.g. `v1.0.1`) and re-tag.

## Release notes format

The release notes use the [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- New features

### Changed
- Changes to existing functionality

### Deprecated
- Features that will be removed in a future release

### Removed
- Removed features

### Fixed
- Bug fixes

### Security
- Vulnerability fixes
```

The `### Security` section is for vulnerability fixes that should be called out prominently in the release notes.

## See also

- [roadmap.md](../roadmap.md) — Versioning policy, supported versions
- [CHANGELOG.md](../../CHANGELOG.md) — Historical release notes
- [CONTRIBUTING.md](../../CONTRIBUTING.md) — Contribution workflow
- [security.md](../security.md) — Security model
