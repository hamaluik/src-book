use crate::source::Source;
use anyhow::Result;

mod xelatex;
pub use xelatex::*;

mod pdf;
pub use pdf::*;

#[derive(Debug)]
pub enum Sink {
    XeLaTeX(XeLaTeX),
    PDF(PDF),
}

pub trait Render {
    fn render(&self, source: &Source) -> Result<()>;
}

impl Render for Sink {
    fn render(&self, source: &Source) -> Result<()> {
        match self {
            Sink::XeLaTeX(x) => x.render(source),
            Sink::PDF(p) => p.render(source),
        }
    }
}
