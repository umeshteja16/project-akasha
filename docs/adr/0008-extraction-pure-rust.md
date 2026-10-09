# 0008: Text extraction in pure Rust (pdf-extract, ocrs, text-splitter)

- Status: accepted · 2026-10-09

## Context
Step 2.5 needs text out of uploads (plain text, Markdown, CSV, JSON, PDF, images) with
page numbers for PDF citations, plus chunks for search. The plan named `pdfium-render`.
Builds must stay simple on dev machines, CI and the distroless Docker image, and one bad
file must never take down a worker.

## Decision
- **New crate `crates/ingest` (`akasha-ingest`)**: a blocking library, bytes + MIME in,
  `Extraction` (normalised text, page spans, notes) and `Chunk`s out. No database, no
  HTTP server. The app runs it on `spawn_blocking` inside the `extract_file` job.
- **PDF: `pdf-extract` 0.12 on `lopdf` 0.42**, called page by page
  (`output_doc_page`), each page under `catch_unwind` because pdf-extract still
  `unwrap`s on some malformed input. Load errors and "every page failed" are permanent
  `Corrupt` errors; password-protected files are a permanent `Encrypted` error (an empty
  user password is tried first). Alternatives:
  - `pdfium-render`: best fidelity, but needs the pdfium shared library at runtime on
    every machine and in the image (download per platform, glibc coupling). Rejected for
    now; it stays the upgrade path if text quality becomes the bottleneck, behind the
    same `extract` function.
  - `pdf_oxide` (0.3.x): promising and fast, but young and changing quickly. Revisit.
- **Scanned pages**: a page with no text but with image XObjects is `needs_ocr`. With
  OCR on, its largest image is recognised if it is JPEG (`DCTDecode`) or an 8-bit
  gray/RGB bitmap (raw or Flate); JBIG2/CCITT/JPEG 2000 stay `needs_ocr`. No page
  rasteriser (that would need pdfium).
- **OCR: `ocrs` 0.13 (rten 0.26)**, pure Rust. Models (`text-detection.rten`,
  `text-recognition.rten`, ~12 MB) are downloaded on first use from the ocrs-models
  bucket into `AKASHA_MODELS_DIR` (`/var/lib/akasha/models` volume in Docker), verified
  against pinned SHA-256 digests on every load, written atomically. `AKASHA_OCR_ENABLED`
  turns OCR off; `AKASHA_OCR_MODELS_URL=` (empty) disables downloads for offline
  installs (copy the files in). Model trouble is retryable, never permanent. Tests that
  need models are `#[ignore]`d.
- **Text formats**: lossy UTF-8 decode, NFC, `\n` line ends, control/zero-width chars
  dropped, whitespace collapsed. Markdown goes through `pulldown-cmark` and keeps
  heading, paragraph, list, table and code text but no syntax, raw HTML or front matter.
  CSV and JSON are indexed as written.
- **Audio/video**: status `ready`, extractor `none`, a note that transcription is not
  available yet (step 6). The file stays downloadable; it is not an error.
- **Chunking: `text-splitter`** by characters: up to 2000 (aims for 1500+), overlap
  250 (~512 tokens, ~12%). Character sizing avoids pulling a tokenizer before the
  embedding model is chosen (step 2.6 may switch to a token sizer). Chunks never cross
  a PDF page; offsets are Unicode-character offsets into the stored full text.
- **Storage**: `file_extractions` (one row per file: full text, page spans as JSONB,
  notes, extractor + version) and `file_chunks` (owner denormalised, page, char span,
  text, generated `tsvector` with the `english` config like the legacy index, GIN
  index). The `embedding` column is added by the embeddings step with its dimension.
  Both tables cascade from `files` and are replaced in one transaction per run.

## Consequences
- No system libraries: `cargo build` and the distroless image work unchanged.
- Text quality of pdf-extract is below pdfium on complex layouts (columns, tables).
- `ttf-parser` (via lopdf) is flagged unmaintained (RUSTSEC-2026-0192); ignored in
  `deny.toml` with a note to revisit.
- OCR is CPU-heavy (~0.5 s for a small image in an optimised build); it runs on the
  blocking pool, so `AKASHA_WORKER_CONCURRENCY` bounds it.
