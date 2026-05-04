# Release Process

## Prerequisites

- Maintainer access to push protected `v*` tags.
- Conventional commits on `main` (required by `git-cliff` for changelog generation).
- [git-cliff](https://git-cliff.github.io/) installed locally:

  ```bash
  # cargo
  cargo install git-cliff

  # or Homebrew (macOS / Linux)
  brew install git-cliff

  # or download a pre-built binary from the releases page
  ```

## Steps

### 1. Create a release branch

Branch from `main`:

```bash
git checkout main && git pull
git checkout -b release/v<VERSION>
```

### 2. Bump version

Update the `version` field in `Cargo.toml` to the new stable version (remove any `-beta.N` suffix):

```bash
# Example: 0.1.0-beta.2 → 0.1.0
```

### 3. Update changelog

Generate the changelog entry for the new version from conventional commits:

```bash
git cliff --bump -o CHANGELOG.md
```

Review the diff, the version section header and date are generated automatically.

### 4. Commit

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: prepare v<VERSION>"
```

### 5. Tag and push

Create the tag on the release branch and push both branch and tag:

```bash
git tag v<VERSION>
git push origin release/v<VERSION>
git push origin v<VERSION>
```

`v*` tags are protected — only maintainers with the required permissions can push them.

### 6. CI creates a pre-release

The tag push triggers the pipeline (`.github/workflows/ci.yml`):

1. **check** — formatting, clippy, doc lint
2. **test** — `cargo test`
3. **security** — `cargo audit` + `cargo deny`
4. **build** — cross-compiled binaries for 5 targets (Linux x86_64, Linux ARM64, Windows x86_64, macOS x86_64, macOS ARM64)
5. **release** — runs `git-cliff --latest` to extract release notes, then creates a **GitHub Pre-release** with binaries attached

All tag-pushed releases start as pre-releases, regardless of tag name.

### 7. Open a pull request

Open a PR from `release/v<VERSION>` to `main`. The PR contains the version bump and changelog update. Use the pre-release page on GitHub to verify the generated release notes and attached binaries.

### 8. Review

- Review the PR diff (version bump, changelog entries).
- Review the pre-release page (release notes formatting, binary artifacts).

### 9. Merge

Merge the PR to `main`. The tag remains on the release branch commit — it is an ancestor of `main` through the merge and stays valid.

### 10. Publish the release

1. Go to [Releases](https://github.com/adminelix/mxsend/releases).
2. Find the pre-release, click **Edit**.
3. Click **Publish release** to promote it to a full release.

### 11. Clean up

Delete the release branch:

```bash
git branch -d release/v<VERSION>
git push origin --delete release/v<VERSION>
```

## Notes

- **Beta / pre-release tags**: Tags like `v0.2.0-beta.1` are ignored by `git-cliff` — the `tag_pattern` in `cliff.toml` only matches stable semver (`vX.Y.Z`), so they don't appear in the changelog. A beta tag push still triggers the CI pipeline and creates a pre-release.
- **Versioning**: This project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
- **Changelog**: Maintained in `CHANGELOG.md` using the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format, auto-generated via [`git-cliff`](https://git-cliff.github.io/).
