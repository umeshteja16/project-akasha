//! Owner isolation (the critical property), filters, similar files and the
//! HNSW path for large libraries.

mod support;

use akasha_search::{SearchMode, search_chunks, search_files, similar_files};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use support::{file, file_without_vectors, models, request, user};

const TXT: &str = "text/plain";

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn another_users_chunks_never_appear(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let bob = user(&pool, "bob@example.com").await;
    let ada_file = file(&pool, ada, "aardvark.txt", TXT, &["the aardvark digs"]).await;
    let bob_file = file(&pool, bob, "aardvark.txt", TXT, &["the aardvark digs"]).await;
    // Bob also has extra close matches that would outrank nothing of Ada's.
    file(&pool, bob, "more.txt", TXT, &["aardvark aardvark", "digs"]).await;

    for mode in [
        SearchMode::Keyword,
        SearchMode::Semantic,
        SearchMode::Hybrid,
    ] {
        let res = search_chunks(&pool, ada, &request("aardvark digs", mode), &models())
            .await
            .expect("search");
        assert!(!res.results.is_empty(), "{mode:?}");
        assert!(
            res.results.iter().all(|r| r.file.id == ada_file),
            "{mode:?} leaked another user's chunk"
        );
        let files = search_files(&pool, ada, &request("aardvark", mode), &models())
            .await
            .expect("files");
        assert!(
            files.results.iter().all(|r| r.file.id == ada_file),
            "{mode:?}"
        );

        // Naming Bob's file explicitly does not help.
        let mut req = request("aardvark", mode);
        req.filter.file_ids = vec![bob_file];
        let res = search_chunks(&pool, ada, &req, &models())
            .await
            .expect("search");
        assert!(res.results.is_empty(), "{mode:?}");
    }

    assert!(
        similar_files(&pool, ada, bob_file, 5)
            .await
            .expect("similar")
            .is_none()
    );
    let similar = similar_files(&pool, bob, bob_file, 5)
        .await
        .expect("similar")
        .expect("found");
    assert!(similar.iter().all(|s| s.file.id != ada_file));
    assert_eq!(similar.len(), 1);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn filters_restrict_results(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let pdf = file(&pool, ada, "a.pdf", "application/pdf", &["aardvark report"]).await;
    let txt = file(&pool, ada, "b.txt", TXT, &["aardvark notes"]).await;
    let png = file(&pool, ada, "c.png", "image/png", &["aardvark photo text"]).await;
    sqlx::query("UPDATE files SET created_at = now() - interval '10 days', tags = '{zoo,work}' WHERE id = $1")
        .bind(pdf)
        .execute(&pool)
        .await
        .expect("age pdf");
    sqlx::query("UPDATE files SET is_pinned = true, tags = '{zoo}' WHERE id = $1")
        .bind(txt)
        .execute(&pool)
        .await
        .expect("pin txt");

    let ids = |res: &akasha_search::ChunkResults| {
        let mut ids: Vec<_> = res.results.iter().map(|r| r.file.id).collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let sorted = |mut v: Vec<uuid::Uuid>| {
        v.sort();
        v
    };
    for mode in [
        SearchMode::Keyword,
        SearchMode::Semantic,
        SearchMode::Hybrid,
    ] {
        let run = |f: &dyn Fn(&mut akasha_search::SearchRequest)| {
            let mut req = request("aardvark", mode);
            f(&mut req);
            let pool = pool.clone();
            async move {
                search_chunks(&pool, ada, &req, &models())
                    .await
                    .expect("search")
            }
        };
        let res = run(&|r| r.filter.mime_patterns = vec!["application/pdf".into()]).await;
        assert_eq!(ids(&res), vec![pdf], "{mode:?} type");
        let res = run(&|r| r.filter.mime_patterns = vec!["image/%".into(), "text/%".into()]).await;
        assert_eq!(ids(&res), sorted(vec![png, txt]), "{mode:?} types");
        let res = run(&|r| r.filter.from = Some(Utc::now() - Duration::days(1))).await;
        assert_eq!(ids(&res), sorted(vec![png, txt]), "{mode:?} from");
        let res = run(&|r| r.filter.to = Some(Utc::now() - Duration::days(1))).await;
        assert_eq!(ids(&res), vec![pdf], "{mode:?} to");
        let res = run(&|r| r.filter.tags = vec!["zoo".into()]).await;
        assert_eq!(ids(&res), sorted(vec![pdf, txt]), "{mode:?} tag");
        let res = run(&|r| r.filter.tags = vec!["zoo".into(), "work".into()]).await;
        assert_eq!(ids(&res), vec![pdf], "{mode:?} all tags");
        let res = run(&|r| r.filter.pinned = Some(true)).await;
        assert_eq!(ids(&res), vec![txt], "{mode:?} pinned");
        let res = run(&|r| r.filter.file_ids = vec![png, pdf]).await;
        assert_eq!(ids(&res), sorted(vec![png, pdf]), "{mode:?} ids");
    }
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn similar_files_rank_by_shared_content(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let src = file(
        &pool,
        ada,
        "src.txt",
        TXT,
        &["aardvark burrow night", "aardvark digs"],
    )
    .await;
    let close = file(&pool, ada, "close.txt", TXT, &["aardvark burrow at night"]).await;
    let far = file(&pool, ada, "far.txt", TXT, &["quantum chess opening"]).await;
    let pending = file_without_vectors(&pool, ada, "pending.txt", TXT, &["aardvark"]).await;

    let similar = similar_files(&pool, ada, src, 5)
        .await
        .expect("ok")
        .expect("found");
    let ids: Vec<_> = similar.iter().map(|s| s.file.id).collect();
    assert_eq!(
        ids,
        vec![close, far],
        "source and unembedded files excluded"
    );
    assert!(similar[0].similarity > similar[1].similarity);
    assert_eq!(similar[0].file.name, "close.txt");

    let one = similar_files(&pool, ada, src, 1)
        .await
        .expect("ok")
        .expect("found");
    assert_eq!(one.len(), 1);
    // Not embedded yet: nothing to compare, but the file exists.
    let none = similar_files(&pool, ada, pending, 5)
        .await
        .expect("ok")
        .expect("exists");
    assert!(none.is_empty());
    assert!(
        similar_files(&pool, ada, uuid::Uuid::new_v4(), 5)
            .await
            .expect("ok")
            .is_none()
    );
    assert!(similar_files(&pool, ada, src, 0).await.is_err());
}

/// Above the exact-scan threshold the HNSW index serves semantic search.
#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn large_libraries_use_the_vector_index(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let target = file(&pool, ada, "target.txt", TXT, &["aardvark burrow"]).await;
    let bulk = file_without_vectors(&pool, ada, "bulk.txt", TXT, &[]).await;
    let n = akasha_db::search::EXACT_SCAN_MAX_CHUNKS + 10;
    // Inserting into a live HNSW index row by row takes ~17 s; a bulk build after
    // the insert takes ~1 s. Same index definition as migration 0007.
    sqlx::query("DROP INDEX file_chunks_embedding_idx")
        .execute(&pool)
        .await
        .expect("drop index");
    sqlx::query(
        "INSERT INTO file_chunks (file_id, owner_id, chunk_index, char_start, char_end, text, embedding)
         SELECT $1, $2, g, 0, 1, 'x',
                (SELECT array_agg(sin(g * 7.0 + d)::real) FROM generate_series(1, 384) d)::vector
         FROM generate_series(0, $3::int - 1) g",
    )
    .bind(bulk)
    .bind(ada)
    .bind(i32::try_from(n).expect("n"))
    .execute(&pool)
    .await
    .expect("bulk chunks");
    sqlx::query(
        "CREATE INDEX file_chunks_embedding_idx ON file_chunks USING hnsw (embedding vector_cosine_ops)",
    )
    .execute(&pool)
    .await
    .expect("build index");

    // The bulk vectors are noise: keep the loosely related ones to count them.
    let mut req = request("aardvark burrow", SearchMode::Semantic);
    req.include_weak = true;
    let res = search_chunks(&pool, ada, &req, &models())
        .await
        .expect("search");
    assert_eq!(res.results.len(), 10);
    assert_eq!(res.results[0].file.id, target);
}
