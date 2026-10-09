//! Reading the model's answer defensively: models wrap JSON in code fences,
//! add prose around it, return tags as one string, shout, or repeat
//! themselves. Anything unusable yields `None` rather than junk in the library.

use serde_json::Value;

/// Most suggested tags kept per file.
pub const MAX_AUTO_TAGS: usize = 5;
/// Longest suggested tag, in characters.
const MAX_TAG_CHARS: usize = 40;
const MAX_TAG_WORDS: usize = 4;
/// Longest summary kept, in characters (the column allows 1000).
const MAX_SUMMARY_CHARS: usize = 600;
const MAX_SENTENCES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrichment {
    pub summary: String,
    pub tags: Vec<String>,
}

/// The summary and tags in a model answer, or `None` if it holds neither.
pub fn parse(answer: &str) -> Option<Enrichment> {
    let object = json_object(answer)?;
    let summary = object
        .get("summary")
        .and_then(Value::as_str)
        .map(clean_summary)
        .unwrap_or_default();
    let raw_tags: Vec<String> = match object.get("tags") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(s)) => s.split([',', ';']).map(str::to_owned).collect(),
        _ => Vec::new(),
    };
    let tags = normalize_auto_tags(&raw_tags);
    if summary.is_empty() && tags.is_empty() {
        return None;
    }
    Some(Enrichment { summary, tags })
}

/// The first JSON object in `text` (code fences and surrounding prose allowed).
fn json_object(text: &str) -> Option<serde_json::Map<String, Value>> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    match serde_json::from_str(&text[start..=end]) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// Lowercased, trimmed (of whitespace, `#` and punctuation), de-duplicated
/// tags of sensible length, at most [`MAX_AUTO_TAGS`]; others are dropped
/// rather than rejected.
pub fn normalize_auto_tags(raw: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in raw {
        let tag = tag
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let tag = tag
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '+')
            .to_owned();
        let words = tag.split(' ').count();
        if tag.is_empty()
            || tag.chars().count() > MAX_TAG_CHARS
            || words > MAX_TAG_WORDS
            || tag.chars().any(char::is_control)
            || tags.contains(&tag)
        {
            continue;
        }
        tags.push(tag);
        if tags.len() == MAX_AUTO_TAGS {
            break;
        }
    }
    tags
}

/// One paragraph, at most [`MAX_SENTENCES`] sentences and
/// [`MAX_SUMMARY_CHARS`] characters (cut at a word, with an ellipsis).
fn clean_summary(raw: &str) -> String {
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut end = text.len();
    let mut sentences = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | '!' | '?')
            && text[i + c.len_utf8()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
        {
            sentences += 1;
            if sentences == MAX_SENTENCES {
                end = i + c.len_utf8();
                break;
            }
        }
    }
    let text = &text[..end];
    if text.chars().count() <= MAX_SUMMARY_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(MAX_SUMMARY_CHARS).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', ';', ':', ' ']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_fenced_json_parse() {
        let plain = parse(r#"{"summary": "A guide.", "tags": ["Rust", "async"]}"#);
        assert_eq!(
            plain,
            Some(Enrichment {
                summary: "A guide.".into(),
                tags: vec!["rust".into(), "async".into()],
            })
        );
        let fenced = "Sure! Here it is:\n```json\n{\"summary\": \"S.\", \"tags\": \"a, b\"}\n```";
        let parsed = parse(fenced).expect("fenced");
        assert_eq!(parsed.tags, vec!["a", "b"]);
    }

    #[test]
    fn junk_is_rejected() {
        for bad in [
            "",
            "no json here",
            "{not json}",
            "[1, 2]",
            r#"{"summary": "", "tags": []}"#,
            r#"{"other": 1}"#,
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn tags_are_normalised_deduped_and_capped() {
        let raw: Vec<String> = [
            " #Machine   Learning ",
            "machine learning",
            "C++",
            "",
            "a tag that is far far too long to be useful",
            "one two three four five",
            "x\u{7}y",
            "Neural Networks.",
            "b",
            "c",
            "d",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        assert_eq!(
            normalize_auto_tags(&raw),
            vec!["machine learning", "c++", "neural networks", "b", "c"]
        );
    }

    #[test]
    fn summaries_keep_three_sentences_and_bounded_length() {
        assert_eq!(
            clean_summary("One.  Two!\nThree? Four. Five."),
            "One. Two! Three?"
        );
        assert_eq!(
            clean_summary("Version 1.5 is out. Yes."),
            "Version 1.5 is out. Yes."
        );
        let long = "word ".repeat(400);
        let cut = clean_summary(&long);
        assert!(cut.chars().count() <= MAX_SUMMARY_CHARS + 1);
        assert!(cut.ends_with("word…"));
    }
}
