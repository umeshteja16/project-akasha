//! Listing a watched folder (blocking; run on the blocking pool).

use std::{collections::HashMap, path::Path, time::SystemTime};

use chrono::{DateTime, Utc};

use super::filter::Filter;

/// Most files considered in one folder.
pub const MAX_FILES: usize = 200_000;

/// A file found in the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub size: u64,
    /// Truncated to microseconds (what Postgres keeps).
    pub mtime: DateTime<Utc>,
}

/// The folder's candidate files by relative path, `/`-separated.
#[derive(Debug, Default)]
pub struct Listing {
    pub files: HashMap<String, Seen>,
    /// Entries that could not be read (permissions, vanished mid-walk). Deletions are
    /// skipped when any occurred, so an unreadable subfolder never looks deleted.
    pub errors: u64,
    /// Symlinks and non-UTF-8 names, never followed or imported.
    pub ignored: u64,
}

/// Why a folder could not be listed at all.
#[derive(Debug, thiserror::Error)]
pub enum WalkError {
    #[error("the folder has more than {MAX_FILES} matching files; narrow it with include globs")]
    TooMany,
    #[error("the folder cannot be read: {0}")]
    Root(std::io::Error),
}

/// List `root` (canonical). Never follows symlinks.
pub fn list(root: &Path, filter: &Filter) -> Result<Listing, WalkError> {
    std::fs::read_dir(root).map_err(WalkError::Root)?;
    let mut out = Listing::default();
    let mut walk = walkdir::WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter();
    while let Some(entry) = walk.next() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                out.errors += 1;
                continue;
            }
        };
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            out.ignored += 1;
            continue;
        }
        let Some(rel) = relative(root, entry.path()) else {
            out.ignored += 1;
            if file_type.is_dir() {
                walk.skip_current_dir();
            }
            continue;
        };
        if file_type.is_dir() {
            if !filter.enter_dir(&rel) {
                walk.skip_current_dir();
            }
            continue;
        }
        if !file_type.is_file() || !filter.accept_file(&rel) {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(_) => {
                out.errors += 1;
                continue;
            }
        };
        if out.files.len() >= MAX_FILES {
            return Err(WalkError::TooMany);
        }
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        out.files.insert(
            rel,
            Seen {
                size: meta.len(),
                mtime: micros(mtime),
            },
        );
    }
    Ok(out)
}

/// `path` relative to `root`, `/`-separated; `None` for non-UTF-8 names.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    Some(parts?.join("/"))
}

/// A file time as stored in Postgres (microsecond precision).
pub fn micros(time: SystemTime) -> DateTime<Utc> {
    let t: DateTime<Utc> = time.into();
    DateTime::from_timestamp_micros(t.timestamp_micros()).unwrap_or(t)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn lists_supported_files_without_following_symlinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = std::fs::canonicalize(tmp.path()).expect("canonical");
        let outside = tempfile::tempdir().expect("outside");
        std::fs::write(outside.path().join("secret.md"), "s").expect("write");
        std::fs::create_dir_all(root.join("a/b")).expect("mkdir");
        std::fs::create_dir_all(root.join(".obsidian")).expect("mkdir");
        std::fs::write(root.join("a/b/note.md"), "hello").expect("write");
        std::fs::write(root.join("top.txt"), "x").expect("write");
        std::fs::write(root.join("app.exe"), "x").expect("write");
        std::fs::write(root.join(".obsidian/workspace.json"), "{}").expect("write");
        symlink(outside.path(), root.join("linked")).expect("symlink");
        symlink(outside.path().join("secret.md"), root.join("a/secret.md")).expect("symlink");

        let filter = Filter::new(&[], &[]).expect("filter");
        let listing = list(&root, &filter).expect("list");
        let mut names: Vec<&str> = listing.files.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, ["a/b/note.md", "top.txt"]);
        assert_eq!(listing.files["a/b/note.md"].size, 5);
        assert_eq!((listing.ignored, listing.errors), (2, 0));
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        let filter = Filter::new(&[], &[]).expect("filter");
        assert!(matches!(
            list(Path::new("/definitely/not/here"), &filter),
            Err(WalkError::Root(_))
        ));
    }
}
