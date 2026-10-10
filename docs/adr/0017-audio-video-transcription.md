# 0017: Audio and video transcription

- Status: accepted · 2026-10-10

## Context
Recordings (voice notes, meetings, lectures, screen recordings) were stored and playable
but had no text, so search and chat could not see them. The roadmap item asked for
on-device speech to text with `whisper-rs`, timestamps that search results and citations
can point at, and no system ffmpeg.

## Decision
- **Speech model: whisper.cpp through `whisper-rs` 0.16** (new crate `crates/media`,
  feature `whisper`, on by default in the binary). It is the fastest CPU option, gives
  segment timestamps directly and runs the multilingual GGML models (`tiny` … `large-v3-
  turbo`; default `base`). A pure-Rust `candle` Whisper was the alternative: no C++
  toolchain, but slower on CPU, more code to own (mel spectrogram, decoding loop,
  timestamp tokens) and no ready-made timestamps. whisper.cpp builds from the vendored
  source with cmake (CI runners have it; the Docker build stage installs it). Docker
  builds use the bindings shipped with whisper-rs-sys (`WHISPER_DONT_GENERATE_BINDINGS`,
  no libclang) and portable CPU flags (`GGML_NATIVE=OFF`, x86-64 with AVX2/FMA/F16C) so
  the image does not crash on a different CPU. OpenMP stays off (no `libgomp` in
  distroless). `whisper-rs`/`whisper-rs-sys` are Unlicense; `deny.toml` allows that for
  these two crates only.
- **Models**: `AKASHA_WHISPER_MODEL`, files from the `ggerganov/whisper.cpp` Hugging Face
  repository (through `AKASHA_ML_MODELS_URL`, so mirrors and air-gapped installs work like
  the other models), SHA-256 pinned in `crates/media/src/models.rs`, streamed to disk
  while hashing and renamed into `<models>/whisper/` only when verified. `akasha models
  download` and `models check` include it; `fake` selects a deterministic test model.
- **Decoding is pure Rust**: Symphonia 0.5 demuxes WAV, MP3, FLAC, Ogg, MP4/M4A/MOV and
  WebM/Matroska and decodes AAC, ALAC, MP3, FLAC, Vorbis and PCM; Opus (Ogg and WebM, e.g.
  phone voice notes) goes through `opus-decoder` (pure Rust, RFC 8251 conformant), which
  decodes straight to 16 kHz mono. Everything else is mixed to mono and resampled to
  16 kHz with our own streaming windowed-sinc filter (no extra dependency). Video tracks
  are ignored. Not supported: AC-3/E-AC-3/DTS audio, Opus with more than two channels,
  and containers Symphonia cannot read (e.g. AVI, WMA); such files end `failed` with a
  clear message.
- **Pipeline**: `extract_file` transcribes `audio/*` and `video/*` when
  `AKASHA_TRANSCRIBE_ENABLED` (default on; off keeps the old "ready, no text"). The blob is
  streamed to a temporary file, decoded and fed to the model in 10-minute windows cut at
  the quietest moment near the window end (bounded memory, progress between windows,
  silence never reaches the model, which tends to invent text for it). Only the first
  `AKASHA_TRANSCRIBE_MAX_MINUTES` (120) are transcribed, with a note. One transcription at
  a time per process (a semaphore), each with `AKASHA_TRANSCRIBE_THREADS` (0 = cores, at
  most 8); other jobs keep running. Progress goes to the new generic `jobs.progress`
  (`akasha_jobs::report_progress`) and is shown on the file page. A dropped job (worker
  shutdown) cancels the model through whisper.cpp's abort callback.
- **Data**: the transcript is one line per segment in `file_extractions.text`
  (extractor `transcript`), with `segments` (`start_ms`, `end_ms`, character span) and
  `duration_ms` next to it; `file_chunks.start_ms/end_ms` (nullable, like `page`) say when
  a chunk's speech starts and ends (migration 0015). Transcript chunks are smaller (1000
  characters, about a minute of speech) so a hit lands near the moment. Search results,
  chat citations (and the prompt: "at 12:34") and MCP passages (`at`) carry the time.
- **UI**: recordings get a player with the transcript beside it; every line's timestamp
  seeks the player, the line being spoken is highlighted, and links from search and
  citations add `?t=<seconds>` (plus `at=` to mark the passage). The view is a lazily
  loaded chunk.

## Consequences
- The binary needs cmake and a C++ compiler to build with default features
  (`--no-default-features --features onnx` builds without Whisper; the `fake` model still
  works). First build is ~2 minutes longer.
- Transcription is CPU heavy: `base` runs several times faster than real time on 4
  modern cores, `small` about 3× slower than `base`. Long recordings take a while; the
  file stays `processing` with visible progress.
- The fake model ("tone N hertz" per second of sine wave) keeps tests deterministic and
  exercises real decoding/resampling; the real model is tested by an ignored test and by
  the Docker smoke test (`models check` with `tiny`).
- `opus-decoder` 0.1 is young; its debug overflow checks are disabled in the dev profile
  (one shift that release builds wrap, harmless).
