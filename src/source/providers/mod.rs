mod git_repository;
use anyhow::Result;
pub use git_repository::*;

use super::Source;

pub trait SourceProvider {
    fn apply(&self, source: &mut Source) -> Result<()>;
}
