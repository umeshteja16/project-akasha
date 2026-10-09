use clap::{Parser, Subcommand};

use akasha::{run_migrate, run_serve, telemetry};
use akasha_core::Config;

/// Akasha: a self-hosted personal knowledge retrieval server.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run migrations, then serve the HTTP API.
    Serve,
    /// Apply database migrations and exit.
    Migrate,
    /// Print the OpenAPI document as JSON and exit.
    Openapi,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Openapi = cli.command {
        println!("{}", akasha::openapi_json()?);
        return Ok(());
    }

    let config = Config::load()?;
    telemetry::init(config.log_format);

    match cli.command {
        Command::Serve => run_serve(config).await,
        Command::Migrate => run_migrate(config).await,
        Command::Openapi => unreachable!("handled above"),
    }
}
