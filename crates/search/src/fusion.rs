//! Reciprocal Rank Fusion and rerank ordering (pure, no I/O).

use std::collections::HashMap;

use akasha_db::search::Candidate;
use uuid::Uuid;

use crate::types::Scores;

/// Which retriever produced a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Keyword,
    Semantic,
    Filename,
}

/// A chunk after fusion.
#[derive(Debug, Clone, PartialEq)]
pub struct Fused {
    pub chunk_id: i64,
    pub file_id: Uuid,
    pub scores: Scores,
}

/// Fuse ranked lists: each chunk scores `Σ 1 / (k + rank)` over the lists it
/// appears in (rank is 1-based). Best first; ties go to the lower chunk id, so
/// the order is deterministic.
pub fn rrf(lists: &[(Source, &[Candidate])], k: f64) -> Vec<Fused> {
    let mut fused: Vec<Fused> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    for (source, list) in lists {
        for (i, c) in list.iter().enumerate() {
            let rank = u32::try_from(i + 1).unwrap_or(u32::MAX);
            let slot = *index.entry(c.chunk_id).or_insert_with(|| {
                fused.push(Fused {
                    chunk_id: c.chunk_id,
                    file_id: c.file_id,
                    scores: Scores::default(),
                });
                fused.len() - 1
            });
            let s = &mut fused[slot].scores;
            // A chunk listed twice by one retriever counts once (its best rank).
            let fresh = match source {
                Source::Keyword => set_once(&mut s.keyword_rank, rank, &mut s.keyword, c.score),
                Source::Semantic => set_once(&mut s.semantic_rank, rank, &mut s.semantic, c.score),
                Source::Filename => {
                    let fresh = s.filename_rank.is_none();
                    if fresh {
                        s.filename_rank = Some(rank);
                    }
                    fresh
                }
            };
            if fresh {
                s.fused += 1.0 / (k + f64::from(rank));
            }
        }
    }
    fused.sort_by(|a, b| {
        b.scores
            .fused
            .total_cmp(&a.scores.fused)
            .then(a.chunk_id.cmp(&b.chunk_id))
    });
    fused
}

fn set_once(rank: &mut Option<u32>, r: u32, score: &mut Option<f32>, s: f32) -> bool {
    if rank.is_some() {
        return false;
    }
    *rank = Some(r);
    *score = Some(s);
    true
}

/// Indices of `scores` from best to worst; NaN sorts last, ties keep input order.
pub fn rerank_order(scores: &[f32]) -> Vec<usize> {
    let key = |i: usize| {
        let s = scores[i];
        if s.is_nan() { f32::NEG_INFINITY } else { s }
    };
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&a, &b| key(b).total_cmp(&key(a)));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(chunk_id: i64, score: f32) -> Candidate {
        Candidate {
            chunk_id,
            file_id: Uuid::nil(),
            score,
        }
    }

    #[test]
    fn chunks_found_by_both_retrievers_win() {
        let keyword = [c(1, 0.9), c(2, 0.8), c(3, 0.7)];
        let semantic = [c(4, 0.99), c(3, 0.95), c(1, 0.5)];
        let fused = rrf(
            &[(Source::Keyword, &keyword), (Source::Semantic, &semantic)],
            60.0,
        );
        let ids: Vec<i64> = fused.iter().map(|f| f.chunk_id).collect();
        // 1: 1/61 + 1/63, 3: 1/63 + 1/62, 4: 1/61, 2: 1/62
        assert_eq!(ids, vec![1, 3, 4, 2]);
        let top = &fused[0].scores;
        assert_eq!(top.keyword_rank, Some(1));
        assert_eq!(top.semantic_rank, Some(3));
        assert_eq!(top.semantic, Some(0.5));
        assert!((top.fused - (1.0 / 61.0 + 1.0 / 63.0)).abs() < 1e-12);
    }

    #[test]
    fn ties_break_on_chunk_id_and_duplicates_count_once() {
        let keyword = [c(9, 0.5), c(9, 0.4)];
        let semantic = [c(5, 0.5)];
        let fused = rrf(
            &[(Source::Keyword, &keyword), (Source::Semantic, &semantic)],
            60.0,
        );
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].chunk_id, 5, "equal scores: lower id first");
        assert!((fused[1].scores.fused - 1.0 / 61.0).abs() < 1e-12);
        assert_eq!(fused[1].scores.keyword, Some(0.5));
    }

    #[test]
    fn file_name_list_adds_its_own_rank() {
        let keyword = [c(1, 0.2)];
        let names = [c(2, 0.9), c(1, 0.5)];
        let fused = rrf(
            &[(Source::Keyword, &keyword), (Source::Filename, &names)],
            60.0,
        );
        assert_eq!(fused[0].chunk_id, 1);
        assert_eq!(fused[0].scores.filename_rank, Some(2));
        assert_eq!(fused[1].scores.filename_rank, Some(1));
        assert_eq!(fused[1].scores.keyword_rank, None);
    }

    #[test]
    fn empty_lists_fuse_to_nothing() {
        assert!(rrf(&[(Source::Keyword, &[])], 60.0).is_empty());
    }

    #[test]
    fn rerank_order_is_descending_stable_and_nan_last() {
        assert_eq!(
            rerank_order(&[0.1, f32::NAN, 0.9, 0.1, 0.5]),
            vec![2, 4, 0, 3, 1]
        );
        assert!(rerank_order(&[]).is_empty());
    }
}
