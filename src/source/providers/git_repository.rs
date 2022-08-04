use super::SourceProvider;
use crate::source::{Author, AuthorBuilder, Source, SourceFile};
use anyhow::{anyhow, Context, Result};
use ignore::Walk;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct GitRepository {
    root: PathBuf,
    title: String,
    authors: Vec<Author>,
    source_files: Vec<SourceFile>,
}

impl GitRepository {
    /// Load a git repository starting from the root folder
    /// allow is a list of callbacks, where the file will be included if ANY of the callbacks
    /// return true
    /// block is a list of callbacks, where the file will NOT be included if ANY of the callbacks
    /// return true
    /// if allow is empty and block is empty, all files (following .gitignore) (which are text
    /// files) will be included
    pub fn load<P: Into<PathBuf>>(
        root: P,
        allow: Vec<fn(&Path) -> bool>,
        block: Vec<fn(&Path) -> bool>,
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

        // if the title isn't set, default to the name of the folder of 'root'
        let title = {
            match root.file_name() {
                Some(name) => name.to_string_lossy().to_string(),
                None => {
                    return Err(anyhow!(
                        "Repository {} doesn't have a name?",
                        root.display()
                    ))
                }
            }
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

            'walk: for entry in Walk::new(&root) {
                let entry = entry.with_context(|| "Failed to walk repository directory")?;

                if allow.is_empty() && block.is_empty() {
                    push_path(entry.into_path())?;
                    continue 'walk;
                }

                let allowed = allow.iter().any(|&allow| allow(entry.path()));
                let blocked = block.iter().any(|&block| block(entry.path()));

                if allowed && !blocked {
                    push_path(entry.into_path())?;
                }
            }

            source_files.into_iter().map(Into::into).collect()
        };

        Ok(GitRepository {
            root,
            title,
            authors,
            source_files,
        })
    }
}

impl SourceProvider for GitRepository {
    fn apply(&self, source: &mut Source) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod test {
    use super::GitRepository;

    #[test]
    fn repository_adds_files() {
        use std::ffi::OsStr;
        let repo = GitRepository::load(
            ".",
            vec![|p| p.extension().map(OsStr::to_str).flatten() == Some("rs")],
            vec![],
        )
        .expect("can load repository");
        assert_ne!(repo.source_files.len(), 0);
    }
}
