//! The benchmark: queries with the files a good search returns first
//! (`eval/queries.json`), and the corpus they refer to (`eval/corpus/`).

use std::{collections::HashSet, path::Path};

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Suite {
    pub version: u32,
    pub queries: Vec<Query>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Query {
    pub id: String,
    /// Report group (keyword, paraphrase, filename, multi, ...).
    pub kind: String,
    pub query: String,
    /// Corpus file names that answer the query.
    pub relevant: Vec<String>,
}

impl Suite {
    /// Load and check a suite against the corpus file names.
    pub fn load(path: &Path, corpus: &[String]) -> anyhow::Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let suite: Self =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        suite.check(corpus)?;
        Ok(suite)
    }

    fn check(&self, corpus: &[String]) -> anyhow::Result<()> {
        if self.version != 1 {
            bail!("unsupported suite version {}", self.version);
        }
        let mut ids = HashSet::new();
        for q in &self.queries {
            if !ids.insert(&q.id) {
                bail!("duplicate query id `{}`", q.id);
            }
            if q.relevant.is_empty() {
                bail!("query `{}` lists no relevant files", q.id);
            }
            if let Some(missing) = q.relevant.iter().find(|r| !corpus.contains(r)) {
                bail!("query `{}`: `{missing}` is not in the corpus", q.id);
            }
        }
        Ok(())
    }
}

/// The corpus files (sorted by name, so ingestion order is reproducible).
pub fn corpus_files(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let name = entry.file_name().into_string().map_err(|n| {
                anyhow::anyhow!("corpus file name is not UTF-8: {}", n.to_string_lossy())
            })?;
            names.push(name);
        }
    }
    names.sort();
    if names.is_empty() {
        bail!("no corpus files in {}", dir.display());
    }
    Ok(names)
}
