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
        filter.after = Some((ListKey::of(last, ListOrder::Newest), last.id));
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

/// Walk every page of `order` with page size 2.
async fn walk(pool: &PgPool, owner: Uuid, order: ListOrder) -> Vec<Uuid> {
    let mut filter = ListFilter {
        order,
        limit: 2,
        ..Default::default()
    };
    let mut seen = Vec::new();
    loop {
        let page = list(pool, owner, &filter).await.expect("page");
        let Some(last) = page.last() else { break };
        filter.after = Some((ListKey::of(last, order), last.id));
        seen.extend(page.iter().map(|f| f.id));
    }
    seen
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn list_sorts_and_paginates_in_every_order(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    // Sizes 10..50; names chosen so name order differs from both.
    let names = [
        "delta.txt",
        "Alpha.txt",
        "charlie.txt",
        "bravo.txt",
        "alpha.txt",
    ];
    let mut files = Vec::new();
    for (n, name) in (1..=5).zip(names) {
        let file = add(&pool, ada, n, "text/plain").await;
        let changes = FileChanges {
            original_name: Some(name),
            ..FileChanges::default()
        };
        update(&pool, ada, file.id, changes).await.expect("rename");
        files.push(file);
    }
    let ids = |idx: &[usize]| idx.iter().map(|&i| files[i].id).collect::<Vec<_>>();
    let newest = walk(&pool, ada, ListOrder::Newest).await;
    assert_eq!(newest, ids(&[4, 3, 2, 1, 0]));
    assert_eq!(
        walk(&pool, ada, ListOrder::Oldest).await,
        ids(&[0, 1, 2, 3, 4])
    );
    assert_eq!(
        walk(&pool, ada, ListOrder::Largest).await,
        ids(&[4, 3, 2, 1, 0])
    );

    // "Alpha" and "alpha" tie case-insensitively; the id breaks the tie.
    let by_name = walk(&pool, ada, ListOrder::Name).await;
    let (a, b) = if files[1].id < files[4].id {
        (1, 4)
    } else {
        (4, 1)
    };
    assert_eq!(by_name, ids(&[a, b, 3, 2, 0]));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn tag_counts_split_own_and_suggested(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let bob = user(&pool, "bob@x.y").await;
    let one = add(&pool, ada, 1, "text/plain").await;
    let two = add(&pool, ada, 2, "text/plain").await;
    add(&pool, bob, 3, "text/plain").await;
    let own = vec!["work".to_owned()];
    let auto = vec!["work".to_owned(), "tax".to_owned()];
    for id in [one.id, two.id] {
        let changes = FileChanges {
            auto_tags: Some(&auto),
            ..FileChanges::default()
        };
        update(&pool, ada, id, changes).await.expect("auto");
    }
    let changes = FileChanges {
        tags: Some(&own),
        ..FileChanges::default()
    };
    update(&pool, ada, one.id, changes).await.expect("own");

    let counts = tag_counts(&pool, ada).await.expect("counts");
    let tax = TagCount {
        tag: "tax".into(),
        user_files: 0,
        auto_files: 2,
    };
    let work = TagCount {
        tag: "work".into(),
        user_files: 1,
        auto_files: 1,
    };
    assert_eq!(counts, vec![tax, work]);
    assert!(tag_counts(&pool, bob).await.expect("bob").is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn hash_lock_is_reentrant_within_a_transaction(pool: PgPool) {
    let mut tx = pool.begin().await.expect("begin");
    lock_hash(&mut tx, &hash(9)).await.expect("lock");
    lock_hash(&mut tx, &hash(9)).await.expect("relock");
    tx.commit().await.expect("commit");
}
