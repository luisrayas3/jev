use crate::{file, text, trust};

pub fn catalog() -> String {
    [
        "# jevs API\n",
        file::API_DOCS,
        text::API_DOCS,
        trust::API_DOCS,
    ]
    .join("\n")
}
