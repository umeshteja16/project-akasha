use super::*;
use crate::{
    MIGRATOR,
    files::{self, NewFile},
    users,
};

async fn user(pool: &PgPool, email: &str) -> Uuid {
    match users::create(pool, email, "h", None).await.expect("create") {
        users::Created::Ok(u) => u.id,
        users::Created::EmailTaken => panic!("email taken"),
    }
}

async fn file(pool: &PgPool, owner: Uuid, n: u8) -> Uuid {
    let mut conn = pool.acquire().await.expect("conn");
    let h = format!("{n:02x}").repeat(32);
    files::insert(
        &mut conn,
        &NewFile {
            owner_id: owner,
            original_name: "a.txt",
            content_hash: &h,
            mime_type: "text/plain",
            size_bytes: 1,
        },
    )
    .await
    .expect("insert")
    .expect("new")
    .id
}

async fn source(pool: &PgPool, owner: Uuid, path: &str) -> Option<Source> {
    let mut conn = pool.acquire().await.expect("conn");
    create(
        &mut conn,
        &NewSource {
            owner_id: owner,
            name: "Notes",
            path,
            include_globs: &[],
            exclude_globs: &[],
            on_delete: "delete",
            import_tags: true,
        },
    )
    .await
    .expect("create")
}

fn state(s: &Source, rel: &'static str, file_id: Option<Uuid>) -> FileState<'static> {
    FileState {
        source_id: s.id,
        owner_id: s.owner_id,
        rel_path: rel,
        size_bytes: 1,
        mtime: Utc::now(),
        content_hash: None,
        file_id,
        created_file: true,
        skip_reason: None,
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_path_is_watched_once_per_owner(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let bob = user(&pool, "bob@x.y").await;
    assert!(source(&pool, ada, "/data/notes").await.is_some());
    assert!(source(&pool, ada, "/data/notes").await.is_none());
    assert!(source(&pool, bob, "/data/notes").await.is_some());
    assert_eq!(list(&pool, ada).await.expect("list").len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sources_cannot_map_another_users_file(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let bob = user(&pool, "bob@x.y").await;
    let s = source(&pool, ada, "/data/a").await.expect("source");
    let bobs = file(&pool, bob, 1).await;
    let mut conn = pool.acquire().await.expect("conn");
    let err = upsert_file(&mut conn, &state(&s, "x.txt", Some(bobs)))
        .await
        .expect_err("foreign key");
    assert!(matches!(err, sqlx::Error::Database(_)), "{err:?}");
    // Other users see nothing of it.
    assert!(get(&pool, bob, s.id).await.expect("get").is_none());
    assert!(
        update(
            &pool,
            bob,
            s.id,
            SourceChanges {
                enabled: Some(false),
                ..Default::default()
            }
        )
        .await
        .expect("update")
        .is_none()
    );
    assert!(!delete(&mut conn, bob, s.id).await.expect("delete"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn deleting_a_file_keeps_the_row_without_it(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let s = source(&pool, ada, "/data/a").await.expect("source");
    let f = file(&pool, ada, 1).await;
    let mut conn = pool.acquire().await.expect("conn");
    upsert_file(&mut conn, &state(&s, "x.txt", Some(f)))
        .await
        .expect("upsert");
    let summary = summary(&pool, ada, s.id)
        .await
        .expect("summary")
        .expect("found");
    assert_eq!(summary.file_count, 1);
    files::delete(&mut conn, ada, f).await.expect("delete");
    let rows = files_of(&pool, s.id).await.expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].file_id, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn created_files_skip_ones_another_source_maps(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let a = source(&pool, ada, "/data/a").await.expect("a");
    let b = source(&pool, ada, "/data/b").await.expect("b");
    let shared = file(&pool, ada, 1).await;
    let own = file(&pool, ada, 2).await;
    let mut conn = pool.acquire().await.expect("conn");
    upsert_file(&mut conn, &state(&a, "s.txt", Some(shared)))
        .await
        .expect("1");
    upsert_file(&mut conn, &state(&a, "o.txt", Some(own)))
        .await
        .expect("2");
    let mut other = state(&b, "s.txt", Some(shared));
    other.created_file = false;
    upsert_file(&mut conn, &other).await.expect("3");
    assert_eq!(
        created_files(&mut conn, ada, a.id).await.expect("created"),
        [own]
    );
    let moved = other_mapping(&mut conn, shared, a.id, "s.txt")
        .await
        .expect("other");
    assert_eq!(moved, Some((b.id, "s.txt".to_owned())));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_scan_lease_is_exclusive_until_it_ends_or_expires(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let s = source(&pool, ada, "/data/a").await.expect("source");
    assert!(acquire_scan(&pool, s.id, 600.0).await.expect("1").is_some());
    assert!(acquire_scan(&pool, s.id, 600.0).await.expect("2").is_none());
    assert!(
        acquire_scan(&pool, s.id, 0.0)
            .await
            .expect("stale")
            .is_some()
    );
    let stats = serde_json::json!({ "files": 3 });
    finish_scan(&pool, s.id, "ok", None, &stats, true)
        .await
        .expect("finish");
    let done = get(&pool, ada, s.id).await.expect("get").expect("found");
    assert_eq!(
        (done.status.as_str(), done.last_scan["files"].as_i64()),
        ("ok", Some(3))
    );
    assert!(done.last_scan_at.is_some() && done.scan_started_at.is_none());
    update(
        &pool,
        ada,
        s.id,
        SourceChanges {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("pause");
    assert!(
        acquire_scan(&pool, s.id, 600.0)
            .await
            .expect("paused")
            .is_none()
    );
    assert!(enabled(&pool).await.expect("enabled").is_empty());
}
