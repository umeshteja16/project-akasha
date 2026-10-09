//! Chunk sizes, overlap, offsets and page boundaries.

mod support;

use akasha_ingest::{ChunkOptions, Extraction, Options, chunk, extract};
use support::{PageSpec, pdf};

fn chars(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end - start).collect()
}

fn plain(text: &str) -> Extraction {
    extract(text.as_bytes(), "text/plain", &Options::default()).expect("text")
}

fn sentences(n: usize) -> String {
    (0..n)
        .map(|i| format!("Sentence number {i} says something mildly interesting about ünïcödé."))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn chunk_offsets_point_at_the_chunk_text() {
    let e = plain(&sentences(200));
    let chunks = chunk(&e, &ChunkOptions::default());
    assert!(chunks.len() > 3);
    for (i, c) in chunks.iter().enumerate() {
        assert_eq!(c.index as usize, i);
        assert_eq!(c.page, None);
        assert_eq!(chars(&e.text, c.char_start, c.char_end), c.text);
        assert!(c.text.chars().count() <= 2000, "chunk {i} too long");
    }
    assert_eq!(chunks[0].char_start, 0);
    assert_eq!(chunks.last().map(|c| c.char_end), Some(e.char_count()));
}

#[test]
fn neighbouring_chunks_overlap_without_gaps() {
    let e = plain(&sentences(200));
    let options = ChunkOptions {
        max_chars: 800,
        overlap_chars: 100,
    };
    let chunks = chunk(&e, &options);
    for pair in chunks.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(b.char_start > a.char_start, "chunks advance");
        assert!(b.char_start <= a.char_end, "no gap between chunks");
        let overlap = a.char_end - b.char_start;
        assert!(
            (1..=100).contains(&overlap),
            "overlap {overlap} within the configured 100"
        );
        assert!(a.text.chars().count() >= 600 - 100, "chunks are not tiny");
    }
}

#[test]
fn short_text_is_one_chunk_and_empty_text_none() {
    let e = plain("Just a note.");
    let chunks = chunk(&e, &ChunkOptions::default());
    assert_eq!(chunks.len(), 1);
    assert_eq!(
        (
            chunks[0].char_start,
            chunks[0].char_end,
            chunks[0].text.as_str()
        ),
        (0, 12, "Just a note.")
    );
    assert!(chunk(&plain("  \n "), &ChunkOptions::default()).is_empty());
}

#[test]
fn chunks_never_span_pages_and_carry_page_numbers() {
    let long: Vec<String> = (0..40)
        .map(|i| format!("Line {i} of page two with filler words."))
        .collect();
    let long: Vec<&str> = long.iter().map(String::as_str).collect();
    let bytes = pdf(&[
        PageSpec::Text(&["Short first page."]),
        PageSpec::Blank,
        PageSpec::Text(&long),
    ]);
    let e = extract(&bytes, "application/pdf", &Options::default()).expect("pdf");
    let options = ChunkOptions {
        max_chars: 400,
        overlap_chars: 50,
    };
    let chunks = chunk(&e, &options);
    assert_eq!(chunks[0].page, Some(1));
    assert_eq!(chunks[0].text, "Short first page.");
    assert!(
        chunks.iter().all(|c| c.page != Some(2)),
        "blank page has no chunks"
    );
    let page3 = &e.pages[2];
    for c in chunks.iter().skip(1) {
        assert_eq!(c.page, Some(3));
        assert!(c.char_start >= page3.char_start && c.char_end <= page3.char_end);
        assert_eq!(chars(&e.text, c.char_start, c.char_end), c.text);
    }
    assert!(chunks.len() > 3);
}

#[test]
fn chunking_is_deterministic() {
    let e = plain(&sentences(100));
    assert_eq!(
        chunk(&e, &ChunkOptions::default()),
        chunk(&e, &ChunkOptions::default())
    );
}
