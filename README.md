# mxsend

> A tiny, stateless CLI tool for sending Matrix notifications from servers and scripts.

`mxsend` is the Matrix equivalent of `mail` or `mailsend`: it logs in, delivers a message, and logs out. No background daemon, no persistent store, no configuration files. It is purpose-built for one-shot notifications from cron jobs, CI pipelines, monitoring alerts, and administrative scripts.

## Features

- **Stateless** — No local credential cache, E2EE store, or config files. Every run is independent.
- **Cross-platform** — Single static Rust binary. Runs on any platform supported by the Rust toolchain.
- **Direct messages & rooms** — Send to a user ID (`@user:server`) to auto-create or reuse a DM, or target a room ID (`!room:server`) directly.
- **End-to-end encryption** — Optional E2EE support via recovery key verification.
- **Environment variables** — All options can be passed via `MXSEND_*` env vars for clean scripting.
- **Lightweight** — Minimal dependencies, fast compile, small release binary.

## How it differs from matrix-commander

| | **mxsend** | **matrix-commander** |
|---|---|---|
| **Scope** | One-shot notification sender | Full-featured Matrix CLI client |
| **State** | Stateless — nothing stored locally | Persistent E2EE store, credential cache, and device state |
| **Use case** | Server alerts, cron jobs, CI pipelines | Interactive messaging, bots, listening for events |
| **Capabilities** | Send text to user/room, optional E2EE | Send/receive files, listen for messages, manage rooms, SSO, emoji verification, etc. |
| **Footprint** | Single minimal binary | Larger Python package with storage directory |

If you need a general-purpose Matrix client on the command line, `matrix-commander` is the better choice. If you just need to fire off a notification and forget, use `mxsend`.

## Installation

### Prerequisites

- A Rust toolchain (see [rustup.rs](https://rustup.rs/))

### Build from source

```bash
git clone https://gitlab.com/adminelix/mxsend.git
cd mxsend
cargo build --release
```

The binary will be available at `target/release/mxsend`.

### Install with Cargo

```bash
cargo install --path .
```

## Usage

### Command line

```bash
mxsend \
  --from "@bot:example.com" \
  --password "s3cr3t" \
  --to "@admin:example.com" \
  "Server backup completed successfully."
```

Send to a room instead:

```bash
mxsend \
  -f "@bot:example.com" \
  -p "s3cr3t" \
  -t "!alerts:example.com" \
  "Disk usage above 90%."
```

### Environment variables

For cleaner scripting and to avoid credentials in process lists, use environment variables:

```bash
export MXSEND_FROM="@bot:example.com"
export MXSEND_PASSWORD="s3cr3t"
export MXSEND_TO="@admin:example.com"

mxsend "Cron job finished at $(date)"
```

### End-to-end encryption

If your server requires E2EE, provide a recovery key:

```bash
mxsend \
  -f "@bot:example.com" \
  -p "s3cr3t" \
  -t "@admin:example.com" \
  -k "YOUR_RECOVERY_KEY" \
  "Sensitive alert: unauthorized access detected."
```

## Development

```bash
# Quick syntax check
cargo check

# Run tests
cargo test

# Run integration tests (requires Docker / Podman for testcontainers)
cargo test --test integration_test

# Format code
cargo fmt

# Linting
cargo clippy
```

## Project

- **Repository:** https://gitlab.com/adminelix/mxsend

