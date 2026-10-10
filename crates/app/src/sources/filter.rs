//! Which files of a watched folder are considered.
//!
//! - hidden files and folders (name starting with `.`, e.g. `.obsidian/`, `.trash/`,
//!   `.git/`) are always skipped, as are Obsidian's `_resources`-style folders only
//!   when excluded by a glob;
//! - only file types Akasha can import (by extension, see `files::sniff`);
//! - include globs (none = everything) and exclude globs on the path relative to the
//!   folder, `/`-separated (`**/*.md`, `Archive/**`).

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::files::sniff;

/// Most globs per list, and longest glob.
pub const MAX_GLOBS: usize = 50;
pub const MAX_GLOB_LEN: usize = 200;

/// Compiled include/exclude rules of a source.
#[derive(Debug, Clone)]
pub struct Filter {
    include: Option<GlobSet>,
    exclude: GlobSet,
}

impl Filter {
    pub fn new(include: &[String], exclude: &[String]) -> Result<Self, String> {
        Ok(Self {
            include: if include.is_empty() {
                None
            } else {
                Some(build(include)?)
            },
            exclude: build(exclude)?,
        })
    }

    /// Whether to descend into a directory (relative path, `/`-separated).
    pub fn enter_dir(&self, rel: &str) -> bool {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        !name.starts_with('.')
            && !self.exclude.is_match(rel)
            && !self.exclude.is_match(format!("{rel}/"))
    }

    /// Whether to import a file (relative path, `/`-separated).
    pub fn accept_file(&self, rel: &str) -> bool {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        !name.starts_with('.')
            && sniff::known_extension(name)
            && !self.exclude.is_match(rel)
            && self.include.as_ref().is_none_or(|inc| inc.is_match(rel))
    }
}

/// Validate and normalise globs from a request (trimmed, no empties, deduplicated).
pub fn clean_globs(globs: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for glob in globs {
        let glob = glob.trim();
        if glob.is_empty() || out.iter().any(|g| g == glob) {
            continue;
        }
        if glob.len() > MAX_GLOB_LEN {
            return Err(format!("globs are at most {MAX_GLOB_LEN} characters"));
        }
        Glob::new(glob).map_err(|err| format!("invalid glob `{glob}`: {}", err.kind()))?;
        out.push(glob.to_owned());
    }
    if out.len() > MAX_GLOBS {
        return Err(format!("at most {MAX_GLOBS} globs"));
    }
    Ok(out)
}

fn build(globs: &[String]) -> Result<GlobSet, String> {
    let mut set = GlobSetBuilder::new();
    for glob in globs {
        // `literal_separator`: `*` stays within one folder, `**` crosses folders.
        let glob = globset::GlobBuilder::new(glob)
            .literal_separator(true)
            .build()
            .map_err(|err| format!("invalid glob `{glob}`: {}", err.kind()))?;
        set.add(glob);
    }
    set.build().map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(include: &[&str], exclude: &[&str]) -> Filter {
        let own = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        Filter::new(&own(include), &own(exclude)).expect("valid")
    }

    #[test]
    fn hidden_and_unsupported_files_are_skipped() {
        let f = filter(&[], &[]);
        assert!(f.accept_file("notes/a.md"));
        assert!(f.accept_file("Scan.PDF"));
        assert!(!f.accept_file("notes/.draft.md"));
        assert!(!f.accept_file("setup.exe"));
        assert!(!f.accept_file("README"));
        assert!(!f.enter_dir(".obsidian"));
        assert!(!f.enter_dir("notes/.trash"));
        assert!(f.enter_dir("notes"));
    }

    #[test]
    fn globs_include_and_exclude_relative_paths() {
        let f = filter(&["**/*.md", "*.pdf"], &["Archive/**", "**/draft-*"]);
        assert!(f.accept_file("a.md") && f.accept_file("x/y/a.md"));
        assert!(f.accept_file("top.pdf"));
        assert!(!f.accept_file("x/nested.pdf"), "* stays within a folder");
        assert!(!f.accept_file("Archive/old.md"));
        assert!(!f.accept_file("x/draft-1.md"));
        assert!(!f.enter_dir("Archive"));
        assert!(f.enter_dir("Projects"));
    }

    #[test]
    fn globs_are_validated() {
        let globs = vec![" a/** ".to_owned(), "a/**".to_owned(), String::new()];
        assert_eq!(clean_globs(&globs).expect("ok"), ["a/**"]);
        assert!(clean_globs(&["a/[".to_owned()]).is_err());
        assert!(clean_globs(&["x".repeat(MAX_GLOB_LEN + 1)]).is_err());
    }
}
