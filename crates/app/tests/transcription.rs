//! Audio and video transcription end to end, with the deterministic tone
//! "model" (`AKASHA_WHISPER_MODEL=fake`): a generated WAV file is uploaded,
//! decoded, resampled and transcribed into timestamped chunks that search
//! results and chat citations point into.

mod support;

use akasha_core::Config;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    TestApp,
    chat::{ask, conversation, find},
    test_config,
};

/// A mono 16-bit WAV file: 3 s of 440 Hz, 1 s of silence, 3 s of 880 Hz, at 44.1 kHz.
fn tones_wav() -> Vec<u8> {
    let rate = 44_100u32;
    let mut samples: Vec<i16> = Vec::new();
    let mut tone = |freq: f64, seconds: u32| {
        for i in 0..rate * seconds {
            let t = f64::from(i) / f64::from(rate);
            samples.push(((2.0 * std::f64::consts::PI * freq * t).sin() * 12_000.0) as i16);
        }
    };
    tone(440.0, 3);
    tone(0.0, 1);
    tone(880.0, 3);
    let data = u32::try_from(samples.len() * 2).expect("size");
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn id(v: &Value) -> String {
    v["id"].as_str().expect("id").to_owned()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn recordings_are_transcribed_with_timestamps_and_found_by_search(pool: PgPool) {
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "tones.wav", &tones_wav()).await.json());
    app.run_jobs().await;

    let detail = app
        .send("GET", &format!("/api/v1/files/{fid}"), &ada, None)
        .await
        .json();
    assert_eq!(detail["status"], "ready", "{detail}");

    let e = app
        .send(
            "GET",
            &format!("/api/v1/files/{fid}/extraction"),
            &ada,
            None,
        )
        .await
        .json();
    assert_eq!(e["extractor"], "transcript");
    assert_eq!(e["text"], "tone 440 hertz\ntone 880 hertz");
    assert_eq!(e["duration_ms"], 7000);
    assert_eq!(
        e["segments"],
        json!([
            { "start_ms": 0, "end_ms": 3000, "char_start": 0, "char_end": 14 },
            { "start_ms": 4000, "end_ms": 7000, "char_start": 15, "char_end": 29 },
        ])
    );
    assert!(
        e["notes"][0]
            .as_str()
            .expect("note")
            .contains("transcribed")
    );
    assert_eq!(e["chunk_count"], 1);

    let hits = app
        .send(
            "GET",
            "/api/v1/search/chunks?q=hertz&mode=keyword",
            &ada,
            None,
        )
        .await
        .json();
    let hit = &hits["results"][0];
    assert_eq!(hit["file"]["id"], fid.as_str(), "{hits}");
    assert_eq!(hit["start_ms"], 0);
    assert_eq!(hit["end_ms"], 7000);
    assert_eq!(hit["page"], Value::Null);

    let grouped = app
        .send("GET", "/api/v1/search?q=hertz", &ada, None)
        .await
        .json();
    let first = &grouped["results"][0];
    assert_eq!(first["file"]["id"], fid.as_str(), "{grouped}");
    assert_eq!(first["matches"][0]["start_ms"], 0, "{grouped}");

    // Chat sources carry the time too.
    let conv = conversation(&app, &ada).await;
    let (_, events) = ask(&app, &ada, &conv, json!({ "content": "tone hertz" })).await;
    let sources = find(&events, "sources")["sources"].clone();
    assert_eq!(sources[0]["file_id"], fid.as_str(), "{sources}");
    assert_eq!(sources[0]["start_ms"], 0);
    assert_eq!(sources[0]["end_ms"], 7000);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn undecodable_recordings_fail_and_disabled_transcription_skips(pool: PgPool) {
    let app = TestApp::new(pool.clone());
    let ada = app.user("ada@example.com").await;
    let mut mp3 = b"ID3\x03\x00\x00\x00\x00\x00\x0a".to_vec();
    mp3.extend_from_slice(&[0u8; 64]);
    let broken = id(&app.upload(&ada, "broken.mp3", &mp3).await.json());
    app.run_jobs().await;
    let detail = app
        .send("GET", &format!("/api/v1/files/{broken}"), &ada, None)
        .await
        .json();
    assert_eq!(detail["status"], "failed", "{detail}");
    assert!(detail["error"].as_str().expect("error").contains("audio"));

    // Turned off: stored and playable, no text, nothing downloaded.
    let config = Config {
        transcribe_enabled: false,
        ..test_config()
    };
    let off = TestApp::with_config(pool, config.clone());
    let bob = off.user("bob@example.com").await;
    let fid = id(&off.upload(&bob, "tones.wav", &tones_wav()).await.json());
    off.run_jobs_with(&config).await;
    let e = off
        .send(
            "GET",
            &format!("/api/v1/files/{fid}/extraction"),
            &bob,
            None,
        )
        .await
        .json();
    assert_eq!(e["extractor"], "none");
    assert_eq!(e["segments"], json!([]));
    assert_eq!(e["duration_ms"], Value::Null);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn a_missing_speech_model_is_retried_not_fatal(pool: PgPool) {
    let models = tempfile::tempdir().expect("tempdir");
    let config = Config {
        whisper_model: "base".into(),
        ml_models_url: String::new(), // no downloads
        models_dir: models.path().to_string_lossy().into_owned(),
        ..test_config()
    };
    let app = TestApp::with_config(pool.clone(), config.clone());
    let ada = app.user("ada@example.com").await;
    let fid = id(&app.upload(&ada, "tones.wav", &tones_wav()).await.json());
    app.run_jobs_with(&config).await;
    let detail = app
        .send("GET", &format!("/api/v1/files/{fid}"), &ada, None)
        .await
        .json();
    // Still processing: the job waits for a retry (the model may appear).
    assert_eq!(detail["status"], "processing", "{detail}");
    assert_eq!(detail["processing"]["state"], "failed", "{detail}");

    let status = app
        .send("GET", "/api/v1/system/status", &ada, None)
        .await
        .json();
    assert_eq!(status["transcription"]["status"], "unavailable", "{status}");
}
