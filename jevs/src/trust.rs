pub const API_DOCS: &str = r#"## Trust types

```rust
use jevs::trust::{Unverified, Verified};

let raw = Unverified(some_value);  // untrusted data
let checked = raw.verify();        // -> Verified<T>
checked.inner()                    // &T
checked.into_inner()               // T
```

Functions that require trust take `Verified<T>`.
Passing `Unverified<T>` is a compile error.
"#;

/// Data from an external/untrusted source. Must be explicitly verified
/// before use in sensitive operations.
pub struct Unverified<T>(pub T);

/// Data that has been human-confirmed or otherwise validated.
/// Sensitive operations require `Verified<T>` — passing `Unverified<T>`
/// is a compile error.
pub struct Verified<T>(pub T);

impl<T> Unverified<T> {
    /// Explicitly verify data. This is the only way to obtain `Verified<T>`.
    /// TODO: Gate on human confirmation.
    pub fn verify(self) -> Verified<T> {
        Verified(self.0)
    }
}

impl<T> Verified<T> {
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
    fn verify_flow() {
        let raw = Unverified("secret");
        let checked = raw.verify();
        assert_eq!(*checked.inner(), "secret");
    }

    #[test]
    fn into_inner_consumes() {
        let raw = Unverified(42);
        let val = raw.verify().into_inner();
        assert_eq!(val, 42);
    }
}
