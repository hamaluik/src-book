use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UserName {
    pub service: String,
    pub user_name: String,
}

impl UserName {
    #[allow(unused)]
    pub fn new<S1: ToString, S2: ToString>(service: S1, user_name: S2) -> UserName {
        UserName {
            service: service.to_string(),
            user_name: user_name.to_string(),
        }
    }
}

impl fmt::Display for UserName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.service, self.user_name)
    }
}

#[derive(Builder, Debug, Eq, Default, Clone, Serialize, Deserialize)]
#[builder(setter(into))]
pub struct Author {
    #[builder(setter(each(name = "user_name", into)), default)]
    pub user_names: Vec<UserName>,
    #[builder(setter(into, strip_option), default)]
    pub name: Option<String>,
    #[builder(setter(into, strip_option), default)]
    pub email: Option<String>,
    #[builder(setter(into, strip_option), default)]
    pub identifier: Option<String>,
    #[builder(setter(into, strip_option), default)]
    pub role: Option<String>,
    #[builder(default)]
    pub prominence: usize,
}

impl fmt::Display for Author {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::default();

        match (&self.name, &self.email) {
            (Some(name), Some(email)) => parts.push(format!("{name} <{email}>")),
            (Some(name), None) => parts.push(name.clone()),
            (None, Some(email)) => parts.push(email.clone()),
            _ => {}
        }

        if let Some(identifier) = &self.identifier {
            parts.push(identifier.clone());
        }

        write!(f, "{}", parts.join(" "))?;

        if let Some(role) = &self.role {
            if !parts.is_empty() {
                write!(f, ", {role}")?;
            } else {
                write!(f, "{role}")?;
            }
        }

        if !self.user_names.is_empty() {
            let user_names = self
                .user_names
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join(", ");

            if !parts.is_empty() || self.role.is_some() {
                write!(f, " ({user_names})")?;
            } else {
                write!(f, "{user_names}")?;
            }
        }

        Ok(())
    }
}

impl PartialEq for Author {
    fn eq(&self, other: &Author) -> bool {
        if self.email.is_some() && self.email == other.email {
            return true;
        }

        if self.identifier.is_some() && self.identifier == other.identifier {
            return true;
        }

        false
    }
}

impl PartialOrd for Author {
    fn partial_cmp(&self, other: &Author) -> Option<Ordering> {
        match self.prominence.partial_cmp(&other.prominence) {
            Some(Ordering::Equal) => match (self.role.is_some(), other.role.is_some()) {
                (true, false) => Some(Ordering::Greater),
                (false, true) => Some(Ordering::Less),
                _ => self.to_string().partial_cmp(&other.to_string()),
            },
            ordering => ordering,
        }
    }
}

impl Ord for Author {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or_else(|| Ordering::Equal)
    }
}

impl<S: Into<String>> From<S> for Author {
    fn from(s: S) -> Self {
        Author {
            name: None,
            email: None,
            identifier: Some(s.into()),
            role: None,
            prominence: 0,
            user_names: Vec::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn can_create_author_with_builder_pattern() {
        let author = AuthorBuilder::default()
            .name("Kenton Hamaluik")
            .email("kenton@hamaluik.ca")
            .build()
            .expect("can build author");

        assert_eq!(
            author.to_string(),
            "Kenton Hamaluik <kenton@hamaluik.ca>".to_string()
        );
    }

    #[test]
    fn author_gets_formatted_decently() {
        let author = Author {
            name: Some("Kenton Hamaluik".to_string()),
            email: None,
            identifier: None,
            role: Some("Associate Chimpanzee".to_string()),
            prominence: 42,
            user_names: vec![
                UserName::new("GitHub", "hamaluik"),
                UserName::new("sourcehut", "hamaluik"),
            ],
        };

        assert_eq!(
            author.to_string(),
            "Kenton Hamaluik, Associate Chimpanzee (GitHub: hamaluik, sourcehut: hamaluik)"
                .to_string()
        );
    }

    #[test]
    fn authors_get_sorted_properly() {
        let mut authors = vec![
            AuthorBuilder::default()
                .name("A")
                .prominence(42usize)
                .build()
                .unwrap(),
            AuthorBuilder::default()
                .name("B")
                .prominence(5usize)
                .build()
                .unwrap(),
        ];
        authors.sort();
        assert_eq!(authors[0].name, Some("B".to_string()));
        assert_eq!(authors[1].name, Some("A".to_string()));

        let mut authors = vec![
            AuthorBuilder::default()
                .name("A")
                .role("Commander in Chimp")
                .build()
                .unwrap(),
            AuthorBuilder::default().name("B").build().unwrap(),
        ];
        authors.sort();
        assert_eq!(authors[0].name, Some("B".to_string()));
        assert_eq!(authors[1].name, Some("A".to_string()));

        let mut authors = vec![
            AuthorBuilder::default().name("A").build().unwrap(),
            AuthorBuilder::default().name("B").build().unwrap(),
        ];
        authors.sort();
        assert_eq!(authors[0].name, Some("A".to_string()));
        assert_eq!(authors[1].name, Some("B".to_string()));
    }
}
