//! "Similar files": the owner's files whose chunks lie closest to a file's mean
//! embedding. Uses only stored vectors, so it works without loading a model.

use std::collections::HashMap;

use akasha_db::{
    PgPool,
    search::{self, ChunkFilter},
};
use uuid::Uuid;

use crate::{EF_SEARCH, SearchError, types::SimilarFile};

/// Largest `limit` for [`similar_files`].
pub const MAX_SIMILAR: usize = 20;
/// Nearest chunks examined per requested file.
const CHUNKS_PER_RESULT: usize = 10;

/// Up to `limit` of the owner's other files, most similar first. `None` if the
/// file does not exist (or is not the owner's); empty if it has no vectors yet.
pub async fn similar_files(
    pool: &PgPool,
    owner_id: Uuid,
    file_id: Uuid,
    limit: usize,
) -> Result<Option<Vec<SimilarFile>>, SearchError> {
    if !(1..=MAX_SIMILAR).contains(&limit) {
        return Err(SearchError::InvalidRequest(format!(
            "limit must be 1-{MAX_SIMILAR}"
        )));
    }
    let Some(mean) = search::mean_embedding(pool, owner_id, file_id).await? else {
        let exists = !search::files_by_ids(pool, owner_id, &[file_id])
            .await?
            .is_empty();
        return Ok(exists.then(Vec::new));
    };
    let filter = ChunkFilter {
        exclude_file: Some(file_id),
        ..ChunkFilter::default()
    };
    let pool_size = i64::try_from(limit * CHUNKS_PER_RESULT).unwrap_or(200);
    let nearest = search::semantic(pool, owner_id, &mean, &filter, pool_size, EF_SEARCH).await?;

    // Best (first) chunk per file, in order.
    let mut best: Vec<(Uuid, f32)> = Vec::new();
    for c in nearest {
        if !best.iter().any(|(id, _)| *id == c.file_id) {
            best.push((c.file_id, c.score));
        }
    }
    best.truncate(limit);
    let ids: Vec<Uuid> = best.iter().map(|(id, _)| *id).collect();
    let mut files: HashMap<Uuid, _> = search::files_by_ids(pool, owner_id, &ids)
        .await?
        .into_iter()
        .map(|f| (f.id, f))
        .collect();
    Ok(Some(
        best.into_iter()
            .filter_map(|(id, similarity)| {
                files.remove(&id).map(|f| SimilarFile {
                    file: f.into(),
                    similarity,
                })
            })
            .collect(),
    ))
}
