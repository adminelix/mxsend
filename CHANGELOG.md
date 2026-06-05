# Changelog
## [v0.2.0] - 2026-06-05

### Features
- add --no-tls-verify flag for disabling TLS certificate verification

### Bug Fixes
- update download URLs from linux-gnu to linux-musl

<details>
<summary>Other</summary>

### CI
- remove unused update_release input from release step
- switch Linux build targets from glibc to musl
- install zig via setup-zig action for cargo-zigbuild
- extract security audit into reusable scheduled workflow (#26)

### Chores
- bump actions/cache from 4 to 5
- bump tokio from 1.52.1 to 1.52.3
- bump matrix-sdk from 0.16.0 to 0.16.1
- update all transitive deps via cargo update
- simplify version specs to major.minor format
- bump matrix-sdk from 0.16 to 0.17
- bump version to 0.1.2-beta.0
- bump serde_json from 1.0.149 to 1.0.150
- bump reqwest from 0.13.3 to 0.13.4
- bump http from 1.4.0 to 1.4.1
- bump serial_test from 3.4.0 to 3.5.0
</details>

## [v0.1.1] - 2026-05-08

### Bug Fixes
- prevent custom room from being mistaken for DM room
- collapse nested if in test to satisfy clippy

<details>
<summary>Other</summary>

### CI
- fix git-cliff crash on pre-release tags by using explicit --tag flag
- use --latest flag to output only current version in release notes

### Chores
- prepare v0.1.1

### Documentation
- reference Mitchell Hashimoto's gist on merge strategy
</details>

## [v0.1.0] - 2026-05-07

### Features
- add room member filter to sync settings for lazy loading
- add session management and dependency updates for Matrix client
- switch to a data_dir with default
- provide env vars for configuration
- enable e2e-encryption and add recovery key option
- add CLI logging with -v/-vv verbosity flags
- implement sending messages to room IDs
- generate release notes from conventional commits via git-cliff
- support stdin as message source
- graceful logout on SIGINT/SIGTERM during send
- validate recipient existence before sending
- render messages as Markdown by default with --plain flag

### Bug Fixes
- prevent duplicate DM rooms on multiple runs
- replace hardcoded localhost with container host and dynamic server_name
- escape newlines properly in release description
- detect Windows target by triple in artifact packaging
- use body_path for release notes to avoid YAML escaping issues

<details>
<summary>Other</summary>

### CI
- add GitLab CI configuration
- add GitLab CI pipeline with cross-compilation and release automation
- use conditional install for cargo-audit and cargo-deny
- cache cargo metadata files to fix reinstall errors
- use --force flag for cargo-audit and cargo-deny installs
- add GitLab pipeline with cross-compilation
- add GitLab pipeline with cross-compilation
- pre-build custom image with tools, move Docker env to runner config
- move build-ci-image to prepare stage
- add macOS darwin cross-compilation builds
- add explicit linker for macOS cross-compilation targets
- remove trailing whitespace breaking shell line continuation in release job
- migrate from GitLab CI to GitHub Actions
- fail pipeline on cargo audit findings
- upgrade intel mac runner from macos-13 to macos-26
- upgrade arm mac runner from macos-latest to macos-26
- add Dependabot config for weekly dependency updates
- add caching to speed up GitHub Actions workflow
- skip cargo install on cache hit in security job

### Chores
- initial commit
- rename project to matrix-send
- add .gitignore
- remove accidentally committed .idea folder
- update dependencies
- bump version to 0.1.0-rc.1
- add Unicode-3.0 and BSL-1.0 to cargo-deny license allowlist
- add NOTICE file and SPDX headers for license compliance
- bump actions/checkout from 4 to 6
- bump actions/upload-artifact from 4 to 7
- bump softprops/action-gh-release from 2 to 3
- bump actions/download-artifact from 4 to 8
- bump version to 0.1.0-beta.1
- bump version to 0.1.0-beta.2
- add release process documentation and tooling
- prepare v0.1.0

### Documentation
- add README
- update project description
- rewrite README with project purpose and usage guide
- add pipeline overview comment, enable release update
- simplify README and unversion release artifacts
- note pre-release promotion requirement in CI header
- fix broken intra-doc link to MessageSender::send_internal
- simplify CI pipeline header comment
- document OneFlow branching model in contributing guide

### Refactor
- improve code structure and error handling for room determination and message sending
- streamline CLI argument descriptions and session restoration logic
- remove unused import of url
- find proper function name
- improve error handling and avoid panics
- improve session management and env variable usage in build script and main code
- split lib/bin crate and improve code quality
- redesign lib API with MessageSender builder, Recipient enum, and SendOptions
- rename variables for clarity and consistency
- move serial annotation from module to individual tests
- clean up public API and file ordering
- rename project to mxsend
- use typed Interrupted error and admin API test verification

### Styling
- format code

### Testing
- add integration test with synapse testcontainers

### Build
- update CLI parsing from structopt to clap and add new dependencies
- configure optimized release and development profiles
- bump dependencies (tokio 1.52, clap 4.6, anyhow 1.0.102, reqwest 0.13, serial_test 3.4)
</details>

<!-- generated by git-cliff -->
