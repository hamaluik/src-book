use super::Source;
use anyhow::{anyhow, Context, Result};
use derive_builder::Builder;
use std::path::{Path, PathBuf};

#[derive(Builder, Debug)]
#[builder(setter(into), build_fn(skip, error = "anyhow::Error"))]
pub struct Repository {
    root: PathBuf,
    title: String,
    #[builder(setter(each(name = "author", into)))]
    authors: Vec<String>,
    #[builder(setter(into, strip_option), default)] // allow setting a string, defaulting to none
    license: Option<String>,
    /// All of the source files, taken from the builder as well as walking the repository
    source_files: Vec<PathBuf>,
}

impl RepositoryBuilder {
    /// Build the repository
    /// allow is a list of callbacks, where the file will be included if ANY of the callbacks
    /// return true
    /// block is a list of callbacks, where the file will NOT be included if ANY of the callbacks
    /// return true
    /// if allow is empty and block is empty, all files (following .gitignore) (which are text
    /// files) will be included
    pub fn build(
        &mut self,
        allow: Vec<fn(&Path) -> bool>,
        block: Vec<fn(&Path) -> bool>,
    ) -> Result<Repository> {
        // first make sure we have all the fields we need
        if self.root.is_none() {
            return Err(anyhow!("Reposity root not provided!"));
        }
        let root = self.root.take().unwrap();

        // make sure the root is a path
        if !root.is_dir() {
            return Err(anyhow!(
                "Repository path {} isn't a directory!",
                root.display()
            ));
        }

        // get the repository
        let repo = git2::Repository::open(&root).with_context(|| {
            format!(
                "Failed to open path {} as a git repository!",
                root.display()
            )
        })?;

        // if the title isn't set, default to the name of the folder of 'root'
        let title = match self.title.take() {
            Some(t) => t,
            None => {
                let canonical_root = match std::fs::canonicalize(&root) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(anyhow!("Failed to canonicalize {}: {e:#}", root.display()));
                    }
                };

                match canonical_root.file_name() {
                    Some(name) => name.to_string_lossy().to_string(),
                    None => {
                        return Err(anyhow!(
                            "Repository {} doesn't have a name?",
                            canonical_root.display()
                        ))
                    }
                }
            }
        };

        use std::collections::HashSet;
        let authors = self.authors.take().unwrap_or_default();

        // add authors from authors files in the folder
        let mut authors: HashSet<String> = HashSet::from_iter(authors.into_iter());
        let mut add_authors_file = |path: PathBuf| {
            if path.exists() && path.is_file() {
                if let Ok(contents) = std::fs::read_to_string(path) {
                    for author in contents.lines() {
                        authors.insert(author.to_string());
                    }
                }
            }
        };
        add_authors_file(root.join("AUTHORS"));
        add_authors_file(root.join("AUTHORS.txt"));
        add_authors_file(root.join("AUTHORS.md"));

        // add authors from git commit history
        {
            let mut walk = repo
                .revwalk()
                .with_context(|| "Failed to start walking the repository")?;
            let head = repo
                .head()
                .with_context(|| "Failed to get the repository HEAD")?;
            let head_oid = head
                .resolve()
                .with_context(|| "Failed to resolve HEAD reference")?
                .target()
                .ok_or(anyhow!("HEAD doesn't have an OID reference"))?;
            walk.push(head_oid)
                .with_context(|| "Failed to push head OID to revwalk")?;

            for oid in walk.into_iter() {
                let oid = oid.with_context(|| "Failed to get OID while walking repository")?;
                let commit = repo
                    .find_commit(oid)
                    .with_context(|| format!("Failed to find commit for OID {}", oid))?;
                let author = commit.author();

                let author = match (author.name(), author.email()) {
                    (Some(name), Some(email)) => Some(format!("{name} <{email}>")),
                    (Some(name), None) => Some(name.to_string()),
                    (None, Some(email)) => Some(email.to_string()),
                    _ => None,
                };
                if let Some(author) = author {
                    authors.insert(author);
                }
            }
        }

        // convert our authors back into a vector and sort it
        let mut authors: Vec<String> = authors.into_iter().collect();
        authors.sort();
        // make it immutable again
        let authors = authors;

        let license = self.license.take().flatten();

        let mut source_files = {
            use ignore::Walk;

            let mut source_files = Vec::default();
            'walk: for entry in Walk::new(&root) {
                let entry = entry.with_context(|| "Failed to walk repository directory")?;

                if allow.is_empty() && block.is_empty() {
                    source_files.push(entry.into_path());
                    continue 'walk;
                }

                let allowed = allow.iter().any(|&allow| allow(entry.path()));
                let blocked = block.iter().any(|&block| block(entry.path()));

                if allowed && !blocked {
                    source_files.push(entry.into_path());
                }
            }

            source_files
        };
        // add on any files we explicitely included
        if let Some(sf) = self.source_files.take() {
            source_files.extend(sf.into_iter());
        }
        // sort it so there is at least some semblance of order
        source_files.sort();
        // make it immutable again
        let source_files = source_files;

        Ok(Repository {
            root,
            title,
            authors,
            license,
            source_files,
        })
    }
}

impl Source for Repository {
    fn title(&self) -> &str {
        self.title.as_str()
    }

    fn authors(&self) -> Vec<&str> {
        self.authors.iter().map(String::as_str).collect()
    }

    fn license(&self) -> Option<String> {
        self.license.clone()
    }

    fn list_source_files(&self) -> Vec<PathBuf> {
        self.source_files.clone()
    }

    fn source_file(&self, path: &Path) -> Result<String> {
        let repo_path = self.root.join(path);
        std::fs::read_to_string(repo_path)
            .with_context(|| format!("Failed to open file '{}' for reading", path.display()))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn repository_builder_works() {
        let repo = RepositoryBuilder::default()
            .root(".")
            //.title("src-book")
            //.author("Kenton Hamaluik <kenton@hamaluik.ca>")
            .license("Apache-2.0")
            .build(
                vec![|p: &Path| match p.extension() {
                    Some(ext) => ext.to_str() == Some("rs"),
                    None => false,
                }],
                Vec::default(),
            )
            .expect("can build repository");

        assert_eq!(repo.root, PathBuf::from("."));
        assert_eq!(repo.title, "src-book".to_string());
        assert_eq!(repo.authors.len(), 1);
        assert_eq!(
            repo.authors[0],
            "Kenton Hamaluik <kenton@hamaluik.ca>".to_string()
        );
        assert_eq!(repo.license, Some("Apache-2.0".to_string()));
        assert_ne!(repo.source_files.len(), 0);
        for path in repo.list_source_files() {
            eprintln!("File: {}", path.display());
        }
    }
}
