//! Request and response bodies for `/api/v1/collections`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use akasha_core::Error;
use akasha_db::collections::{Collection, CollectionRef};

pub const NAME_MAX: usize = 100;
pub const DESCRIPTION_MAX: usize = 2000;
/// Files per add/remove request.
pub const MAX_FILE_IDS: usize = 100;
/// Collections per user.
pub const MAX_COLLECTIONS: i64 = 200;

macro_rules! names {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $s:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
        #[serde(rename_all = "lowercase")]
        pub enum $name { $($variant),* }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $s),* }
            }

            /// The stored name; unknown names fall back to the first variant.
            pub fn from_db(s: &str) -> Self {
                match s { $($s => Self::$variant,)* _ => names!(@first $($variant)*) }
            }
        }
    };
    (@first $first:ident $($rest:ident)*) => { Self::$first };
}

names! {
    /// A collection's colour, from the UI's muted palette (never a raw colour).
    CollectionColor {
        Sage = "sage", Sky = "sky", Ochre = "ochre", Clay = "clay", Plum = "plum",
        Slate = "slate",
    }
}

names! {
    /// A collection's icon.
    CollectionIcon {
        Folder = "folder", Book = "book", Briefcase = "briefcase", Flask = "flask",
        Heart = "heart", Star = "star", Archive = "archive", Receipt = "receipt",
        Plane = "plane", Home = "home", Graduation = "graduation", Code = "code",
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CollectionResponse {
    pub id: Uuid,
    pub name: String,
    /// Plain text, may be empty.
    pub description: String,
    pub color: CollectionColor,
    pub icon: CollectionIcon,
    pub file_count: i64,
    pub created_at: DateTime<Utc>,
    /// Last change, including files added or removed.
    pub updated_at: DateTime<Utc>,
}

impl From<Collection> for CollectionResponse {
    fn from(c: Collection) -> Self {
        Self {
            id: c.id,
            name: c.name,
            description: c.description,
            color: CollectionColor::from_db(&c.color),
            icon: CollectionIcon::from_db(&c.icon),
            file_count: c.file_count,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// A collection a file belongs to.
#[derive(Debug, Serialize, ToSchema)]
pub struct CollectionSummary {
    pub id: Uuid,
    pub name: String,
    pub color: CollectionColor,
    pub icon: CollectionIcon,
}

impl From<CollectionRef> for CollectionSummary {
    fn from(c: CollectionRef) -> Self {
        Self {
            id: c.id,
            name: c.name,
            color: CollectionColor::from_db(&c.color),
            icon: CollectionIcon::from_db(&c.icon),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CollectionList {
    /// By name.
    pub items: Vec<CollectionResponse>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateCollection {
    /// 1-100 characters, unique among your collections (ignoring case).
    pub name: String,
    /// Up to 2000 characters.
    pub description: Option<String>,
    pub color: Option<CollectionColor>,
    pub icon: Option<CollectionIcon>,
    /// Files to put in it right away (up to 100; ids that are not yours are skipped).
    pub file_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateCollection {
    pub name: Option<String>,
    /// `""` clears it.
    pub description: Option<String>,
    pub color: Option<CollectionColor>,
    pub icon: Option<CollectionIcon>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CollectionFilesRequest {
    /// Up to 100 file ids. Ids that are not your files are skipped.
    pub file_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CollectionFilesChanged {
    /// The files actually added (or removed); already-present or unknown ids are left out.
    pub file_ids: Vec<Uuid>,
    /// The collection afterwards.
    pub collection: CollectionResponse,
}

pub fn clean_name(raw: &str) -> Result<String, Error> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err(Error::bad_request(format!(
            "name must be 1-{NAME_MAX} characters"
        )));
    }
    Ok(name)
}

pub fn clean_description(raw: &str) -> Result<String, Error> {
    let text = raw.trim();
    if text.chars().count() > DESCRIPTION_MAX {
        return Err(Error::bad_request(format!(
            "description must be at most {DESCRIPTION_MAX} characters"
        )));
    }
    Ok(text.to_owned())
}

pub fn check_ids(ids: &[Uuid]) -> Result<(), Error> {
    if ids.len() > MAX_FILE_IDS {
        return Err(Error::bad_request(format!(
            "at most {MAX_FILE_IDS} file ids per request"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_palette() {
        assert_eq!(
            clean_name("  Trips   2026 ").ok().as_deref(),
            Some("Trips 2026")
        );
        assert!(clean_name("   ").is_err());
        assert!(clean_name(&"x".repeat(101)).is_err());
        assert!(clean_description(&"x".repeat(2001)).is_err());
        assert_eq!(CollectionColor::from_db("plum"), CollectionColor::Plum);
        assert_eq!(CollectionColor::from_db("#ff0000"), CollectionColor::Sage);
        assert_eq!(CollectionIcon::from_db("code").as_str(), "code");
    }
}
