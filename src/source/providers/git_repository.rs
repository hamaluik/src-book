//! Git repository file discovery and author extraction.
//!
//! Walks a git repository to collect source files (respecting `.gitignore`) and extracts
//! author information from commit history. Authors are ranked by commit count to determine
//! prominence ordering on the title page.
//!
//! Supports optional submodule exclusion to prevent external dependency code from being
//! included in the generated book. Submodules are detected via `git2::Repository::submodules()`.

use crate::source::{Author, AuthorBuilder};
use anyhow::{anyhow, Context, Result};
use globset::GlobMatcher;
use ignore::Walk;
use std::collections::HashMap;
use std::path::PathBuf;

/// A loaded git repository with extracted files and authors.
#[derive(Debug)]
pub struct GitRepository {
    pub _root: PathBuf,
    pub authors: Vec<Author>,
    pub source_files: Vec<PathBuf>,
}

impl GitRepository {
    /// Load a git repository starting from the root folder.
    ///
    /// When `exclude_submodules` is true, files within git submodule directories are
    /// excluded from the source file list. This prevents external dependency code from
    /// being included in the generated book.
    pub fn load<P: Into<PathBuf>>(
        root: P,
        block: Vec<GlobMatcher>,
        exclude_submodules: bool,
    ) -> Result<GitRepository> {
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

        // collect submodule paths if exclusion is enabled
        let submodule_paths: Vec<PathBuf> = if exclude_submodules {
            repo.submodules()
                .unwrap_or_default()
                .iter()
                .map(|sm| PathBuf::from(sm.path()))
                .collect()
        } else {
            Vec::new()
        };

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

            for oid in walk {
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

                // match against relative path so globs like "Cargo.lock" work
                let rel_path = entry.path().strip_prefix(&root).unwrap_or(entry.path());

                // skip if path is inside a submodule directory
                let in_submodule = submodule_paths
                    .iter()
                    .any(|sm_path| rel_path.starts_with(sm_path));
                if in_submodule {
                    continue;
                }

                let blocked = block.iter().any(|glob| glob.is_match(rel_path));
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

impl GitRepository {
    /// Returns the paths of all submodules in the repository, or an empty vec if none.
    pub fn submodule_paths<P: Into<PathBuf>>(root: P) -> Vec<PathBuf> {
        let root: PathBuf = root.into();
        let Ok(repo) = git2::Repository::open(&root) else {
            return Vec::new();
        };

        let submodules = repo.submodules().unwrap_or_default();
        submodules
            .iter()
            .map(|sm| PathBuf::from(sm.path()))
            .collect()
    }
}

#[cfg(test)]
mod test {
    use globset::Glob;

    use super::GitRepository;

    #[test]
    fn repository_adds_files() {
        let repo = GitRepository::load(
            ".",
            vec![Glob::new("*.lock").unwrap().compile_matcher()],
            true,
        )
        .expect("can load repository");
        assert_ne!(repo.source_files.len(), 0);
    }
}
