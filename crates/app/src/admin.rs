//! Admin commands and startup checks around ML models (ADR 0009):
//! `akasha models download`, `akasha reembed` and the embedding-model guard.

use akasha_core::Config;
use akasha_db::{
    PgPool,
    embeddings::{self, ModelCheck},
};
use akasha_ingest::models as ocr_models;
use akasha_ml::catalog;
use anyhow::{Context, bail};

use crate::jobs::{kinds::EmbedFile, ml::ml_options};

/// Refuse to start when the configured embedding model is not the one the stored
/// vectors came from: mixing two models' vectors silently breaks search. On a
/// fresh database this records the configured model.
pub async fn check_embedding_model(pool: &PgPool, config: &Config) -> anyhow::Result<()> {
    let model = catalog::embed_model(&config.embed_model)?;
    catalog::rerank_model(&config.rerank_model)?;
    let dim = i32::try_from(model.dim)?;
    match embeddings::check_model(pool, model.name, dim).await? {
        ModelCheck::Matches => Ok(()),
        ModelCheck::Mismatch(recorded) => bail!(
            "AKASHA_EMBED_MODEL is `{}` ({} dimensions), but the search index was built with \
             `{}` ({} dimensions). Set AKASHA_EMBED_MODEL={} again, or switch models with \
             `akasha reembed` (re-embeds every file).",
            model.name,
            model.dim,
            recorded.name,
            recorded.dim,
            recorded.name
        ),
        ModelCheck::ColumnMismatch { column_dim } => bail!(
            "AKASHA_EMBED_MODEL `{}` makes {} dimensions but the database stores {column_dim}; \
             run `akasha reembed` to switch.",
            model.name,
            model.dim
        ),
    }
}

/// `akasha reembed`: switch the database to the configured embedding model and
/// queue every file for re-embedding. Stop workers running the old model first.
pub async fn run_reembed(config: Config, force: bool) -> anyhow::Result<()> {
    let model = catalog::embed_model(&config.embed_model)?;
    let pool = akasha_db::connect(&config.database_url, 2)
        .await
        .context("connecting to database")?;
    akasha_db::migrate(&pool).await?;
    let recorded = embeddings::recorded(&pool).await?;
    let same = recorded
        .as_ref()
        .is_some_and(|r| r.name == model.name && usize::try_from(r.dim).ok() == Some(model.dim));
    if same && !force {
        println!(
            "Already using `{}`; nothing to do (pass --force to re-embed everything anyway).",
            model.name
        );
        return Ok(());
    }
    if model.dim > embeddings::MAX_INDEXED_DIM {
        bail!(
            "`{}` has {} dimensions; the vector index supports at most {}",
            model.name,
            model.dim,
            embeddings::MAX_INDEXED_DIM
        );
    }

    let mut tx = pool.begin().await?;
    let files = embeddings::reset(&mut tx, model.name, model.dim).await?;
    for file_id in &files {
        akasha_jobs::enqueue(&mut tx, &EmbedFile { file_id: *file_id }).await?;
    }
    tx.commit().await?;
    let from = recorded.map_or_else(|| "nothing".to_owned(), |r| format!("`{}`", r.name));
    println!(
        "Switched from {from} to `{}` ({} dimensions) and queued {} file(s) for embedding.\n\
         Run workers with AKASHA_EMBED_MODEL={}; files are searchable by keyword meanwhile.",
        model.name,
        model.dim,
        files.len(),
        model.name
    );
    Ok(())
}

/// `akasha models download`: fetch every configured model into `models_dir`, so a
/// machine can run offline afterwards (copy the directory to air-gapped hosts).
pub async fn run_models_download(config: Config) -> anyhow::Result<()> {
    let embed = catalog::embed_model(&config.embed_model)?;
    let rerank = catalog::rerank_model(&config.rerank_model)?;
    let options = ml_options(&config);
    let dir = options.models_dir.clone();
    let ocr_url = config.ocr_models_url.clone();
    let ocr = config.ocr_enabled;
    let whisper = if config.transcribe_enabled {
        Some(akasha_media::models::find(&config.whisper_model)?)
    } else {
        None
    };
    let hf = config.ml_models_url.clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if ocr {
            for model in [ocr_models::DETECTION, ocr_models::RECOGNITION] {
                ocr_models::ensure(&dir, &ocr_url, &model)?;
                println!("ocr: {} ok", model.name);
            }
        } else {
            println!("ocr: disabled, skipped");
        }
        akasha_ml::download_embedder(embed, &options)?;
        println!("embedding: {} ok", embed.name);
        match rerank {
            Some(model) => {
                akasha_ml::download_reranker(model, &options)?;
                println!("rerank: {} ok", model.name);
            }
            None => println!("rerank: disabled, skipped"),
        }
        match whisper {
            Some(akasha_media::models::ModelChoice::Whisper(model)) => {
                akasha_media::models::ensure(&dir, &hf, model)?;
                println!("transcription: whisper {} ok", model.name);
            }
            Some(akasha_media::models::ModelChoice::Fake) => {
                println!("transcription: test model, nothing to download");
            }
            None => println!("transcription: disabled, skipped"),
        }
        Ok(())
    })
    .await
    .context("download task")??;
    println!("Models are in {}", config.models_dir);
    Ok(())
}

/// `akasha models check`: load the configured embedding model, reranker and
/// speech model (downloading them if needed) and run one inference each, to verify
/// an install (model files, ONNX Runtime library, whisper.cpp build) before serving.
pub async fn run_models_check(config: Config) -> anyhow::Result<()> {
    let ml = crate::jobs::ml::MlProvider::from_config(&config);
    let started = std::time::Instant::now();
    let embedder = ml.embedder().await?;
    let vector = tokio::task::spawn_blocking(move || embedder.embed_query("hello world"))
        .await
        .context("embedding task")??;
    println!(
        "embedding: {} ok ({} dimensions, {:?})",
        config.embed_model,
        vector.len(),
        started.elapsed()
    );
    let started = std::time::Instant::now();
    match ml.reranker().await? {
        Some(reranker) => {
            let name = reranker.name();
            let scores = tokio::task::spawn_blocking(move || {
                reranker.score("greeting", &["hello world", "tax return"])
            })
            .await
            .context("rerank task")??;
            println!(
                "rerank: {name} ok (scores {scores:?}, {:?})",
                started.elapsed()
            );
        }
        None => println!("rerank: disabled"),
    }
    let started = std::time::Instant::now();
    let transcriber = crate::jobs::transcribe::TranscriberProvider::from_config(&config);
    match transcriber.get().await? {
        Some(model) => {
            // Two seconds of a quiet tone: proves the model loads and runs here.
            let samples: Vec<f32> = (0..akasha_media::SAMPLE_RATE * 2)
                .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 0.1)
                .collect();
            let segments = tokio::task::spawn_blocking(move || {
                model.transcribe(&samples, &akasha_media::Control::default())
            })
            .await
            .context("transcription task")??;
            println!(
                "transcription: {} ok ({} segments, {:?}, cpu build {})",
                config.whisper_model,
                segments.len(),
                started.elapsed(),
                crate::cpu::build_variant()
            );
        }
        None => println!("transcription: disabled"),
    }
    Ok(())
}
