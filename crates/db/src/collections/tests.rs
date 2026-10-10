use super::*;
use crate::{
    MIGRATOR,
    files::{self, ListFilter, NewFile},
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

async fn make(pool: &PgPool, owner: Uuid, name: &str) -> Saved {
    let mut conn = pool.acquire().await.expect("conn");
    create(
        &mut conn,
        &NewCollection {
            owner_id: owner,
            name,
            description: "",
            color: "sage",
            icon: "folder",
        },
    )
    .await
    .expect("create")
}

fn ok(saved: Saved) -> Collection {
    match saved {
        Saved::Ok(c) => c,
        other => panic!("expected a collection, got {other:?}"),
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn names_are_unique_per_owner_ignoring_case(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let bob = user(&pool, "bob@x.y").await;
    let trips = ok(make(&pool, ada, "Trips").await);
    assert!(matches!(make(&pool, ada, "trips").await, Saved::NameTaken));
    // Another user may use the same name.
    ok(make(&pool, bob, "Trips").await);

    let other = ok(make(&pool, ada, "Taxes").await);
    let mut conn = pool.acquire().await.expect("conn");
    let rename = CollectionChanges {
        name: Some("TRIPS"),
        ..CollectionChanges::default()
    };
    assert!(matches!(
        update(&mut conn, ada, other.id, rename)
            .await
            .expect("update"),
        Saved::NameTaken
    ));
    // Bob cannot see or change Ada's collection.
    assert!(get(&pool, bob, trips.id).await.expect("get").is_none());
    let recolor = CollectionChanges {
        color: Some("plum"),
        ..CollectionChanges::default()
    };
    assert!(matches!(
        update(&mut conn, bob, trips.id, recolor)
            .await
            .expect("update"),
        Saved::NotFound
    ));
    assert!(
        delete(&mut conn, bob, trips.id)
            .await
            .expect("delete")
            .is_none()
    );
    let names: Vec<_> = list(&pool, ada)
        .await
        .expect("list")
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["Taxes", "Trips"]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn only_the_owners_files_can_be_added(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let bob = user(&pool, "bob@x.y").await;
    let a1 = file(&pool, ada, 1).await;
    let a2 = file(&pool, ada, 2).await;
    let b1 = file(&pool, bob, 3).await;
    let c = ok(make(&pool, ada, "Work").await);
    let mut conn = pool.acquire().await.expect("conn");

    let added = add_files(&mut conn, ada, c.id, &[a1, b1, a1])
        .await
        .expect("add");
    assert_eq!(added.iter().map(|f| f.id).collect::<Vec<_>>(), [a1]);
    assert_eq!(added[0].name, "a.txt");
    // Bob cannot add to (or remove from) Ada's collection, even his own file.
    assert!(
        add_files(&mut conn, bob, c.id, &[b1])
            .await
            .expect("add")
            .is_empty()
    );
    assert!(
        remove_files(&mut conn, bob, c.id, &[a1])
            .await
            .expect("rm")
            .is_empty()
    );
    // The schema refuses a cross-owner row outright.
    let forged = sqlx::query!(
        "INSERT INTO collection_files (collection_id, file_id, owner_id) VALUES ($1, $2, $3)",
        c.id,
        b1,
        ada
    )
    .execute(&mut *conn)
    .await;
    assert!(forged.is_err());

    add_files(&mut conn, ada, c.id, &[a2]).await.expect("add");
    assert_eq!(
        get(&pool, ada, c.id)
            .await
            .expect("get")
            .expect("c")
            .file_count,
        2
    );
    let refs = of_file(&pool, ada, a1).await.expect("of");
    assert_eq!(refs.iter().map(|r| r.id).collect::<Vec<_>>(), [c.id]);
    assert!(of_file(&pool, bob, a1).await.expect("of").is_empty());

    let listed = files::list(
        &pool,
        ada,
        &ListFilter {
            collection_id: Some(c.id),
            limit: 10,
            ..ListFilter::default()
        },
    )
    .await
    .expect("list");
    assert_eq!(listed.len(), 2);
    let as_bob = files::list(
        &pool,
        bob,
        &ListFilter {
            collection_id: Some(c.id),
            limit: 10,
            ..ListFilter::default()
        },
    )
    .await
    .expect("list");
    assert!(as_bob.is_empty());

    let removed = remove_files(&mut conn, ada, c.id, &[a2]).await.expect("rm");
    assert_eq!(removed.iter().map(|f| f.id).collect::<Vec<_>>(), [a2]);
    // Deleting the collection keeps the files.
    assert_eq!(
        delete(&mut conn, ada, c.id)
            .await
            .expect("delete")
            .as_deref(),
        Some("Work")
    );
    assert!(files::get(&pool, ada, a1).await.expect("get").is_some());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn opening_a_file_does_not_touch_updated_at(pool: PgPool) {
    let ada = user(&pool, "ada@x.y").await;
    let id = file(&pool, ada, 1).await;
    let before = files::get(&pool, ada, id)
        .await
        .expect("get")
        .expect("file");
    let mut conn = pool.acquire().await.expect("conn");
    let opened = files::mark_opened(&mut conn, ada, id)
        .await
        .expect("open")
        .expect("file");
    assert_eq!(opened.open_count, 1);
    assert!(opened.last_opened_at.is_some());
    assert_eq!(opened.updated_at, before.updated_at);
    let bob = user(&pool, "bob@x.y").await;
    assert!(
        files::mark_opened(&mut conn, bob, id)
            .await
            .expect("open")
            .is_none()
    );

    let recent = files::list(
        &pool,
        ada,
        &ListFilter {
            order: files::ListOrder::Opened,
            limit: 10,
            ..ListFilter::default()
        },
    )
    .await
    .expect("list");
    assert_eq!(recent.iter().map(|f| f.id).collect::<Vec<_>>(), [id]);
}
