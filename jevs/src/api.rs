use crate::{file, stash, text, trust};

pub fn catalog() -> String {
    [
        "# jevs API\n",
        file::FILE_API_DOCS,
        file::TREE_API_DOCS,
        stash::API_DOCS,
        text::API_DOCS,
        trust::API_DOCS,
    ]
    .join("\n")
}
