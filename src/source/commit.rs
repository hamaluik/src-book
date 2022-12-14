use super::Author;
use chrono::prelude::*;

pub struct Commit {
    pub author: Author,
    pub summary: Option<String>,
    pub body: Option<String>,
    pub date: DateTime<FixedOffset>,
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
