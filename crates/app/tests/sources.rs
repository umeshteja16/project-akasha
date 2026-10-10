//! Watched folders: import, changes, renames, deletions, symlink and root
//! protection, owner isolation, idempotent rescans and removal.

mod support;

use std::{
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use akasha_core::Config;
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{
    TestApp,
    mcp::{bearer, token},
    test_config,
};

/// A temporary watch root with an `outside` sibling that must never be read.
struct Tree {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    outside: PathBuf,
    config: Config,
}

fn tree() -> Tree {
    let tmp = tempfile::tempdir().expect("tempdir");
    let base = std::fs::canonicalize(tmp.path()).expect("canonical");
    let root = base.join("root");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::create_dir_all(&outside).expect("mkdir");
    std::fs::write(outside.join("secret.txt"), "top secret outside the root").expect("write");
    let config = Config {
        watch_roots: vec![root.display().to_string()],
        ..test_config()
    };
    Tree {
        _tmp: tmp,
        root,
        outside,
        config,
    }
}

/// Write a file with a modification time `age` seconds ago (scans skip files that
/// changed in the last two seconds).
fn put(dir: &Path, rel: &str, bytes: &[u8], age: u64) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, bytes).expect("write");
    let file = std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("open");
    file.set_modified(SystemTime::now() - Duration::from_secs(age))
        .expect("mtime");
}

async fn add(app: &TestApp, cookie: &str, body: Value) -> Value {
    let res = app
        .send("POST", "/api/v1/sources", cookie, Some(body))
        .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.json());
    res.json()
}

