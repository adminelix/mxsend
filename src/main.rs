use clap::Parser;
use mxsend::{MessageSender, SendOptions};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{FilterExt, Targets};
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn main() {
    let opts = SendOptions::parse();

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

    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    if let Err(e) = rt.block_on(MessageSender::new(opts).send()) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
