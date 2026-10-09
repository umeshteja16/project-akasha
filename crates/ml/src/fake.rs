//! Deterministic stand-ins for the real models: no files, no ONNX Runtime, same
//! output for the same input on every machine. Tests use them to exercise the
//! whole pipeline; texts sharing words get similar vectors, which is enough to
//! check ranking plumbing (not quality).

use crate::{Embedder, MlError, Reranker, catalog::EmbedModel, normalize};

/// Hashed bag of words: each lowercase word adds ±1 to one dimension.
pub struct HashEmbedder {
    model: &'static EmbedModel,
}

impl HashEmbedder {
    pub fn new(model: &'static EmbedModel) -> Self {
        Self { model }
    }

    fn vector(&self, text: &str) -> Vec<f32> {
        let dim = self.model.dim;
        let mut v = vec![0.0f32; dim];
        for word in words(text) {
            let h = fnv1a(word.as_bytes());
            // Reduce in u64 first, so the cast cannot truncate.
            let i = usize::try_from(h % dim as u64).unwrap_or(0);
            v[i] += if h >> 63 == 0 { 1.0 } else { -1.0 };
        }
        if v.iter().all(|x| *x == 0.0) {
            // Cosine distance is undefined for a zero vector.
            v[0] = 1.0;
        }
        normalize(&mut v);
        v
    }
}

impl Embedder for HashEmbedder {
    fn model(&self) -> &'static EmbedModel {
        self.model
    }

    fn embed_documents(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, MlError> {
        Ok(texts.iter().map(|t| self.vector(t)).collect())
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, MlError> {
        Ok(self.vector(text))
    }
}

/// Scores a document by the share of query words it contains.
pub struct OverlapReranker;

impl Reranker for OverlapReranker {
    fn name(&self) -> &'static str {
        crate::catalog::OVERLAP_RERANK_MODEL
    }

    fn score(&self, query: &str, documents: &[&str]) -> Result<Vec<f32>, MlError> {
        let query: Vec<String> = words(query).collect();
        Ok(documents
            .iter()
            .map(|doc| {
                if query.is_empty() {
                    return 0.0;
                }
                let doc: Vec<String> = words(doc).collect();
                let hits = query.iter().filter(|w| doc.contains(w)).count();
                hits as f32 / query.len() as f32
            })
            .collect())
    }
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{HASH_EMBED_MODEL, embed_model};

    fn dot(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    #[test]
    fn hash_vectors_are_deterministic_normalised_and_word_sensitive() {
        let e = HashEmbedder::new(embed_model(HASH_EMBED_MODEL).expect("model"));
        let v = e
            .embed_documents(&["The aardvark digs", "the AARDVARK digs!", "quantum chess"])
            .expect("embed");
        assert_eq!(v[0].len(), 384);
        assert_eq!(v[0], v[1], "case and punctuation are ignored");
        assert!((dot(&v[0], &v[0]) - 1.0).abs() < 1e-5);
        let q = e.embed_query("aardvark").expect("query");
        assert!(dot(&q, &v[0]) > dot(&q, &v[2]));
        let empty = e.embed_query("").expect("empty");
        assert!((dot(&empty, &empty) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn overlap_scores_in_input_order() {
        let r = OverlapReranker;
        let s = r
            .score(
                "night aardvark",
                &["cats sleep", "the aardvark digs at night", "aardvark"],
            )
            .expect("score");
        assert_eq!(s, vec![0.0, 1.0, 0.5]);
    }
}
