use anyhow::Result;
use std::path::{Path, PathBuf};

mod repository;
pub use repository::Repository;

pub trait Source {
    fn title(&self) -> &str;
    fn authors(&self) -> Vec<&str>;
    fn license(&self) -> Option<String>;
    fn list_source_files(&self) -> Vec<PathBuf>;
    fn source_file(&self, path: &Path) -> Result<String>;
}
