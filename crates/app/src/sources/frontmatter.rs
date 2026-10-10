//! Tags from Markdown front matter (Obsidian and most static-site tools):
//!
//! ```text
//! ---
//! tags: [project, idea]        # or `tags: project, idea`, or `tags: project`
//! tags:                        # or a YAML list
//!   - project
//!   - "reading list"
//! ---
//! ```
//!
//! A deliberately small parser (no YAML dependency): only the `tags` (or `tag`) key
//! of a front-matter block at the very start of the file is read. Leading `#` is
//! dropped and nested tags keep their `/` (`area/work`).

/// How much of a file is looked at.
pub const MAX_HEAD: usize = 16 * 1024;
/// Most tags taken from one file.
const MAX_TAGS: usize = 20;

/// Tags declared in the front matter of `text`, normalised like user tags.
pub fn tags(text: &str) -> Vec<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let mut in_list = false;
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "---" || trimmed == "..." {
            break;
        }
        if in_list {
            if let Some(item) = trimmed.trim_start().strip_prefix("- ") {
                push(&mut out, item);
                continue;
            }
            if trimmed.trim_start() == "-" || trimmed.trim().is_empty() {
                continue;
            }
            in_list = false;
        }
        // Only top-level keys.
        if line.starts_with([' ', '\t']) {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        if !matches!(key.trim(), "tags" | "tag" | "Tags") {
            continue;
        }
        // ` #` starts a YAML comment.
        let value = value.split(" #").next().unwrap_or(value).trim();
        if value.is_empty() {
            in_list = true;
        } else {
            let inner = value
                .strip_prefix('[')
                .and_then(|v| v.strip_suffix(']'))
                .unwrap_or(value);
            for item in inner.split(',') {
                push(&mut out, item);
            }
        }
    }
    out
}

fn push(out: &mut Vec<String>, raw: &str) {
    if out.len() >= MAX_TAGS {
        return;
    }
    let raw = raw.split(" #").next().unwrap_or(raw).trim();
    let raw = raw.trim_end_matches(',').trim();
    let unquoted = raw
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .unwrap_or(raw);
    let tag = unquoted
        .trim()
        .trim_start_matches('#')
        .trim()
        .to_lowercase();
    if tag.is_empty() || tag.chars().count() > 50 || out.contains(&tag) {
        return;
    }
    out.push(tag);
}

#[cfg(test)]
mod tests {
    use super::tags;

    #[test]
    fn reads_inline_flow_and_block_lists() {
        assert_eq!(
            tags("---\ntags: [Project, \"idea\"]\n---\nbody"),
            ["project", "idea"]
        );
        assert_eq!(tags("---\ntitle: x\ntags: a, b # c\n---\n"), ["a", "b"]);
        assert_eq!(tags("---\ntags: \"#x\"\n---\n"), ["x"]);
        assert_eq!(tags("---\ntag: area/work\n---\n"), ["area/work"]);
        assert_eq!(
            tags(
                "---\naliases:\n  - nope\ntags:\n  - one\n  - 'reading list'\n  -\nnext: 1\n---\n"
            ),
            ["one", "reading list"]
        );
    }

    #[test]
    fn needs_front_matter_at_the_start() {
        assert!(tags("# Title\n---\ntags: [x]\n---\n").is_empty());
        assert!(tags("---\ntitle: no tags\n---\ntags: [x]\n").is_empty());
        assert!(tags("").is_empty());
        assert_eq!(tags("\u{feff}---\r\ntags: [x]\r\n---\r\n"), ["x"]);
    }
}
