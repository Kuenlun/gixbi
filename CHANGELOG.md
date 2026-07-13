# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/Kuenlun/gixbi/compare/v0.1.0...v0.1.1) - 2026-07-13

### Fixed

- neutralize hostile commit data in the rendered report

### Other

- determine versions from git tags, the crate lives outside registries
- release v0.1.0 ([#1](https://github.com/Kuenlun/gixbi/pull/1))

## [0.1.0](https://github.com/Kuenlun/gixbi/releases/tag/v0.1.0) - 2026-07-13

### Added

- cargo-themed CLI with color policy and shell completions
- git-graph style terminal renderer
- branch interaction analysis core
- backend-agnostic repository access over gix and git2

### Fixed

- harden backends for shallow clones, unborn HEAD and unreadable objects

### Other

- lockpick pipeline, pinned toolchain and release-plz binary releases
- README with graph legend and usage
- exhaustive unit and integration suites behind a coverage gate
- hand tip indices out of the arena walk
- scaffold project with toolchain and quality-gate config
