//! Which server directories a user may watch, and opening files in them safely.
//!
//! The admin lists root directories in `AKASHA_WATCH_ROOTS`; a root may contain
//! `{user_id}` or `{email}` to give every user their own subtree. A requested folder
//! is canonicalised (`..` and symlinks resolved) and must lie inside one of the
//! user's canonical roots, compared component by component. Scans re-check the
//! source's own path every time, skip symlinks inside the tree, and open files with
//! `O_NOFOLLOW`, then confirm that the opened inode is the one at the canonical path
//! inside the root, so a symlink swapped in during a scan cannot leak a file from
//! elsewhere.

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use akasha_core::Error;
use uuid::Uuid;

/// The user's watch roots: placeholders filled in, canonicalised, missing ones
/// dropped. Blocking (filesystem calls).
pub fn roots_for(configured: &[String], user_id: Uuid, email: &str) -> Vec<PathBuf> {
    let email = email.to_lowercase();
    let email_ok =
        !email.is_empty() && email != "." && email != ".." && !email.contains(['/', '\\', '\0']);
    configured
        .iter()
        .filter_map(|root| {
            if root.contains("{email}") && !email_ok {
                return None;
            }
            let root = root
                .replace("{user_id}", &user_id.to_string())
                .replace("{email}", &email);
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                return None;
            }
            let canonical = std::fs::canonicalize(&root).ok()?;
            canonical.is_dir().then_some(canonical)
        })
        .collect()
}

/// Whether a canonical path lies inside (or is) one of the canonical roots.
pub fn within(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| path.starts_with(root))
}

/// Check a folder a user asked to watch; returns its canonical path. Blocking.
pub fn resolve(requested: &str, roots: &[PathBuf]) -> Result<PathBuf, Error> {
    if roots.is_empty() {
        return Err(Error::forbidden(
            "watched folders are not enabled on this server (AKASHA_WATCH_ROOTS)",
        ));
    }
    let requested = requested.trim();
    if requested.is_empty() || requested.contains('\0') || requested.len() > 4096 {
        return Err(Error::bad_request("invalid folder path"));
    }
    let path = Path::new(requested);
    if !path.is_absolute() {
        return Err(Error::bad_request("the folder path must be absolute"));
    }
    let outside = || Error::forbidden("that folder is outside the allowed watch roots");
    // Same answer for "missing" and "outside" when outside: no probing of the server's
    // filesystem beyond the roots.
    let canonical = match std::fs::canonicalize(path) {
        Ok(c) => c,
        Err(_) if !within(&lexical(path), roots) => return Err(outside()),
        Err(_) => return Err(Error::bad_request("that folder does not exist")),
    };
    if !within(&canonical, roots) {
        return Err(outside());
    }
    if !canonical.is_dir() {
        return Err(Error::bad_request("that path is not a folder"));
    }
    Ok(canonical)
}

/// `path` with `.` and `..` resolved without touching the filesystem.
fn lexical(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Why a file could not be opened safely.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// A symlink, a special file, or a path that leads outside the root.
    #[error("not a regular file inside the folder")]
    Unsafe,
}

/// Open `root/rel` for reading without following symlinks out of `root` (canonical).
/// Blocking.
pub fn open_inside(root: &Path, rel: &Path) -> Result<(File, std::fs::Metadata), OpenError> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let path = root.join(rel);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|err| {
            if err.raw_os_error() == Some(libc::ELOOP) {
                OpenError::Unsafe
            } else {
                OpenError::Io(err)
            }
        })?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(OpenError::Unsafe);
    }
    // An intermediate directory may have been swapped for a symlink: the canonical
    // path must still be inside the root and name the very inode we opened.
    let canonical = std::fs::canonicalize(&path)?;
    let at_path = std::fs::symlink_metadata(&canonical)?;
    if !canonical.starts_with(root) || at_path.dev() != meta.dev() || at_path.ino() != meta.ino() {
        return Err(OpenError::Unsafe);
    }
    Ok((file, meta))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use akasha_core::ErrorCode;

    use super::*;

    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = std::fs::canonicalize(tmp.path()).expect("canonical");
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("notes")).expect("mkdir");
        std::fs::create_dir_all(&outside).expect("mkdir");
        std::fs::write(outside.join("secret.txt"), "secret").expect("write");
        std::fs::write(root.join("notes/a.md"), "a").expect("write");
        (tmp, root, outside)
    }

    #[test]
    fn folders_must_be_inside_a_root() {
        let (_tmp, root, outside) = dirs();
        let roots = vec![root.clone()];
        let ok = resolve(root.join("notes").to_str().expect("utf8"), &roots).expect("inside");
        assert_eq!(ok, root.join("notes"));
        assert_eq!(
            resolve(root.to_str().expect("utf8"), &roots).expect("root"),
            root
        );
        let code = |p: &Path| {
            resolve(p.to_str().expect("utf8"), &roots)
                .expect_err("rejected")
                .code
        };
        assert_eq!(code(&outside), ErrorCode::Forbidden);
        assert_eq!(
            code(&root.join("notes/../../outside")),
            ErrorCode::Forbidden
        );
        assert_eq!(code(&root.join("missing")), ErrorCode::BadRequest);
        assert_eq!(code(&outside.join("missing")), ErrorCode::Forbidden);
        assert_eq!(code(&root.join("notes/a.md")), ErrorCode::BadRequest);
        assert_eq!(code(Path::new("notes")), ErrorCode::BadRequest);
        // A sibling whose name merely starts with the root's.
        let sibling = PathBuf::from(format!("{}2", root.display()));
        std::fs::create_dir(&sibling).expect("mkdir");
        assert_eq!(code(&sibling), ErrorCode::Forbidden);
        assert_eq!(
            resolve("/", &[]).expect_err("no roots").code,
            ErrorCode::Forbidden
        );
    }

    #[test]
    fn symlinks_out_of_a_root_are_rejected() {
        let (_tmp, root, outside) = dirs();
        let roots = vec![root.clone()];
        symlink(&outside, root.join("escape")).expect("symlink");
        let err = resolve(root.join("escape").to_str().expect("utf8"), &roots).expect_err("escape");
        assert_eq!(err.code, ErrorCode::Forbidden);
        // A symlink to a file outside, and through a symlinked directory.
        symlink(outside.join("secret.txt"), root.join("notes/link.txt")).expect("symlink");
        assert!(matches!(
            open_inside(&root, Path::new("notes/link.txt")),
            Err(OpenError::Unsafe)
        ));
        assert!(matches!(
            open_inside(&root, Path::new("escape/secret.txt")),
            Err(OpenError::Unsafe)
        ));
        let (_, meta) = open_inside(&root, Path::new("notes/a.md")).expect("regular file");
        assert_eq!(meta.len(), 1);
    }

    #[test]
    fn roots_fill_in_the_user_and_drop_missing_ones() {
        let (_tmp, root, _) = dirs();
        let id = Uuid::new_v4();
        std::fs::create_dir(root.join(id.to_string())).expect("mkdir");
        std::fs::create_dir(root.join("ada@x.y")).expect("mkdir");
        let configured = vec![
            format!("{}/{{user_id}}", root.display()),
            format!("{}/{{email}}", root.display()),
            format!("{}/missing", root.display()),
            "relative/path".to_owned(),
        ];
        let roots = roots_for(&configured, id, "Ada@X.Y");
        assert_eq!(roots, [root.join(id.to_string()), root.join("ada@x.y")]);
        assert!(roots_for(&configured[1..2], id, "../..").is_empty());
    }
}
