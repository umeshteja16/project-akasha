use clap::{Parser, Subcommand};

use akasha::{admin, run_migrate, run_serve, run_worker, telemetry};
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
    /// Manage ML model files.
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
    /// Switch to the configured embedding model (AKASHA_EMBED_MODEL): drop all
    /// vectors, resize the vector column and queue every file for re-embedding.
    /// Stop workers running the old model first.
    Reembed {
        /// Re-embed even if the configured model is already in use.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Download every configured model (OCR, embedding, rerank) into
    /// AKASHA_MODELS_DIR, for offline and air-gapped installs.
    Download,
    /// Load the configured embedding model and reranker and run one inference
    /// each (verifies model files and the ONNX Runtime library).
    Check,
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
        Command::Models {
            command: ModelsCommand::Download,
        } => admin::run_models_download(config).await,
        Command::Models {
            command: ModelsCommand::Check,
        } => admin::run_models_check(config).await,
        Command::Reembed { force } => admin::run_reembed(config, force).await,
        Command::Openapi => unreachable!("handled above"),
    }
}
