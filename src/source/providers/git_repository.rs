use crate::source::{Author, AuthorBuilder, Source};
use anyhow::{anyhow, Context, Result};
use globset::GlobMatcher;
use ignore::Walk;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct GitRepository {
    pub _root: PathBuf,
    pub authors: Vec<Author>,
    pub source_files: Vec<PathBuf>,
}

impl GitRepository {
    /// Load a git repository starting from the root folder
    pub fn load<P: Into<PathBuf>>(root: P, block: Vec<GlobMatcher>) -> Result<GitRepository> {
        let root: PathBuf = root.into();

        // make sure the root is a path
        if !root.is_dir() {
            return Err(anyhow!(
                "Repository path {} isn't a directory!",
                root.display()
            ));
        }

        let root = match std::fs::canonicalize(&root) {
            Ok(p) => p,
            Err(e) => {
                return Err(anyhow!("Failed to canonicalize {}: {e:#}", root.display()));
            }
        };

        // get the repository
        let repo = git2::Repository::open(&root).with_context(|| {
            format!(
                "Failed to open path {} as a git repository!",
                root.display()
            )
        })?;

        // load the authors from commits
        let authors = {
            // count the number of commits per author
            let mut authors: HashMap<(Option<String>, Option<String>), usize> = HashMap::default();

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

                let author = (
                    author.name().map(ToString::to_string),
                    author.email().map(ToString::to_string),
                );
                *(authors.entry(author).or_insert(0)) += 1;
            }

            let authors: Result<Vec<Author>> = authors
                .into_iter()
                .map(|((name, email), count)| {
                    let mut ab = AuthorBuilder::default();
                    ab.prominence(count);
                    if let Some(name) = name {
                        ab.name(name);
                    }
                    if let Some(email) = email {
                        ab.email(email);
                    }
                    ab.build().with_context(|| "Failed to build author")
                })
                .collect();
            authors?
        };

        let source_files = {
            let mut source_files: Vec<PathBuf> = Vec::default();

            let mut push_path = |path: PathBuf| -> Result<()> {
                let path = path
                    .canonicalize()
                    .with_context(|| format!("Failed to canonicalize path {}", path.display()))?;
                let path = path.strip_prefix(&root).with_context(|| {
                    format!(
                        "Failed to remove root {} from path {}",
                        root.display(),
                        path.display()
                    )
                })?;

                if path.is_file() {
                    source_files.push(path.to_path_buf());
                }
                Ok(())
            };

            for entry in Walk::new(&root) {
                let entry = entry.with_context(|| "Failed to walk repository directory")?;

                let blocked = block.iter().any(|glob| glob.is_match(entry.path()));
                if !blocked {
                    push_path(entry.into_path())?;
                }
            }

            source_files
        };

        Ok(GitRepository {
            _root: root,
            authors,
            source_files,
        })
    }
}

impl Source {
    pub fn commits(&self) -> Result<Vec<crate::source::Commit>> {
        // get the repository
        let repo = git2::Repository::open(&self.repository).with_context(|| {
            format!(
                "Failed to open path {} as a git repository!",
                self.repository.display()
            )
        })?;

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

        let mut commits: Vec<crate::source::Commit> = Vec::default();

        for oid in walk.into_iter() {
            let oid = oid.with_context(|| "Failed to get OID while walking repository")?;
            let commit = repo
                .find_commit(oid)
                .with_context(|| format!("Failed to find commit for OID {}", oid))?;

            let commit = crate::source::Commit::from(&commit);
            commits.push(commit);
        }

        Ok(commits)
    }
}

#[cfg(test)]
mod test {
    use globset::Glob;

    use super::GitRepository;

    #[test]
    fn repository_adds_files() {
        let repo = GitRepository::load(".", vec![Glob::new("*.lock").unwrap().compile_matcher()])
            .expect("can load repository");
        assert_ne!(repo.source_files.len(), 0);
    }
}
