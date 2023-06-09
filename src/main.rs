mod server;
mod util;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::server::Server;

fn init_tracing() {
    // #TODO RUST_LOG is not passed from vscode, investigate.

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_ansi(false);

    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}

fn main() -> anyhow::Result<()> {
    init_tracing();

    // #TODO mut sucks here.
    let mut server = Server::new();
    server.run()?;

    Ok(())
}
