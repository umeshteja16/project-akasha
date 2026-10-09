//! Keyword, semantic and hybrid retrieval, reranking, degradation, snippets,
//! grouping and paging, against a real database with the hash embedder.

mod support;

use akasha_search::{ChunkHit, Models, SearchError, SearchMode, search_chunks, search_files};
use sqlx::PgPool;
use support::{file, models, models_with_rerank, request, user};

const TXT: &str = "text/plain";

fn highlighted(hit: &ChunkHit) -> Vec<String> {
    let chars: Vec<char> = hit.chunk.snippet.text.chars().collect();
    hit.chunk
        .snippet
        .highlights
        .iter()
        .map(|h| chars[h.start as usize..h.end as usize].iter().collect())
        .collect()
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn keyword_search_matches_stems_and_reports_snippets(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let f = file(
        &pool,
        ada,
        "notes.txt",
        TXT,
        &["Unrelated opening words. The aardvark was digging burrows all night long."],
    )
    .await;
    file(&pool, ada, "other.txt", TXT, &["Quantum chess openings."]).await;

    let res = search_chunks(
        &pool,
        ada,
        &request("aardvarks dig", SearchMode::Keyword),
        &models(),
    )
    .await
    .expect("search");
    assert_eq!(res.meta.mode, SearchMode::Keyword);
    assert!(!res.meta.degraded);
    assert_eq!(
        res.results.len(),
        1,
        "AND semantics: only the aardvark chunk"
    );
    let hit = &res.results[0];
    assert_eq!(hit.file.id, f);
    assert_eq!(hit.chunk.scores.keyword_rank, Some(1));
    assert!(hit.chunk.scores.keyword.expect("score") > 0.0);
    assert_eq!(hit.chunk.scores.semantic, None, "keyword mode never embeds");
    assert_eq!(highlighted(hit), vec!["aardvark", "digging"]);
    assert!(hit.text.starts_with("Unrelated opening"));
    assert_eq!((hit.chunk.char_start, hit.chunk.page), (0, None));
    assert_eq!(res.meta.timings.embed_ms, 0.0);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn semantic_search_ranks_by_similarity_without_word_matches(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    file(&pool, ada, "a.txt", TXT, &["aardvark burrow"]).await;
    file(&pool, ada, "b.txt", TXT, &["quantum chess opening theory"]).await;

    // By default the unrelated nearest neighbour stays below the relevance floor.
    let res = search_chunks(
        &pool,
        ada,
        &request("burrow", SearchMode::Semantic),
        &models(),
    )
    .await
    .expect("search");
    assert_eq!(res.results.len(), 1);
    assert_eq!(res.meta.loosely_related, 1);
    assert!(!res.results[0].chunk.loosely_related);

    // "aardvarks" does not stem-match for the hash embedder, "burrow" does.
    let mut req = request("burrow", SearchMode::Semantic);
    req.include_weak = true;
    let res = search_chunks(&pool, ada, &req, &models())
        .await
        .expect("search");
    assert_eq!(res.meta.mode, SearchMode::Semantic);
    assert_eq!(res.results.len(), 2, "semantic returns nearest neighbours");
    assert_eq!(res.meta.loosely_related, 0);
    assert!(res.results[1].chunk.loosely_related);

    // A configured floor overrides the model's default.
    let lenient = Models {
        floor: akasha_search::RelevanceFloor {
            min_similarity: Some(-1.0),
            min_rerank_score: None,
        },
        ..models()
    };
    let res = search_chunks(
        &pool,
        ada,
        &request("burrow", SearchMode::Semantic),
        &lenient,
    )
    .await
    .expect("search");
    assert_eq!(res.results.len(), 2);
    assert_eq!(res.results[0].text, "aardvark burrow");
    let top = &res.results[0].chunk.scores;
    assert_eq!(top.keyword_rank, None);
    assert_eq!(top.semantic_rank, Some(1));
    assert!(top.semantic.expect("similarity") > res.results[1].chunk.scores.semantic.expect("s"));
    assert!(res.meta.timings.semantic_ms > 0.0);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn hybrid_puts_chunks_found_by_both_retrievers_first(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    file(&pool, ada, "a.txt", TXT, &["aardvark alone in the text"]).await;
    file(&pool, ada, "b.txt", TXT, &["the aardvark builds a burrow"]).await;
    file(&pool, ada, "c.txt", TXT, &["quantum chess"]).await;

    let res = search_chunks(
        &pool,
        ada,
        &request("aardvark burrow", SearchMode::Hybrid),
        &models(),
    )
    .await
    .expect("search");
    assert_eq!(res.meta.mode, SearchMode::Hybrid);
    let top = &res.results[0];
    assert_eq!(top.text, "the aardvark builds a burrow");
    assert_eq!(top.chunk.scores.keyword_rank, Some(1));
    assert_eq!(top.chunk.scores.semantic_rank, Some(1));
    let expected = 2.0 / 61.0;
    assert!((top.chunk.scores.fused - expected).abs() < 1e-9);
    // The semantic-only match follows; fused scores never increase down the list.
    assert_eq!(res.results[1].text, "aardvark alone in the text");
    assert_eq!(res.results[1].chunk.scores.keyword_rank, None);
    let fused: Vec<f64> = res.results.iter().map(|r| r.chunk.scores.fused).collect();
    assert!(fused.windows(2).all(|w| w[0] >= w[1]), "{fused:?}");
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn file_names_match_keyword_queries(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let f = file(
        &pool,
        ada,
        "q3_budget-final.txt",
        TXT,
        &["numbers and more numbers"],
    )
    .await;
    let res = search_chunks(
        &pool,
        ada,
        &request("budget", SearchMode::Keyword),
        &models(),
    )
    .await
    .expect("search");
    assert_eq!(res.results.len(), 1);
    assert_eq!(res.results[0].file.id, f);
    assert_eq!(res.results[0].chunk.scores.filename_rank, Some(1));
    assert_eq!(res.results[0].chunk.scores.keyword_rank, None);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn unavailable_embedder_degrades_to_keyword(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    file(&pool, ada, "a.txt", TXT, &["the aardvark digs"]).await;
    let models = Models {
        embedder: Err("ONNX Runtime is not installed".into()),
        reranker: Ok(None),
        floor: Default::default(),
    };
    for mode in [SearchMode::Hybrid, SearchMode::Semantic] {
        let res = search_chunks(&pool, ada, &request("aardvark", mode), &models)
            .await
            .expect("no error");
        assert_eq!(res.meta.requested_mode, mode);
        assert_eq!(res.meta.mode, SearchMode::Keyword);
        assert!(res.meta.degraded);
        assert!(
            res.meta.warnings[0].contains("ONNX Runtime"),
            "{:?}",
            res.meta.warnings
        );
        assert_eq!(res.results.len(), 1);
        assert_eq!(res.results[0].chunk.scores.keyword_rank, Some(1));
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn reranker_reorders_the_top_and_failures_only_warn(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    // Keyword ranks the repetitive chunk first; overlap prefers the one with all words.
    file(
        &pool,
        ada,
        "a.txt",
        TXT,
        &["night night night night aardvark"],
    )
    .await;
    file(
        &pool,
        ada,
        "b.txt",
        TXT,
        &["night: the aardvark digs a burrow"],
    )
    .await;
    let mut req = request("night aardvark burrow", SearchMode::Semantic);
    req.rerank = true;

    let res = search_chunks(&pool, ada, &req, &models_with_rerank())
        .await
        .expect("search");
    assert!(res.meta.reranked);
    assert_eq!(res.results[0].text, "night: the aardvark digs a burrow");
    assert_eq!(res.results[0].chunk.scores.rerank, Some(1.0));

    let broken = Models {
        reranker: Err("model files missing".into()),
        ..models()
    };
    let res = search_chunks(&pool, ada, &req, &broken)
        .await
        .expect("search");
    assert!(!res.meta.reranked);
    assert!(
        res.meta
            .warnings
            .iter()
            .any(|w| w.contains("model files missing"))
    );
    assert_eq!(res.results.len(), 2);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn file_results_group_chunks_and_pages_work(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let many = file(
        &pool,
        ada,
        "many.pdf",
        "application/pdf",
        &[
            "aardvark one",
            "aardvark two",
            "aardvark three",
            "aardvark four",
        ],
    )
    .await;
    file(&pool, ada, "one.txt", TXT, &["an aardvark"]).await;

    let req = request("aardvark", SearchMode::Keyword);
    let files = search_files(&pool, ada, &req, &models())
        .await
        .expect("files");
    assert_eq!(files.results.len(), 2);
    let hit = files
        .results
        .iter()
        .find(|h| h.file.id == many)
        .expect("many.pdf");
    assert_eq!(hit.match_count, 4);
    assert_eq!(hit.matches.len(), 3, "at most three chunks per file");
    assert!(hit.matches.iter().all(|m| m.page.is_some()));
    assert_eq!(hit.file.mime_type, "application/pdf");

    let chunks = search_chunks(&pool, ada, &req, &models())
        .await
        .expect("chunks");
    assert_eq!(chunks.results.len(), 5);

    let mut page = request("aardvark", SearchMode::Keyword);
    page.limit = 2;
    let first = search_chunks(&pool, ada, &page, &models())
        .await
        .expect("p1");
    assert!(first.meta.has_more);
    page.offset = 4;
    let last = search_chunks(&pool, ada, &page, &models())
        .await
        .expect("p3");
    assert_eq!(last.results.len(), 1);
    assert!(!last.meta.has_more);
    let seen: Vec<i64> = first.results.iter().map(|r| r.chunk.chunk_id).collect();
    assert!(!seen.contains(&last.results[0].chunk.chunk_id));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn invalid_requests_are_rejected(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let invalid = |r: Result<_, SearchError>| matches!(r, Err(SearchError::InvalidRequest(_)));
    let m = models();
    let mut req = request("   ", SearchMode::Hybrid);
    assert!(invalid(
        search_chunks(&pool, ada, &req, &m).await.map(|_| ())
    ));
    req.query = "x".repeat(501);
    assert!(invalid(
        search_chunks(&pool, ada, &req, &m).await.map(|_| ())
    ));
    req.query = "ok".into();
    req.limit = 0;
    assert!(invalid(
        search_files(&pool, ada, &req, &m).await.map(|_| ())
    ));
    req.limit = 50;
    req.offset = 151;
    assert!(invalid(
        search_files(&pool, ada, &req, &m).await.map(|_| ())
    ));
    // Queries made only of operators or stop words are fine, just empty for keywords.
    let res = search_chunks(&pool, ada, &request("the -", SearchMode::Keyword), &m)
        .await
        .expect("search");
    assert!(res.results.is_empty());
}
