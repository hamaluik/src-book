use std::cmp::Ordering;
use std::path::{Path, PathBuf};

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum SourceFile {
    /// The source file is a file on the local hard drive
    Path(PathBuf),

    /// The source file is represented a string with an extension / token type
    Contents {
        contents: String,
        path: PathBuf,
        type_token: Option<String>,
    },
}

impl<T: Into<PathBuf>> From<T> for SourceFile {
    fn from(path: T) -> Self {
        SourceFile::Path(path.into())
    }
}

impl SourceFile {
    pub fn from_string<S: ToString, P: Into<PathBuf>>(
        contents: S,
        path: P,
        type_token: Option<String>,
    ) -> SourceFile {
        SourceFile::Contents {
            contents: contents.to_string(),
            path: path.into(),
            type_token,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            SourceFile::Path(p) => p.as_path(),
            SourceFile::Contents { path, .. } => path.as_path(),
        }
    }
}

impl PartialOrd for SourceFile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (SourceFile::Path(self_path), SourceFile::Path(other_path)) => {
                self_path.partial_cmp(other_path)
            }
            (
                SourceFile::Path(self_path),
                SourceFile::Contents {
                    path: other_path, ..
                },
            ) => self_path.partial_cmp(other_path),
            (
                SourceFile::Contents {
                    path: self_path, ..
                },
                SourceFile::Path(other_path),
            ) => self_path.partial_cmp(other_path),
            (
                SourceFile::Contents {
                    path: self_path, ..
                },
                SourceFile::Contents {
                    path: other_path, ..
                },
            ) => self_path.partial_cmp(other_path),
        }
    }
}

impl Ord for SourceFile {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or_else(|| Ordering::Equal)
    }
}
