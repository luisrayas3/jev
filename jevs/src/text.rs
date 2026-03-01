pub const API_DOCS: &str = r#"## Text - pure functions

```rust
let n = jevs::text::line_count("hello\nworld"); // 2
let s = jevs::text::concat(&["a", "b", "c"]);  // "abc"
```
"#;

/// Count lines in a string.
pub fn line_count(text: &str) -> usize {
    text.lines().count()
}

/// Concatenate string slices.
pub fn concat(parts: &[&str]) -> String {
    parts.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_lines() {
        assert_eq!(line_count(""), 0);
        assert_eq!(line_count("one"), 1);
        assert_eq!(line_count("a\nb\nc"), 3);
    }

    #[test]
    fn concats_parts() {
        assert_eq!(concat(&[]), "");
        assert_eq!(concat(&["a", "b", "c"]), "abc");
    }
}
