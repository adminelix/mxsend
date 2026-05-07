# Contributing

Thank you for your interest in contributing to `mxsend`!

## Git Workflow

This project follows the [OneFlow](https://www.oneflow.org/) branching model:

- **`main`** is the default branch and the only long-lived branch. It is protected on GitHub — all changes must go through a pull request.
- Feature branches are created from `main` and merge back into `main`. The choice of merge strategy is left to the developer — see [Mitchell Hashimoto's gist](https://gist.github.com/mitchellh/319019b1b8aac9110fcfb1862e0c97fb) for guidance on when each approach fits best.
- No `develop` branch — `main` always reflects the latest delivered state.
- Release branches tag from `main`; hotfix branches branch from the tag and merge back to `main`.

## Getting Started

1. Fork the repository.
2. Create a feature branch (`git checkout -b feature/my-feature`).
3. Make your changes.
4. Ensure the code passes all checks:
   ```bash
   cargo fmt
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   ```
5. Submit a pull request with a clear description.

## Code of Conduct

Be respectful and constructive in all interactions.

## License

By contributing, you agree that your contributions will be licensed under
the MIT OR Apache-2.0 license.
