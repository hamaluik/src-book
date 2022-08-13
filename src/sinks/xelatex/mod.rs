use super::Render;
use anyhow::{Context, Result};

#[derive(Debug)]
pub struct XeLaTeX {}

impl Render for XeLaTeX {
    fn render(&self, source: &crate::source::Source) -> Result<()> {
        todo!()
    }
}
