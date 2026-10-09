//! Spelling suggestions from the owner's vocabulary (`user_terms`, maintained by
//! triggers on `file_chunks`, migration 0008). Every lookup is filtered by owner:
//! a user is never offered a word from someone else's documents.

use sqlx::PgPool;
use uuid::Uuid;

/// The closest known word for each of `words` (same order): the word itself when
/// the owner's documents contain it or a word with the same English stem (which
/// keyword search already matches: "pangolin" finds "pangolins"), else the most similar word (pg_trgm
/// similarity at least `min_similarity`, ties broken by frequency), else `None`.
pub async fn closest_terms(
    pool: &PgPool,
    owner_id: Uuid,
    words: &[String],
    min_similarity: f32,
) -> Result<Vec<Option<String>>, sqlx::Error> {
    if words.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(
        r#"SELECT best.term
           FROM unnest($2::text[]) WITH ORDINALITY AS w(word, i)
           LEFT JOIN LATERAL (
               SELECT CASE WHEN k.known THEN w.word ELSE t.term END AS term
               FROM user_terms t
               CROSS JOIN LATERAL (SELECT t.term = w.word
                   OR to_tsvector('english', t.term) @@ plainto_tsquery('english', w.word)
                   AS known) k
               WHERE t.owner_id = $1 AND t.term % w.word
                 AND similarity(t.term, w.word) >= $3
               ORDER BY k.known DESC, similarity(t.term, w.word) DESC,
                        t.chunk_count DESC, t.term
               LIMIT 1
           ) best ON true
           ORDER BY w.i"#,
        owner_id,
        words,
        min_similarity,
    )
    .fetch_all(pool)
    .await
}
