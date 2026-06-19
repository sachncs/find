# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive open source documentation (CODE_OF_CONDUCT.md, CONTRIBUTING guidelines)
- GitHub issue templates for bug reports and feature requests
- Dependabot configuration for automated dependency updates
- EditorConfig for consistent code formatting across editors
- Gitattributes for line ending normalization
- GitHub Sponsors funding configuration

### Changed
- Enhanced README with improved structure, badges, and roadmap
- Expanded CONTRIBUTING.md with detailed development guidelines
- Improved .gitignore with comprehensive coverage for IDEs, OS files, and project artifacts

### Fixed
- Repository metadata consistency

## [1.0.0] - 2026-04-12

### Added
- **High-Performance Rust Core**: Replaced the Python prototype with a production-grade Rust implementation using `k256` and `rayon`.
- **512-Variant Search Engine**: Implemented range-splitting using powers of 2 ($2^0..2^{255}$) and cumulative summations.
- **Ambiguity Handling**: Added explicit candidate disambiguation to handle Y-parity during X-coordinate matching ($v \pm j$).
- **Structured Observability**: Added non-blocking rolling file logs using `tracing-appender` and daily logs in the `./logs` directory.
- **Export Capabilities**: Added JSON export for generated subtraction variants via the `--output-dir` flag.
- **Comprehensive Testing**: Added property-based tests (`proptest`), unit tests for edge cases, and robust integration tests.
- **Mathematical Documentation**: Added deep architectural and mathematical documentation across the codebase.

### Changed
- Refactored error handling to use `thiserror` for unified, contextual error reporting.
- Optimized critical point arithmetic paths to minimize allocations and redundant coordinate conversions.

### Fixed
- Fixed a panic condition in the variant generator when a subtraction resulted in the Identity point (point at infinity).
- Corrected out-of-range scalar scalar conversion logic for BigUint summations exceeding the curve order.

## [0.1.2] - 2026-04-26

### Fixed
- Minor search optimization fix

## [0.1.1] - 2026-04-26

### Added
- GitHub Actions CI workflow with multi-platform testing (Ubuntu, macOS, Windows)
- Pull request template with review checklist
- CODEOWNERS file for automatic reviewer assignment
- SECURITY.md with vulnerability reporting policy
- Extended error handling with domain-specific error types
- Orchestrator module for session management and resume
- Persistence module for atomic checkpoint operations
- Expanded test suite with orchestrator and audit tests

### Changed
- Refactored search engine with improved parallelism
- Enhanced documentation and testing strategy

## [0.1.0] - 2026-04-25

### Added
- Major refactoring of core search engine
- New orchestrator module for session management
- Persistence module for checkpoint handling
- Improved test coverage with integration and audit tests
- Enhanced error handling and reporting

### Changed
- Refactored ECC module for better code organization
- Updated dependencies and build configuration

## [0.0.2] - 2026-04-15

### Added
- Enhanced algorithm documentation
- Improved ECC point arithmetic
- Extended error handling
- Better CLI interface with checkpoint support
- Parallel search with batch normalization
- Comprehensive test suite

### Changed
- Refactored search engine for better performance
- Updated README with detailed architecture

## [0.0.1] - 2026-04-13

### Added
- Initial release of secp256k1 find tool
- Basic search functionality
- SEC1 public key parsing
- Parallel sweep engine
- CLI interface with basic options
