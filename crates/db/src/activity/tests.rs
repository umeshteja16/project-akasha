use serde_json::json;

use super::*;
use crate::{MIGRATOR, users};

async fn user(pool: &PgPool, email: &str) -> Uuid {
    match users::create(pool, email, "h", None).await.expect("create") {
        users::Created::Ok(u) => u.id,
        users::Created::EmailTaken => panic!("email taken"),
    }
}

fn page(limit: i64) -> ListFilter {
    ListFilter {
        limit,
        ..ListFilter::default()
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn events_are_listed_to_their_owner_only(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let bob = user(&pool, "bob@x.y").await;
    let mut conn = pool.acquire().await.expect("conn");
    for kind in ["file.uploaded", "file.renamed", "file.deleted"] {
        let mut ev = NewEvent::new(ada, kind, "files");
        ev.subject = Some("a.txt");
        record(&mut conn, &ev).await.expect("record");
    }
    let mut sec = NewEvent::new(ada, "auth.signed_in", "security");
    sec.ip = Some("10.1.2.3");
    record(&mut conn, &sec).await.expect("record");

    assert!(list(&pool, bob, &page(10)).await.expect("list").is_empty());
    let all = list(&pool, ada, &page(10)).await.expect("list");
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].kind, "auth.signed_in");

    // Keyset paging walks every row once.
    let first = list(&pool, ada, &page(3)).await.expect("list");
    let last = first.last().expect("row");
    let rest = list(
        &pool,
        ada,
        &ListFilter {
            before: Some((last.created_at, last.id)),
            limit: 3,
            ..ListFilter::default()
        },
    )
    .await
    .expect("list");
    assert_eq!(rest.len(), 1);

    let security = list(
        &pool,
        ada,
        &ListFilter {
            categories: vec!["security".into()],
            limit: 10,
            ..ListFilter::default()
        },
    )
    .await
    .expect("list");
    assert_eq!(security.len(), 1);
    assert_eq!(security[0].ip.as_deref(), Some("10.1.2.3"));

    // Clearing keeps the audit log.
    assert_eq!(clear(&pool, ada, &[]).await.expect("clear"), 3);
    assert_eq!(list(&pool, ada, &page(10)).await.expect("list").len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn typing_refines_one_search_and_history_can_be_off(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    for q in ["bud", "budget", "Budget 2026"] {
        record_search(&pool, ada, q, json!({}), "session")
            .await
            .expect("search");
    }
    record_search(&pool, ada, "taxes", json!({}), "session")
        .await
        .expect("search");
    let rows = list(&pool, ada, &page(10)).await.expect("list");
    let subjects: Vec<_> = rows.iter().filter_map(|r| r.subject.as_deref()).collect();
    assert_eq!(subjects, ["taxes", "Budget 2026"]);

    set_search_history(&pool, ada, false).await.expect("off");
    assert!(!keeps_search_history(&pool, ada).await.expect("get"));
    record_search(&pool, ada, "taxes 2025", json!({}), "session")
        .await
        .expect("search");
    record_search(&pool, ada, "secret", json!({}), "session")
        .await
        .expect("search");
    let rows = list(&pool, ada, &page(10)).await.expect("list");
    let subjects: Vec<_> = rows.iter().filter_map(|r| r.subject.as_deref()).collect();
    assert_eq!(subjects, ["taxes", "Budget 2026"]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn repeats_within_the_window_are_skipped(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let mut conn = pool.acquire().await.expect("conn");
    let ev = NewEvent::new(ada, "rate.limited", "security");
    assert!(
        record_unless_recent(&mut conn, &ev, 600.0)
            .await
            .expect("1")
    );
    assert!(
        !record_unless_recent(&mut conn, &ev, 600.0)
            .await
            .expect("2")
    );
    assert!(record_unless_recent(&mut conn, &ev, 0.0).await.expect("3"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn prune_removes_only_old_events(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let mut conn = pool.acquire().await.expect("conn");
    record(&mut conn, &NewEvent::new(ada, "file.uploaded", "files"))
        .await
        .expect("new");
    record(&mut conn, &NewEvent::new(ada, "file.deleted", "files"))
        .await
        .expect("old");
    sqlx::query!(
        "UPDATE activity_events SET created_at = now() - interval '400 days'
         WHERE kind = 'file.deleted'"
    )
    .execute(&mut *conn)
    .await
    .expect("age");
    assert_eq!(prune(&pool, 365).await.expect("prune"), 1);
    let left = list(&pool, ada, &page(10)).await.expect("list");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].kind, "file.uploaded");
}
