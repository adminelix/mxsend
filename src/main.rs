// SPDX-FileCopyrightText: 2026 mxsend contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::io::{IsTerminal, Read};

use clap::Parser;
use clap_verbosity_flag::Verbosity;
use matrix_sdk::ruma::OwnedUserId;
use mxsend::{Interrupted, MessageSender, Recipient, SendOptions};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{FilterExt, Targets};
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Sender's Matrix user ID (e.g. @alice:server.com)
    #[arg(long = "from", short = 'f', env = "MXSEND_FROM")]
    from: OwnedUserId,

    /// Sender's account password for login
    #[arg(long = "password", short = 'p', env = "MXSEND_PASSWORD")]
    password: String,

    /// The recipient — a Matrix user ID (@user:server) or room ID (!room:server)
    #[arg(long = "to", short = 't', env = "MXSEND_TO")]
    to: Recipient,

    /// Recovery key to verify the sender's E2EE device (optional)
    #[arg(long = "recovery-key", short = 'k', env = "MXSEND_RECOVERY_KEY")]
    recovery_key: Option<String>,

    /// Verbosity level (use -v, -vv, -vvv, or -q to suppress output)
    #[command(flatten)]
    verbosity: Verbosity,

    /// Plain text message body to send.
    /// If omitted, the message is read from stdin (e.g. via pipe).
    message: Option<String>,
}

impl Cli {
    fn into_send_options(self) -> SendOptions {
        let mut stdin = std::io::stdin().lock();
        let is_terminal = stdin.is_terminal();
        let message = resolve_message(self.message, &mut stdin, is_terminal).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });

        SendOptions {
            from: self.from,
            password: self.password,
            to: self.to,
            recovery_key: self.recovery_key,
            verbosity: self.verbosity,
            message,
        }
    }
}

/// Resolve the message from a CLI argument or by reading from a reader.
///
/// When `message` is `None` or empty, reads from `reader` unless `is_terminal`
/// is `true` (in which case the user forgot to provide input).
fn resolve_message(
    message: Option<String>,
    reader: &mut impl Read,
    is_terminal: bool,
) -> Result<String, String> {
    match message {
        Some(msg) if !msg.is_empty() => Ok(msg),
        _ => {
            if is_terminal {
                return Err(
                    "No message provided. Pass as an argument or pipe input via stdin.".into(),
                );
            }
            let mut buf = String::new();
            reader
                .read_to_string(&mut buf)
                .map_err(|e| format!("Error reading stdin: {e}"))?;
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                return Err(
                    "No message provided. Pass as an argument or pipe input via stdin.".into(),
                );
            }
            Ok(trimmed.to_string())
        }
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let opts = cli.into_send_options();

    let verbosity_level = opts.verbosity.tracing_level_filter();

    let app_filter = Targets::new()
        .with_target("mxsend", verbosity_level)
        .with_default(tracing_subscriber::filter::LevelFilter::OFF);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("mxsend=off"));

    let combined_filter = env_filter.or(app_filter);

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .compact()
                .with_target(false)
                .without_time()
                .with_filter(combined_filter),
        )
        .init();

    match MessageSender::new(opts).send().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) if e.downcast_ref::<Interrupted>().is_some() => std::process::ExitCode::from(130),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_message_from_arg() {
        let result = resolve_message(Some("hello".into()), &mut "".as_bytes(), true);
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_resolve_message_from_reader() {
        let result = resolve_message(None, &mut "piped message".as_bytes(), false);
        assert_eq!(result.unwrap(), "piped message");
    }

    #[test]
    fn test_resolve_message_multiline() {
        let result = resolve_message(None, &mut "line one\nline two\n".as_bytes(), false);
        assert_eq!(result.unwrap(), "line one\nline two");
    }

    #[test]
    fn test_resolve_message_fails_when_terminal_without_arg() {
        let result = resolve_message(None, &mut "".as_bytes(), true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No message provided"));
    }

    #[test]
    fn test_resolve_message_fails_when_reader_empty() {
        let result = resolve_message(None, &mut "".as_bytes(), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No message provided"));
    }

    #[test]
    fn test_resolve_message_fails_when_reader_whitespace_only() {
        let result = resolve_message(None, &mut "   \n  \n  ".as_bytes(), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No message provided"));
    }

    #[test]
    fn test_resolve_message_empty_arg_falls_through_to_reader() {
        let result = resolve_message(Some("".into()), &mut "from stdin".as_bytes(), false);
        assert_eq!(result.unwrap(), "from stdin");
    }

    #[test]
    fn test_into_send_options_passes_message() {
        let cli = Cli {
            from: "@u:localhost".try_into().unwrap(),
            password: "pass".into(),
            to: Recipient::User("@r:localhost".try_into().unwrap()),
            recovery_key: None,
            verbosity: Default::default(),
            message: Some("test message".into()),
        };
        let opts = cli.into_send_options();
        assert_eq!(opts.message, "test message");
        assert_eq!(opts.from.to_string(), "@u:localhost");
        assert_eq!(opts.to, Recipient::User("@r:localhost".try_into().unwrap()));
    }
}
