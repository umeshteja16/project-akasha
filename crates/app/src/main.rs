use clap::{Parser, Subcommand};

use akasha::{run_migrate, run_serve, run_worker, telemetry};
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
    Serve {
        /// Also run the background worker in this process (single-box installs).
        /// Also enabled by `AKASHA_SERVE_WITH_WORKER=true`.
        #[arg(long)]
        with_worker: bool,
    },
    /// Run migrations, then run the background job worker.
    Worker,
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
        Command::Serve { with_worker } => {
            let with_worker = with_worker || config.serve_with_worker;
            run_serve(config, with_worker).await
        }
        Command::Worker => run_worker(config).await,
        Command::Migrate => run_migrate(config).await,
        Command::Openapi => unreachable!("handled above"),
    }
}
