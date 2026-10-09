//! "Did you mean": when a keyword search finds little, replace each query word
//! the owner's documents do not contain with the most similar word they do
//! contain (pg_trgm over the per-owner vocabulary, [`akasha_db::search::closest_terms`]).

use akasha_db::{PgPool, search};
use uuid::Uuid;

/// Suggest only when the keyword retriever found fewer chunks than this.
pub const SUGGEST_BELOW: usize = 3;
/// Least trigram similarity for a replacement (legacy used 0.4 too).
pub const MIN_SIMILARITY: f32 = 0.4;
/// Words checked per query (the rest are left as typed).
const MAX_WORDS: usize = 12;

/// A word of the query: `[start, end)` byte range and its lowercase form.
struct Word {
    start: usize,
    end: usize,
    lower: String,
}

/// Words the vocabulary could contain: 3-32 letters (as `akasha_terms` keeps).
fn words(query: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start = None;
    let bounds = query
        .char_indices()
        .map(|(i, c)| (i, Some(c)))
        .chain(std::iter::once((query.len(), None)));
    for (i, c) in bounds {
        match (c.is_some_and(char::is_alphanumeric), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                let word = &query[s..i];
                let len = word.chars().count();
                if (3..=32).contains(&len) && word.chars().all(char::is_alphabetic) {
                    out.push(Word {
                        start: s,
                        end: i,
                        lower: word.to_lowercase(),
                    });
                }
                start = None;
            }
            _ => {}
        }
    }
    out.truncate(MAX_WORDS);
    out
}

/// The corrected query, or `None` when every word is known (or nothing close).
pub async fn suggest(
    pool: &PgPool,
    owner_id: Uuid,
    query: &str,
) -> Result<Option<String>, sqlx::Error> {
    let words = words(query);
    let lower: Vec<String> = words.iter().map(|w| w.lower.clone()).collect();
    let closest = search::closest_terms(pool, owner_id, &lower, MIN_SIMILARITY).await?;
    Ok(rewrite(query, &words, &closest))
}

fn rewrite(query: &str, words: &[Word], closest: &[Option<String>]) -> Option<String> {
    let mut out = String::with_capacity(query.len());
    let mut last = 0;
    let mut changed = false;
    for (word, best) in words.iter().zip(closest) {
        if let Some(best) = best.as_deref().filter(|b| *b != word.lower) {
            out.push_str(&query[last..word.start]);
            out.push_str(best);
            last = word.end;
            changed = true;
        }
    }
    out.push_str(&query[last..]);
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(q: &str) -> Vec<String> {
        words(q).into_iter().map(|w| w.lower).collect()
    }

    #[test]
    fn words_are_alphabetic_and_three_to_32_letters() {
        assert_eq!(
            lower("Ardvark in the \"burow\" -x 2024 abc1 Résumé"),
            ["ardvark", "the", "burow", "résumé"]
        );
        assert!(lower("a b c 12").is_empty());
    }

    #[test]
    fn rewrite_keeps_everything_but_the_replaced_words() {
        let q = "\"Ardvark burow\" -zebra";
        let w = words(q);
        let closest = vec![Some("aardvark".into()), Some("burrow".into()), None];
        assert_eq!(
            rewrite(q, &w, &closest).as_deref(),
            Some("\"aardvark burrow\" -zebra")
        );
        // Known words (case aside) are not suggestions.
        let closest = vec![Some("ardvark".into()), Some("burow".into()), None];
        assert_eq!(rewrite(q, &w, &closest), None);
    }
}
