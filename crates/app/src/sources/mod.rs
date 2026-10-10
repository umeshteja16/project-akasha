//! Watched folders ("sources"): server directories, under the admin's
//! `AKASHA_WATCH_ROOTS`, whose files are imported for a user and kept in sync.
//!
//! - [`paths`]: the allow-list, canonicalisation and safe opening (no symlink escapes)
//! - [`filter`]: hidden files, supported types, include/exclude globs
//! - [`walk`]: listing a folder; [`scan`]: the `scan_source` job; [`import`]: one file
//!   through the upload pipeline; [`frontmatter`]: Obsidian tags
//! - [`watch`]: filesystem events (debounced) that queue scans within seconds

pub mod filter;
pub mod frontmatter;
pub mod import;
pub mod paths;
pub mod scan;
pub mod walk;
pub mod watch;
