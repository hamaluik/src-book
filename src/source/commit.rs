use super::Author;
use chrono::prelude::*;
use serde::{Deserialize, Serialize};

/// Controls whether and how commit history appears in the generated book.
#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommitOrder {
    /// Most recent commits appear first (default)
    #[default]
    NewestFirst,
    /// Oldest commits appear first (chronological)
    OldestFirst,
    /// No commit history included
    Disabled,
}

impl CommitOrder {
    /// All available commit order options for selection UI.
    pub fn all() -> &'static [CommitOrder] {
        &[
            CommitOrder::NewestFirst,
            CommitOrder::OldestFirst,
            CommitOrder::Disabled,
        ]
    }
}

impl std::fmt::Display for CommitOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitOrder::NewestFirst => write!(f, "Newest first"),
            CommitOrder::OldestFirst => write!(f, "Oldest first"),
            CommitOrder::Disabled => write!(f, "Disabled"),
        }
    }
}

/// A git commit with author, message, and metadata.
///
/// Displayed in the commit history section of the generated book.
pub struct Commit {
    pub author: Author,
    /// First line of the commit message
    pub summary: Option<String>,
    /// Remaining lines of the commit message
    pub body: Option<String>,
    pub date: DateTime<FixedOffset>,
    /// Full SHA-1 hash
    pub hash: String,
}

impl From<&git2::Commit<'_>> for Commit {
    fn from(c: &git2::Commit) -> Self {
        let author = c.author();
        let author = Author {
            name: author.name().map(ToString::to_string),
            email: author.email().map(ToString::to_string),
            ..Default::default()
        };

        let summary = c.summary().map(ToString::to_string);
        let body = c.body().map(ToString::to_string);

        let time = c.time();
        let timezone = if time.offset_minutes() < 0 {
            FixedOffset::west(time.offset_minutes().abs() * 60)
        } else {
            FixedOffset::east(time.offset_minutes() * 60)
        };
        let date = timezone.timestamp(time.seconds(), 0);

        let hash = c.id().to_string();

        Commit {
            author,
            summary,
            body,
            date,
            hash,
        }
    }
}
