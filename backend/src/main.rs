use clap::Parser;

use dra_server::cli;
use dra_server::config::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `.env` is a development convenience; real environments pass arguments or env vars.
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    cli::init_tracing(&cli.log);
    cli::run(cli).await
}
