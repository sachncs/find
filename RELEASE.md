# Release Process

This document describes the release process for the Secp256k1 Find Tool.

## Versioning

This project follows [Semantic Versioning](https://semver.org/):

- **MAJOR** (X.0.0): Incompatible API changes
- **MINOR** (0.X.0): New functionality in a backwards-compatible manner
- **PATCH** (0.0.X): Backwards-compatible bug fixes

### Version Location

The version is defined in `Cargo.toml`:

```toml
[package]
version = "1.0.0"
```

## Pre-Release Checklist

Before creating a release:

- [ ] All tests pass: `make test`
- [ ] Linting passes: `make lint`
- [ ] Documentation is up to date
- [ ] CHANGELOG.md is updated with release notes
- [ ] Version in Cargo.toml is correct
- [ ] No breaking changes unless major version bump

## Release Steps

### 1. Update Version

Update the version in `Cargo.toml`:

```toml
[package]
version = "1.1.0"  # New version here
```

### 2. Update CHANGELOG.md

Add a new section at the top of CHANGELOG.md:

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

### 3. Commit Changes

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore(release): bump version to 1.1.0"
```

### 4. Create Git Tag

```bash
git tag -a v1.1.0 -m "Release v1.1.0"
```

### 5. Push to GitHub

```bash
git push origin master --tags
```

### 6. Automated Release

The GitHub Actions release workflow will automatically:
1. Build binaries for all supported platforms
2. Generate checksums
3. Create a GitHub Release with release notes
4. Upload all artifacts

## Release Workflow

When a tag matching `v*` is pushed, the release workflow (`.github/workflows/release.yml`) triggers:

### Build Matrix

| Target | OS | Artifact |
|--------|-----|----------|
| x86_64-unknown-linux-gnu | Ubuntu | find-x86_64-linux |
| aarch64-unknown-linux-gnu | Ubuntu | find-aarch64-linux |
| x86_64-apple-darwin | macOS | find-x86_64-macos |
| aarch64-apple-darwin | macOS | find-aarch64-macos |
| x86_64-pc-windows-msvc | Windows | find-x86_64-windows.exe |

### Artifacts

Each release includes:
- Platform-specific binaries
- SHA256 checksums for all binaries
- Automatic release notes from commits

## Manual Release

If you need to create a release manually:

### 1. Build Release Binaries

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

### 2. Create Archives

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

### 3. Generate Checksums

```bash
sha256sum find-*.tar.gz find-*.zip > checksums.txt
```

### 4. Create GitHub Release

1. Go to https://github.com/sachn-cs/find/releases/new
2. Select the tag you created
3. Enter release title: `v1.1.0`
4. Paste release notes from CHANGELOG.md
5. Upload all binaries and checksums
6. Click "Publish release"

## Post-Release

After a release:

- [ ] Verify the release on GitHub
- [ ] Test the binaries on at least one platform
- [ ] Announce the release (if applicable)
- [ ] Update any documentation referencing the version

## Hotfix Releases

For critical bug fixes:

1. Create a branch from the release tag:
   ```bash
   git checkout -b hotfix/v1.1.1 v1.1.0
   ```

2. Apply the fix and commit:
   ```bash
   git commit -m "fix: critical issue description"
   ```

3. Update version in Cargo.toml to patch version

4. Update CHANGELOG.md

5. Tag and push:
   ```bash
   git tag -a v1.1.1 -m "Hotfix v1.1.1"
   git push origin hotfix/v1.1.1 --tags
   ```

6. Merge back to master:
   ```bash
   git checkout master
   git merge hotfix/v1.1.1
   git push origin master
   ```

## Yanked Releases

If a release needs to be yanked:

1. Go to the release on GitHub
2. Click "Edit"
3. Check "This is a pre-release" or "This is a draft"
4. Or delete the release entirely

Note: Yanking a release does not delete the tag. Users with the tag checked out will still have the code.

## Release Notes Format

Use [Keep a Changelog](https://keepachangelog.com/) format:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- New features

### Changed
- Changes to existing functionality

### Deprecated
- Features that will be removed

### Removed
- Removed features

### Fixed
- Bug fixes

### Security
- Vulnerability fixes
```

## Questions?

For questions about the release process, open an issue or contact the maintainer.
