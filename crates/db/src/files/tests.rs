use super::*;
use crate::{MIGRATOR, users};

async fn user(pool: &PgPool, email: &str) -> Uuid {
    match users::create(pool, email, "h", None).await.expect("create") {
        users::Created::Ok(u) => u.id,
        users::Created::EmailTaken => panic!("email taken"),
    }
}

fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

async fn add(pool: &PgPool, owner: Uuid, n: u8, mime: &str) -> File {
    let mut conn = pool.acquire().await.expect("conn");
    let h = hash(n);
    insert(
        &mut conn,
        &NewFile {
            owner_id: owner,
            original_name: "a.txt",
            content_hash: &h,
            mime_type: mime,
            size_bytes: i64::from(n) * 10,
        },
    )
    .await
    .expect("insert")
    .expect("new row")
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn insert_dedupes_per_owner_and_counts_usage(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let bob = user(&pool, "bob@x.y").await;
    let first = add(&pool, ada, 1, "text/plain").await;
    assert_eq!(first.status, "pending");
    assert!(first.tags.is_empty());

    let mut conn = pool.acquire().await.expect("conn");
    let h = hash(1);
    let again = NewFile {
        owner_id: ada,
        original_name: "b.txt",
        content_hash: &h,
        mime_type: "text/plain",
        size_bytes: 10,
    };
    assert!(insert(&mut conn, &again).await.expect("insert").is_none());
    let found = find_by_hash(&mut conn, ada, &h).await.expect("find");
    assert_eq!(found.map(|f| f.id), Some(first.id));

    // Another user may hold the same blob.
    add(&pool, bob, 1, "text/plain").await;
    assert!(is_referenced(&mut conn, &h).await.expect("refs"));

    set_quota(&pool, ada, Some(100)).await.expect("quota");
    let u = usage(&mut conn, ada, false)
        .await
        .expect("usage")
        .expect("user");
    assert_eq!(
        u,
        Usage {
            quota: Some(100),
            used: 10
        }
    );
    assert_eq!(u.remaining(), Some(90));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn owner_filter_applies_everywhere(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let eve = user(&pool, "eve@x.y").await;
    let file = add(&pool, ada, 2, "image/png").await;

    assert!(get(&pool, eve, file.id).await.expect("get").is_none());
    let tags = vec!["x".to_owned()];
    let upd = update(
        &pool,
        eve,
        file.id,
        FileChanges {
            original_name: Some("pwn"),
            is_pinned: Some(true),
            tags: Some(&tags),
            auto_tags: None,
        },
    );
    assert!(upd.await.expect("update").is_none());
    let mut conn = pool.acquire().await.expect("conn");
    assert!(
        delete(&mut conn, eve, file.id)
            .await
            .expect("delete")
            .is_none()
    );
    let filter = ListFilter {
        limit: 10,
        ..Default::default()
    };
    assert!(list(&pool, eve, &filter).await.expect("list").is_empty());

    let deleted = delete(&mut conn, ada, file.id).await.expect("delete");
    assert_eq!(deleted, Some(hash(2)));
    assert!(!is_referenced(&mut conn, &hash(2)).await.expect("refs"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn list_filters_and_paginates(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    for n in 1..=5 {
        let mime = if n % 2 == 0 {
            "image/png"
        } else {
            "text/plain"
        };
        add(&pool, ada, n, mime).await;
    }
    let f3 = list(
        &pool,
        ada,
        &ListFilter {
            limit: 100,
            ..Default::default()
        },
    )
    .await
    .expect("list");
    assert_eq!(f3.len(), 5);
    let tags = vec!["work".to_owned()];
    update(
        &pool,
        ada,
        f3[0].id,
        FileChanges {
            is_pinned: Some(true),
            tags: Some(&tags),
            ..FileChanges::default()
        },
    )
    .await
    .expect("update");

    let mut seen = Vec::new();
    let mut filter = ListFilter {
        limit: 2,
        ..Default::default()
    };
    loop {
        let page = list(&pool, ada, &filter).await.expect("page");
        let Some(last) = page.last() else { break };
        filter.before = Some((last.created_at, last.id));
        seen.extend(page.iter().map(|f| f.id));
    }
    assert_eq!(seen, f3.iter().map(|f| f.id).collect::<Vec<_>>());

    let images = ListFilter {
        mime_patterns: vec!["image/%".into()],
        limit: 100,
        ..Default::default()
    };
    assert_eq!(list(&pool, ada, &images).await.expect("list").len(), 2);
    let pinned = ListFilter {
        pinned: Some(true),
        limit: 100,
        ..Default::default()
    };
    assert_eq!(list(&pool, ada, &pinned).await.expect("list").len(), 1);
    let tagged = ListFilter {
        tag: Some("work".into()),
        limit: 100,
        ..Default::default()
    };
    assert_eq!(list(&pool, ada, &tagged).await.expect("list").len(), 1);
    let ready = ListFilter {
        status: Some("ready".into()),
        limit: 100,
        ..Default::default()
    };
    assert!(list(&pool, ada, &ready).await.expect("list").is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn hash_lock_is_reentrant_within_a_transaction(pool: PgPool) {
    let mut tx = pool.begin().await.expect("begin");
    lock_hash(&mut tx, &hash(9)).await.expect("lock");
    lock_hash(&mut tx, &hash(9)).await.expect("relock");
    tx.commit().await.expect("commit");
}
