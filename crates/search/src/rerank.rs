//! Cross-encoder reranking of the top fused results.

use std::sync::Arc;

use crate::{Models, RERANK_TOP, engine::Ranked, fusion, types::SearchMeta};

/// Characters of each chunk the reranker reads.
const RERANK_CHARS: usize = 2000;

/// Reorder the top [`RERANK_TOP`] by the reranker's scores; leave the order
/// alone (with a warning) if the reranker is unavailable or fails.
pub(crate) async fn rerank(
    query: &str,
    models: &Models,
    ranked: &mut Vec<Ranked>,
    meta: &mut SearchMeta,
) {
    let reranker = match &models.reranker {
        Ok(Some(r)) => Arc::clone(r),
        Ok(None) => return,
        Err(reason) => {
            meta.warnings
                .push(format!("reranking is unavailable ({reason})"));
            return;
        }
    };
    let n = ranked.len().min(RERANK_TOP);
    if n < 2 {
        return;
    }
    let texts: Vec<String> = ranked[..n]
        .iter()
        .map(|r| r.row.text.chars().take(RERANK_CHARS).collect())
        .collect();
    let q = query.to_owned();
    let scored = tokio::task::spawn_blocking(move || {
        let docs: Vec<&str> = texts.iter().map(String::as_str).collect();
        reranker.score(&q, &docs)
    })
    .await;
    let scores = match scored {
        Ok(Ok(s)) if s.len() == n => s,
        Ok(Ok(_)) => {
            return meta
                .warnings
                .push("reranker returned a wrong number of scores".into());
        }
        Ok(Err(err)) => return meta.warnings.push(format!("reranking failed ({err})")),
        Err(err) => return meta.warnings.push(format!("reranking panicked ({err})")),
    };
    let mut head: Vec<Option<Ranked>> = ranked.drain(..n).map(Some).collect();
    let reordered: Vec<Ranked> = fusion::rerank_order(&scores)
        .into_iter()
        .filter_map(|i| {
            let mut r = head[i].take()?;
            r.scores.rerank = Some(scores[i]);
            Some(r)
        })
        .collect();
    ranked.splice(0..0, reordered);
    meta.reranked = true;
}
