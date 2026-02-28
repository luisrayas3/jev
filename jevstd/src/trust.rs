/// Data from an external/untrusted source. Must be explicitly verified
/// before use in sensitive operations.
pub struct Unverified<T>(pub T);

/// Data that has been human-confirmed or otherwise validated.
/// Sensitive operations require `Verified<T>` — passing `Unverified<T>`
/// is a compile error.
pub struct Verified<T>(pub T);

impl<T> Unverified<T> {
    /// Explicitly verify data. This is the only way to obtain `Verified<T>`.
    /// In a real system this would gate on human confirmation.
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
