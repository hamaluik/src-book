//! Git tag metadata for the tags appendix section.
//!
//! Extracts tag information from git2, handling both annotated tags (with tagger
//! info and messages) and lightweight tags (simple refs to commits).

use super::Author;
use jiff::{
    tz::{Offset, TimeZone},
    Timestamp, Zoned,
};
use serde::{Deserialize, Serialize};

/// Controls how tags are sorted in the tags appendix.
#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TagOrder {
    /// Most recent tags first (by commit date)
    #[default]
    NewestFirst,
    /// Oldest tags first (by commit date)
    OldestFirst,
    /// Alphabetical by tag name
    Alphabetical,
    /// Reverse alphabetical by tag name
    AlphabeticalReverse,
}

impl TagOrder {
    /// All available tag order options for selection UI.
    pub fn all() -> &'static [TagOrder] {
        &[
            TagOrder::NewestFirst,
            TagOrder::OldestFirst,
            TagOrder::Alphabetical,
            TagOrder::AlphabeticalReverse,
        ]
    }
}

impl std::fmt::Display for TagOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagOrder::NewestFirst => write!(f, "Newest first"),
            TagOrder::OldestFirst => write!(f, "Oldest first"),
            TagOrder::Alphabetical => write!(f, "Alphabetical"),
            TagOrder::AlphabeticalReverse => write!(f, "Alphabetical (reverse)"),
        }
    }
}

/// A git tag pointing to a commit.
///
/// Tags can be either annotated (with tagger info, message, and their own timestamp)
/// or lightweight (simple refs pointing directly to commits). Annotated tags store
/// additional metadata in `tagger`, `message`, and `tag_date` fields.
pub struct Tag {
    /// Tag name without refs/tags/ prefix (e.g., "v1.0.0")
    pub name: String,
    /// Full SHA-1 hash of the commit this tag points to
    pub commit_hash: String,
    /// First line of the commit message
    pub commit_summary: Option<String>,
    /// Commit timestamp with timezone
    pub commit_date: Zoned,
    /// Whether this is an annotated tag (vs lightweight)
    pub is_annotated: bool,
    /// Tag message (annotated tags only)
    pub message: Option<String>,
    /// Who created the tag (annotated tags only)
    pub tagger: Option<Author>,
    /// When the tag was created (annotated tags only)
    pub tag_date: Option<Zoned>,
}

impl Tag {
    /// Creates a Tag from a git2 reference and repository.
    ///
    /// Handles both annotated and lightweight tags by attempting to peel to a tag
    /// object first, then falling back to treating the reference as a direct commit ref.
    pub fn from_ref(reference: &git2::Reference<'_>, repo: &git2::Repository) -> Option<Self> {
        // extract tag name from refs/tags/name
        let name = reference.shorthand()?.to_string();

        // peel to the target object
        let object = reference.peel(git2::ObjectType::Any).ok()?;

        // try to get annotated tag info
        let (is_annotated, message, tagger, tag_date, commit_oid) =
            if let Ok(tag) = object.peel_to_tag() {
                let tagger_author = tag.tagger().map(|sig| Author {
                    name: sig.name().map(ToString::to_string),
                    email: sig.email().map(ToString::to_string),
                    ..Default::default()
                });

                let tag_timestamp = tag.tagger().map(|sig| {
                    let time = sig.when();
                    let offset = Offset::from_seconds(time.offset_minutes() * 60)
                        .expect("can create offset from git time");
                    let tz = TimeZone::fixed(offset);
                    let ts = Timestamp::from_second(time.seconds())
                        .expect("can create timestamp from git time");
                    ts.to_zoned(tz)
                });

                let msg = tag.message().map(|m| m.trim().to_string());

                // annotated tag target is the commit
                let target_oid = tag.target_id();
                (true, msg, tagger_author, tag_timestamp, target_oid)
            } else {
                // lightweight tag - object is the commit directly
                (false, None, None, None, object.id())
            };

        // get commit info
        let commit = repo.find_commit(commit_oid).ok()?;
        let commit_hash = commit_oid.to_string();
        let commit_summary = commit.summary().map(ToString::to_string);

        let time = commit.time();
        let offset = Offset::from_seconds(time.offset_minutes() * 60)
            .expect("can create offset from git time");
        let tz = TimeZone::fixed(offset);
        let ts =
            Timestamp::from_second(time.seconds()).expect("can create timestamp from git time");
        let commit_date = ts.to_zoned(tz);

        Some(Tag {
            name,
            commit_hash,
            commit_summary,
            commit_date,
            is_annotated,
            message,
            tagger,
            tag_date,
        })
    }

    /// Sorts tags according to the specified order.
    pub fn sort_tags(tags: &mut [Tag], order: TagOrder) {
        match order {
            TagOrder::NewestFirst => {
                tags.sort_by(|a, b| b.commit_date.cmp(&a.commit_date));
            }
            TagOrder::OldestFirst => {
                tags.sort_by(|a, b| a.commit_date.cmp(&b.commit_date));
            }
            TagOrder::Alphabetical => {
                tags.sort_by(|a, b| a.name.cmp(&b.name));
            }
            TagOrder::AlphabeticalReverse => {
                tags.sort_by(|a, b| b.name.cmp(&a.name));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_order_display() {
        assert_eq!(TagOrder::NewestFirst.to_string(), "Newest first");
        assert_eq!(TagOrder::OldestFirst.to_string(), "Oldest first");
        assert_eq!(TagOrder::Alphabetical.to_string(), "Alphabetical");
        assert_eq!(
            TagOrder::AlphabeticalReverse.to_string(),
            "Alphabetical (reverse)"
        );
    }

    #[test]
    fn tag_order_all_returns_all_variants() {
        let all = TagOrder::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&TagOrder::NewestFirst));
        assert!(all.contains(&TagOrder::OldestFirst));
        assert!(all.contains(&TagOrder::Alphabetical));
        assert!(all.contains(&TagOrder::AlphabeticalReverse));
    }
}
