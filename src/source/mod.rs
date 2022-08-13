mod author;
pub use author::*;

mod source_file;
pub use source_file::*;

mod providers;
pub use providers::*;

/// Everything we need to know to render the source code of a project as a book
#[derive(Default, Debug)]
pub struct Source {
    /// The title of the source code / repository / book / etc
    pub title: Option<String>,

    /// All the authors of the repository (which will be sorted by prominence in descending order
    /// at render time)
    pub authors: Vec<Author>,

    /// The SPDX license ID(s) of the source code. NOTE: NOT validated by default Licenses can be
    /// validated by calling the `validate_licenses()` function, which will query the online SPDX
    /// API to check if the license is valid or not
    pub licenses: Vec<String>,

    /// All the source files that will be printed in the book
    pub source_files: Vec<SourceFile>,
}
