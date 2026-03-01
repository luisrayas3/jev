/// Count lines in a string.
pub fn line_count(text: &str) -> usize {
    text.lines().count()
}

/// Concatenate string slices.
pub fn concat(parts: &[&str]) -> String {
    parts.concat()
}
