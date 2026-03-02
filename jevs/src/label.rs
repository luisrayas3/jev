pub const API_DOCS: &str = r#"## Label types - `jevs::label`

```rust
use jevs::label::{Tainted, Endorsed};

let raw = Tainted(some_value);  // low-integrity data
let checked = raw.promote();    // -> Endorsed<T>
checked.inner()                 // &T
checked.into_inner()            // T
```

Functions that require integrity take `Endorsed<T>`.
Passing `Tainted<T>` is a compile error.
"#;

/// Data from an external/untrusted source.
/// Must be explicitly promoted before use in sensitive operations.
pub struct Tainted<T>(pub T);

/// Data that has been human-confirmed or otherwise endorsed.
/// Sensitive operations require `Endorsed<T>`;
/// passing `Tainted<T>` is a compile error.
pub struct Endorsed<T>(pub T);

impl<T> Tainted<T> {
    /// Promote data to endorsed integrity.
    /// This is the only way to obtain `Endorsed<T>`.
    /// TODO: Gate on human confirmation.
    pub fn promote(self) -> Endorsed<T> {
        Endorsed(self.0)
    }
}

impl<T> Endorsed<T> {
    pub fn inner(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promote_flow() {
        let raw = Tainted("secret");
        let checked = raw.promote();
        assert_eq!(*checked.inner(), "secret");
    }

    #[test]
    fn into_inner_consumes() {
        let raw = Tainted(42);
        let val = raw.promote().into_inner();
        assert_eq!(val, 42);
    }
}
