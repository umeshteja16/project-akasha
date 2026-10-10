use std::path::PathBuf;

use clap::{Parser, Subcommand};

use akasha::{admin, eval, run_migrate, run_serve, run_worker, telemetry};
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
    /// Serve MCP over stdio for local AI clients (Claude Desktop, IDEs) by
    /// forwarding to a running Akasha server's `/mcp` endpoint with an API token.
    Mcp {
        /// The Akasha server's base URL.
        #[arg(long, env = "AKASHA_URL", default_value = "http://127.0.0.1:8080")]
        url: String,
        /// A personal API token (Settings → Access tokens). Prefer the env var:
        /// command-line arguments are visible to other local users.
        #[arg(long, env = "AKASHA_TOKEN", hide_env_values = true)]
        token: String,
    },
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
    /// Measure search quality (Recall@k, MRR, nDCG@10, latency) on the benchmark in
    /// `eval/`, in a scratch database next to DATABASE_URL, and fail if it regressed
    /// against the committed baseline.
    Eval {
        /// Use the configured models (AKASHA_EMBED_MODEL, AKASHA_RERANK_MODEL)
        /// instead of the deterministic built-in ones. Downloads them if needed.
        #[arg(long)]
        real_models: bool,
        /// Directory with `queries.json`, `corpus/` and `baselines/`.
        #[arg(long, default_value = "eval")]
        dir: PathBuf,
        /// Where to write the full JSON report [default: target/eval/<models>.json].
        #[arg(long)]
        out: Option<PathBuf>,
        /// Record this run as the baseline instead of comparing against it.
        #[arg(long)]
        update_baseline: bool,
        /// Largest allowed drop of any quality metric.
        #[arg(long, default_value_t = eval::DEFAULT_TOLERANCE)]
        tolerance: f64,
    },
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Download every configured model (OCR, embedding, rerank, Whisper) into
    /// AKASHA_MODELS_DIR, for offline and air-gapped installs.
    Download,
    /// Load the configured embedding model and reranker and run one inference
    /// each (verifies model files and the ONNX Runtime library).
    Check,
}

fn main() -> anyhow::Result<()> {
    // Before any thread starts: may replace this process with the AVX2 build.
    akasha::cpu::dispatch();
    run()
}

#[tokio::main]
async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Command::Openapi = cli.command {
        println!("{}", akasha::openapi_json()?);
        return Ok(());
    }

    // Needs neither the database nor the config; stdout is the protocol.
    if let Command::Mcp { url, token } = cli.command {
        return akasha::mcp::bridge::run(akasha::mcp::bridge::BridgeOptions { url, token }).await;
    }

    let config = Config::load()?;
    let _telemetry = telemetry::init(config.log_format);

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
        Command::Eval {
            real_models,
            dir,
            out,
            update_baseline,
            tolerance,
        } => {
            let opts = eval::EvalOptions {
                real_models,
                dir,
                out,
                update_baseline,
                tolerance,
            };
            eval::run_cli(config, opts).await
        }
        Command::Openapi | Command::Mcp { .. } => unreachable!("handled above"),
    }
}
