//! "Did you mean": per-owner vocabulary maintained by triggers, owner isolation.

mod support;

use akasha_search::{SearchMode, search_files};
use sqlx::PgPool;
use support::{file, models, request, user};

const TXT: &str = "text/plain";

async fn suggestion(pool: &PgPool, owner: uuid::Uuid, q: &str, mode: SearchMode) -> Option<String> {
    search_files(pool, owner, &request(q, mode), &models())
        .await
        .expect("search")
        .meta
        .suggestion
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn misspelt_queries_get_a_suggestion_from_own_documents(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    file(
        &pool,
        ada,
        "zoo.txt",
        TXT,
        &["The aardvark burrows at night.", "Pangolins eat ants."],
    )
    .await;

    assert_eq!(
        suggestion(&pool, ada, "ardvark burows", SearchMode::Keyword)
            .await
            .as_deref(),
        Some("aardvark burrows")
    );
    // Hybrid always finds something semantically, but keyword hits are few.
    assert_eq!(
        suggestion(&pool, ada, "pangolin \"ardvark\"", SearchMode::Hybrid)
            .await
            .as_deref(),
        Some("pangolin \"aardvark\"")
    );
    // Correct queries and nonsense get none; semantic mode never suggests.
    assert_eq!(
        suggestion(&pool, ada, "aardvark", SearchMode::Keyword).await,
        None
    );
    assert_eq!(
        suggestion(&pool, ada, "qwxzrt", SearchMode::Keyword).await,
        None
    );
    assert_eq!(
        suggestion(&pool, ada, "ardvark", SearchMode::Semantic).await,
        None
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn suggestions_never_use_another_users_words(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let bob = user(&pool, "bob@example.com").await;
    file(&pool, ada, "a.txt", TXT, &["quarterly budget"]).await;
    file(
        &pool,
        bob,
        "b.txt",
        TXT,
        &["confidential merger with zanzibar"],
    )
    .await;

    assert_eq!(
        suggestion(&pool, ada, "zanzibr", SearchMode::Keyword).await,
        None
    );
    assert_eq!(
        suggestion(&pool, bob, "zanzibr", SearchMode::Keyword)
            .await
            .as_deref(),
        Some("zanzibar")
    );
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn deleted_documents_leave_the_vocabulary(pool: PgPool) {
    let ada = user(&pool, "ada@example.com").await;
    let gone = file(
        &pool,
        ada,
        "old.txt",
        TXT,
        &["xylophone lessons", "xylophone"],
    )
    .await;
    file(&pool, ada, "keep.txt", TXT, &["lessons learned"]).await;
    let count = |term: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i32>(
                "SELECT chunk_count FROM user_terms WHERE owner_id = $1 AND term = $2",
            )
            .bind(ada)
            .bind(term)
            .fetch_optional(&pool)
            .await
            .expect("count")
        }
    };
    assert_eq!(count("xylophone").await, Some(2));
    assert_eq!(count("lessons").await, Some(2));

    sqlx::query("DELETE FROM files WHERE id = $1")
        .bind(gone)
        .execute(&pool)
        .await
        .expect("delete");
    assert_eq!(count("xylophone").await, None);
    assert_eq!(count("lessons").await, Some(1));
    assert_eq!(
        suggestion(&pool, ada, "xylophon", SearchMode::Keyword).await,
        None
    );

    // Account deletion empties the user's vocabulary.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(ada)
        .execute(&pool)
        .await
        .expect("delete user");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM user_terms")
        .fetch_one(&pool)
        .await
        .expect("left");
    assert_eq!(left, 0);
}
