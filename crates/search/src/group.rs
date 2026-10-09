//! Grouping ranked chunks by file for the search page.

use std::collections::HashMap;

use uuid::Uuid;

use crate::{engine::Ranked, types::FileHit};

/// Chunks shown per file.
pub const MATCHES_PER_FILE: usize = 3;

/// One entry per file in order of its best chunk; each keeps its best
/// [`MATCHES_PER_FILE`] chunks and counts the rest.
pub(crate) fn by_file(ranked: &[Ranked]) -> Vec<FileHit> {
    let mut files: Vec<FileHit> = Vec::new();
    let mut index: HashMap<Uuid, usize> = HashMap::new();
    for r in ranked {
        let slot = *index.entry(r.row.file_id).or_insert_with(|| {
            files.push(FileHit {
                file: r.file(),
                score: r.scores.fused,
                match_count: 0,
                matches: Vec::new(),
            });
            files.len() - 1
        });
        let hit = &mut files[slot];
        hit.match_count = hit.match_count.saturating_add(1);
        if hit.matches.len() < MATCHES_PER_FILE {
            hit.matches.push(r.to_match());
        }
    }
    files
}