async fn files(app: &TestApp, cookie: &str) -> Vec<Value> {
    let res = app
        .send("GET", "/api/v1/files?limit=100", cookie, None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let mut items = res.json()["items"].as_array().expect("items").clone();
    items.sort_by_key(|f| f["name"].as_str().unwrap_or_default().to_owned());
    items
}

fn names(files: &[Value]) -> Vec<&str> {
    files
        .iter()
        .map(|f| f["name"].as_str().expect("name"))
        .collect()
}

async fn source(app: &TestApp, cookie: &str, id: &str) -> Value {
    let res = app
        .send("GET", &format!("/api/v1/sources/{id}"), cookie, None)
        .await;
    assert_eq!(res.status, StatusCode::OK);
    res.json()
}

async fn rescan(app: &TestApp, cookie: &str, id: &str, config: &Config) {
    let res = app
        .send("POST", &format!("/api/v1/sources/{id}/scan"), cookie, None)
        .await;
    assert_eq!(res.status, StatusCode::ACCEPTED, "{:?}", res.json());
    app.run_jobs_with(config).await;
}

const NOTE: &[u8] =
    b"---\ntitle: Aardvarks\ntags: [Animals, \"field notes\"]\n---\nThe aardvark digs burrows.\n";

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn imports_a_vault_and_rescans_idempotently(pool: PgPool) {
    let t = tree();
    let vault = t.root.join("Vault");
    put(&vault, "notes/aardvark.md", NOTE, 60);
    put(&vault, "todo.txt", b"buy termites", 60);
    put(&vault, ".obsidian/workspace.json", b"{}", 60);
    put(&vault, ".trash/old.md", b"deleted note", 60);
    put(&vault, "Archive/2019.md", b"old stuff", 60);
    put(&vault, "setup.exe", b"MZ binary", 60);
    put(&vault, "fake.pdf", b"not really a pdf", 60);
    let app = TestApp::with_config(pool, t.config.clone());
    let ada = app.user("ada@example.com").await;

    let created = add(
        &app,
        &ada,
        json!({ "path": vault.display().to_string(), "exclude_globs": ["Archive/**"] }),
    )
    .await;
    assert_eq!(created["name"], "Vault");
    assert_eq!(created["status"], "pending");
    let id = created["id"].as_str().expect("id").to_owned();
    app.run_jobs_with(&t.config).await;

    let imported = files(&app, &ada).await;
    assert_eq!(names(&imported), ["aardvark.md", "todo.txt"]);
    assert_eq!(imported[0]["tags"], json!(["animals", "field notes"]));
    assert!(
        imported.iter().all(|f| f["status"] == "ready"),
        "{imported:?}"
    );
    let s = source(&app, &ada, &id).await;
    assert_eq!(s["status"], "ok", "{s:?}");
    assert_eq!(s["file_count"], 2);
    assert_eq!(s["skipped_count"], 1, "fake.pdf is not a PDF");
    assert_eq!(s["last_scan"]["imported"], 2);
    let search = app
        .send("GET", "/api/v1/search?q=burrows", &ada, None)
        .await;
    assert_eq!(
        search.json()["results"][0]["file"]["name"],
        "aardvark.md",
        "{:?}",
        search.json()
    );

    // Nothing changed: same files, nothing imported.
    rescan(&app, &ada, &id, &t.config).await;
    let again = files(&app, &ada).await;
    assert_eq!(
        again.iter().map(|f| &f["id"]).collect::<Vec<_>>(),
        imported.iter().map(|f| &f["id"]).collect::<Vec<_>>()
    );
    let s = source(&app, &ada, &id).await;
    assert_eq!(
        (
            s["last_scan"]["imported"].as_u64(),
            s["last_scan"]["updated"].as_u64()
        ),
        (Some(0), Some(0))
    );
    assert_eq!(s["last_scan"]["files"], 3);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn follows_changes_renames_and_deletions(pool: PgPool) {
    let t = tree();
    put(&t.root, "a.md", b"first version about otters", 120);
    put(&t.root, "b.txt", b"badger notes", 120);
    put(&t.root, "c.txt", b"capybara notes", 120);
    let app = TestApp::with_config(pool, t.config.clone());
    let ada = app.user("ada@example.com").await;
    let id = add(&app, &ada, json!({ "path": t.root.display().to_string() })).await["id"]
        .as_str()
        .expect("id")
        .to_owned();
    app.run_jobs_with(&t.config).await;
    let before = files(&app, &ada).await;
    assert_eq!(names(&before), ["a.md", "b.txt", "c.txt"]);
    let a_id = before[0]["id"].clone();
    let b_id = before[1]["id"].clone();
    // Tags and pins survive content changes.
    let res = app
        .send(
            "PATCH",
            &format!("/api/v1/files/{}", a_id.as_str().expect("id")),
            &ada,
            Some(json!({ "tags": ["mine"], "is_pinned": true })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);

    put(
        &t.root,
        "a.md",
        b"second version, now about owls and otters",
        30,
    );
    std::fs::rename(t.root.join("b.txt"), t.root.join("renamed.txt")).expect("rename");
    std::fs::remove_file(t.root.join("c.txt")).expect("remove");
    rescan(&app, &ada, &id, &t.config).await;

    let after = files(&app, &ada).await;
    assert_eq!(names(&after), ["a.md", "renamed.txt"]);
    assert_eq!(after[0]["id"], a_id, "changed file keeps its id");
    assert_eq!(after[0]["tags"], json!(["mine"]));
    assert_eq!(after[0]["is_pinned"], true);
    assert_eq!(after[0]["size_bytes"], 41);
    assert_eq!(after[0]["status"], "ready");
    assert_eq!(after[1]["id"], b_id, "renamed file keeps its id");
    let s = source(&app, &ada, &id).await;
    assert_eq!(s["last_scan"]["updated"], 1, "{s:?}");
    assert_eq!(s["last_scan"]["removed"], 1, "{s:?}");
    let search = app.send("GET", "/api/v1/search?q=owls", &ada, None).await;
    assert_eq!(search.json()["results"][0]["file"]["name"], "a.md");

    // `keep`: a vanished file stays in the library.
    let res = app
        .send(
            "PATCH",
            &format!("/api/v1/sources/{id}"),
            &ada,
            Some(json!({ "on_delete": "keep" })),
        )
        .await;
    assert_eq!(res.json()["on_delete"], "keep");
    std::fs::remove_file(t.root.join("renamed.txt")).expect("remove");
    rescan(&app, &ada, &id, &t.config).await;
    assert_eq!(names(&files(&app, &ada).await), ["a.md", "renamed.txt"]);

    // An emptied folder (unmounted volume?) deletes nothing and reports it.
    std::fs::remove_file(t.root.join("a.md")).expect("remove");
    let res = app
        .send(
            "PATCH",
            &format!("/api/v1/sources/{id}"),
            &ada,
            Some(json!({ "on_delete": "delete" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    rescan(&app, &ada, &id, &t.config).await;
    let s = source(&app, &ada, &id).await;
    assert_eq!(s["status"], "error");
    assert!(
        s["last_error"].as_str().expect("error").contains("mounted"),
        "{s:?}"
    );
    assert_eq!(files(&app, &ada).await.len(), 2);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn paths_outside_the_roots_and_symlinks_are_refused(pool: PgPool) {
    let t = tree();
    put(&t.root, "inside.txt", b"inside the root", 60);
    symlink(&t.outside, t.root.join("escape")).expect("symlink");
    symlink(t.outside.join("secret.txt"), t.root.join("secret-link.txt")).expect("symlink");
    let app = TestApp::with_config(pool, t.config.clone());
    let ada = app.user("ada@example.com").await;

    let refused = [
        t.outside.display().to_string(),
        format!("{}/../outside", t.root.display()),
        t.root.join("escape").display().to_string(),
        "/".to_owned(),
        format!("{}/does-not-exist/../../outside", t.root.display()),
    ];
    for path in refused {
        let res = app
            .send(
                "POST",
                "/api/v1/sources",
                &ada,
                Some(json!({ "path": path })),
            )
            .await;
        assert_eq!(
            res.status,
            StatusCode::FORBIDDEN,
            "{path}: {:?}",
            res.json()
        );
    }
    let res = app
        .send(
            "POST",
            "/api/v1/sources",
            &ada,
            Some(json!({ "path": "relative/dir" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    let res = app
        .send(
            "POST",
            "/api/v1/sources",
            &ada,
            Some(json!({ "path": t.root.join("missing").display().to_string() })),
        )
        .await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);

    // Watching the root imports the regular file only: symlinks are never followed.
    add(&app, &ada, json!({ "path": t.root.display().to_string() })).await;
    app.run_jobs_with(&t.config).await;
    assert_eq!(names(&files(&app, &ada).await), ["inside.txt"]);

    // Overlapping folders are refused.
    std::fs::create_dir(t.root.join("sub")).expect("mkdir");
    let res = app
        .send(
            "POST",
            "/api/v1/sources",
            &ada,
            Some(json!({ "path": t.root.join("sub").display().to_string() })),
        )
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);

    // The admin narrows the roots: the next scan stops without touching anything.
    let narrowed = Config {
        watch_roots: vec![t.root.join("sub").display().to_string()],
        ..t.config.clone()
    };
    let list = app.send("GET", "/api/v1/sources", &ada, None).await.json();
    let id = list["items"][0]["id"].as_str().expect("id").to_owned();
    put(&t.root, "new.txt", b"new file", 60);
    rescan(&app, &ada, &id, &narrowed).await;
    let s = source(&app, &ada, &id).await;
    assert_eq!(s["status"], "error");
    assert!(
        s["last_error"]
            .as_str()
            .expect("msg")
            .contains("watch roots")
    );
    assert_eq!(names(&files(&app, &ada).await), ["inside.txt"]);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn sources_belong_to_their_owner(pool: PgPool) {
    let t = tree();
    put(&t.root, "shared.txt", b"a shared folder", 60);
    let app = TestApp::with_config(pool, t.config.clone());
    let ada = app.user("ada@example.com").await;
    let bob = app.user("bob@example.com").await;
    let id = add(&app, &ada, json!({ "path": t.root.display().to_string() })).await["id"]
        .as_str()
        .expect("id")
        .to_owned();
    app.run_jobs_with(&t.config).await;

    let path = format!("/api/v1/sources/{id}");
    for (method, path, body) in [
        ("GET", path.clone(), None),
        ("PATCH", path.clone(), Some(json!({ "enabled": false }))),
        ("POST", format!("{path}/scan"), None),
        ("DELETE", format!("{path}?delete_files=true"), None),
    ] {
        let res = app.send(method, &path, &bob, body).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{method} {path}");
    }
    let list = app.send("GET", "/api/v1/sources", &bob, None).await.json();
    assert_eq!(list["items"], json!([]));
    assert_eq!(list["roots"], json!([t.root.display().to_string()]));
    assert!(files(&app, &bob).await.is_empty());
    assert_eq!(names(&files(&app, &ada).await), ["shared.txt"]);

    // Bob may watch the same folder: he gets his own copy.
    add(&app, &bob, json!({ "path": t.root.display().to_string() })).await;
    app.run_jobs_with(&t.config).await;
    assert_eq!(names(&files(&app, &bob).await), ["shared.txt"]);

    // A read-only token can look but not add.
    let read = token(&app, &bob, &["read"]).await;
    let res = bearer(&app, "GET", "/api/v1/sources", &read, None).await;
    assert_eq!(res.status, StatusCode::OK);
    let res = bearer(
        &app,
        "POST",
        "/api/v1/sources",
        &read,
        Some(json!({ "path": "/" })),
    )
    .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn pausing_and_removing_a_source(pool: PgPool) {
    let t = tree();
    put(&t.root, "mine.txt", b"uploaded by hand as well", 60);
    put(&t.root, "only-here.txt", b"only in the folder", 60);
    let app = TestApp::with_config(pool, t.config.clone());
    let ada = app.user("ada@example.com").await;
    // Uploaded before: the folder maps it but did not create it.
    let res = app
        .upload(&ada, "mine.txt", b"uploaded by hand as well")
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let id = add(&app, &ada, json!({ "path": t.root.display().to_string() })).await["id"]
        .as_str()
        .expect("id")
        .to_owned();
    app.run_jobs_with(&t.config).await;
    assert_eq!(
        names(&files(&app, &ada).await),
        ["mine.txt", "only-here.txt"]
    );

    let res = app
        .send(
            "PATCH",
            &format!("/api/v1/sources/{id}"),
            &ada,
            Some(json!({ "enabled": false })),
        )
        .await;
    assert_eq!(res.json()["enabled"], false);
    let res = app
        .send("POST", &format!("/api/v1/sources/{id}/scan"), &ada, None)
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    put(
        &t.root,
        "while-paused.txt",
        b"not imported while paused",
        60,
    );
    app.run_jobs_with(&t.config).await;
    assert_eq!(files(&app, &ada).await.len(), 2);

    // Resuming scans again.
    app.send(
        "PATCH",
        &format!("/api/v1/sources/{id}"),
        &ada,
        Some(json!({ "enabled": true })),
    )
    .await;
    app.run_jobs_with(&t.config).await;
    assert_eq!(files(&app, &ada).await.len(), 3);

    let res = app
        .send(
            "DELETE",
            &format!("/api/v1/sources/{id}?delete_files=true"),
            &ada,
            None,
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["deleted_files"], 2);
    assert_eq!(names(&files(&app, &ada).await), ["mine.txt"]);
    let list = app.send("GET", "/api/v1/sources", &ada, None).await.json();
    assert_eq!(list["items"], json!([]));
}

#[sqlx::test(migrator = "akasha_db::MIGRATOR")]
async fn watched_folders_are_off_without_roots(pool: PgPool) {
    let t = tree();
    let app = TestApp::new(pool);
    let ada = app.user("ada@example.com").await;
    let res = app
        .send(
            "POST",
            "/api/v1/sources",
            &ada,
            Some(json!({ "path": t.root.display().to_string() })),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let list = app.send("GET", "/api/v1/sources", &ada, None).await.json();
    assert_eq!(list, json!({ "items": [], "roots": [] }));
}
