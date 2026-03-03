use std::marker::PhantomData;

pub const API_DOCS: &str = r#"## Labels - `jevs::label`

Resources and data carry labels:
confidentiality (Private/Public)
and integrity (Me/Friend/World).
Labels propagate automatically through operations.

```rust
// read returns labeled data
let content = res.data.read("file.txt").await?;

// map preserves labels
let upper = content.map(|s| s.to_uppercase());

// Access the inner value
let s: &str = upper.inner();
let s: String = upper.into_inner();

// Create local data (Public confidentiality, Me integrity)
let fresh = jevs::label::Labeled::local("hello".to_string());

// Combine two labeled values (lattice join: most restrictive)
let combined = a.join(b, |x, y| format!("{x} {y}"));

// Cross label boundaries (human confirmation, async)
let public = jevs::declassify!(private_data).await?;
let trusted = jevs::accredit!(untrusted, jevs::label::Friend).await?;
```

Labels are checked at compile time.
Writing to a resource requires compatible labels.
Most plans: read, transform, write. Labels flow automatically.
"#;

// -- Classification levels ---------------------------------------------------

pub struct Public;
pub struct Private;

/// Marker trait for classification levels.
pub trait Classification {
    fn name() -> &'static str;
}
impl Classification for Public {
    fn name() -> &'static str { "Public" }
}
impl Classification for Private {
    fn name() -> &'static str { "Private" }
}

// -- Integrity levels ------------------------------------------------------

pub struct Me;
pub struct Friend;
pub struct World;

/// Marker trait for integrity levels.
pub trait Integrity {
    fn name() -> &'static str;
}
impl Integrity for Me {
    fn name() -> &'static str { "Me" }
}
impl Integrity for Friend {
    fn name() -> &'static str { "Friend" }
}
impl Integrity for World {
    fn name() -> &'static str { "World" }
}

// -- Lattice join: classification --------------------------------------------
// Most restrictive wins (max in lattice: Private > Public).

pub trait ClassificationJoin<Other: Classification>: Classification {
    type Out: Classification;
}

impl ClassificationJoin<Public> for Public { type Out = Public; }
impl ClassificationJoin<Private> for Public { type Out = Private; }
impl ClassificationJoin<Public> for Private { type Out = Private; }
impl ClassificationJoin<Private> for Private { type Out = Private; }

// -- Lattice join: integrity -----------------------------------------------
// Least trustworthy wins (min in lattice: Me > Friend > World).

pub trait IntegrityJoin<Other: Integrity>: Integrity {
    type Out: Integrity;
}

impl IntegrityJoin<Me> for Me { type Out = Me; }
impl IntegrityJoin<Friend> for Me { type Out = Friend; }
impl IntegrityJoin<World> for Me { type Out = World; }
impl IntegrityJoin<Me> for Friend { type Out = Friend; }
impl IntegrityJoin<Friend> for Friend { type Out = Friend; }
impl IntegrityJoin<World> for Friend { type Out = World; }
impl IntegrityJoin<Me> for World { type Out = World; }
impl IntegrityJoin<Friend> for World { type Out = World; }
impl IntegrityJoin<World> for World { type Out = World; }

// -- Satisfaction: can data flow to a context requiring Required? -------------

/// Data with classification C can flow where Required is needed.
/// C <= Required in the lattice (at most as restrictive).
pub trait SatisfiesClassification<Required: Classification> {}

impl SatisfiesClassification<Public> for Public {}
impl SatisfiesClassification<Private> for Public {}
impl SatisfiesClassification<Private> for Private {}

/// Data with integrity I meets the Required minimum.
/// I >= Required in the lattice (at least as trustworthy).
pub trait SatisfiesIntegrity<Required: Integrity> {}

impl SatisfiesIntegrity<Me> for Me {}
impl SatisfiesIntegrity<Friend> for Me {}
impl SatisfiesIntegrity<World> for Me {}
impl SatisfiesIntegrity<Friend> for Friend {}
impl SatisfiesIntegrity<World> for Friend {}
impl SatisfiesIntegrity<World> for World {}

// -- Labeled<T, C, I> -------------------------------------------------------

/// Data carrying classification and integrity labels.
/// Labels propagate through map/join
/// and are checked at compile time.
pub struct Labeled<T, C: Classification, I: Integrity> {
    value: T,
    _c: PhantomData<C>,
    _i: PhantomData<I>,
}

impl<T, C: Classification, I: Integrity> Labeled<T, C, I> {
    pub fn new(value: T) -> Self {
        Labeled {
            value,
            _c: PhantomData,
            _i: PhantomData,
        }
    }

    pub fn inner(&self) -> &T {
        &self.value
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Labeled<U, C, I> {
        Labeled::new(f(self.value))
    }

    pub fn join<U, V, C2, I2>(
        self,
        other: Labeled<U, C2, I2>,
        f: impl FnOnce(T, U) -> V,
    ) -> Labeled<V, <C as ClassificationJoin<C2>>::Out, <I as IntegrityJoin<I2>>::Out>
    where
        C2: Classification,
        I2: Integrity,
        C: ClassificationJoin<C2>,
        I: IntegrityJoin<I2>,
    {
        Labeled::new(f(self.value, other.value))
    }

    /// Decrease confidentiality: Private -> Public.
    /// Requires human confirmation (async).
    pub async fn declassify(self) -> anyhow::Result<Labeled<T, Public, I>> {
        // TODO: human confirmation gate
        Ok(Labeled::new(self.value))
    }

    /// Increase integrity to a target level.
    /// Requires human confirmation (async).
    pub async fn accredit<Target: Integrity>(
        self,
    ) -> anyhow::Result<Labeled<T, C, Target>> {
        // TODO: human confirmation gate
        Ok(Labeled::new(self.value))
    }

    /// Declassify with gate check.
    /// Use via `jevs::declassify!` macro.
    pub async fn declassify_gated(
        self,
        info: &crate::gate::CrossingInfo,
    ) -> anyhow::Result<Labeled<T, Public, I>> {
        crate::gate::check(info)?;
        Ok(Labeled::new(self.value))
    }

    /// Accredit with gate check.
    /// Use via `jevs::accredit!` macro.
    pub async fn accredit_gated<Target: Integrity>(
        self,
        info: &crate::gate::CrossingInfo,
    ) -> anyhow::Result<Labeled<T, C, Target>> {
        crate::gate::check(info)?;
        Ok(Labeled::new(self.value))
    }
}

/// Create data with maximum trust:
/// Public confidentiality, Me integrity.
/// Use for locally-constructed values
/// not derived from resources.
impl<T> Labeled<T, Public, Me> {
    pub fn local(value: T) -> Self {
        Labeled::new(value)
    }
}

// -- Declassifiable ----------------------------------------------------------

/// Bounded-output types that can auto-cross label boundaries.
/// An enum with N variants carries at most log2(N) bits
/// from the input. Adversarial content can influence
/// which variant (misclassification)
/// but the variant itself is a known-good value.
pub trait Declassifiable {}

impl Declassifiable for bool {}
impl Declassifiable for u8 {}
impl Declassifiable for u16 {}
impl Declassifiable for u32 {}
impl Declassifiable for u64 {}
impl Declassifiable for usize {}
impl Declassifiable for i8 {}
impl Declassifiable for i16 {}
impl Declassifiable for i32 {}
impl Declassifiable for i64 {}
impl Declassifiable for f32 {}
impl Declassifiable for f64 {}

// -- Gated crossing macros ---------------------------------------------------

#[macro_export]
macro_rules! declassify {
    ($expr:expr) => {{
        #[::linkme::distributed_slice(::jevs::gate::CROSSINGS)]
        static CROSSING: $crate::gate::CrossingInfo =
            $crate::gate::CrossingInfo::new(
                file!(), line!(), "declassify", "",
            );
        $expr.declassify_gated(&CROSSING)
    }};
}

#[macro_export]
macro_rules! accredit {
    ($expr:expr, $tier:ty) => {{
        #[::linkme::distributed_slice(::jevs::gate::CROSSINGS)]
        static CROSSING: $crate::gate::CrossingInfo =
            $crate::gate::CrossingInfo::new(
                file!(), line!(), "accredit",
                stringify!($tier),
            );
        $expr.accredit_gated::<$tier>(&CROSSING)
    }};
}

// -- Manual trait impls (avoid bounds on phantom types) -----------------------

impl<T: Clone, C: Classification, I: Integrity> Clone for Labeled<T, C, I> {
    fn clone(&self) -> Self {
        Labeled {
            value: self.value.clone(),
            _c: PhantomData,
            _i: PhantomData,
        }
    }
}

impl<T: std::fmt::Debug, C: Classification, I: Integrity> std::fmt::Debug
    for Labeled<T, C, I>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Labeled")
            .field("value", &self.value)
            .finish()
    }
}

impl<T: PartialEq, C: Classification, I: Integrity> PartialEq for Labeled<T, C, I> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_and_inner() {
        let data = Labeled::local(42);
        assert_eq!(*data.inner(), 42);
    }

    #[test]
    fn into_inner_consumes() {
        let data: Labeled<_, Private, Me> = Labeled::new("hello");
        assert_eq!(data.into_inner(), "hello");
    }

    #[test]
    fn map_preserves_labels() {
        let data: Labeled<i32, Private, Friend> = Labeled::new(10);
        let doubled: Labeled<i32, Private, Friend> = data.map(|x| x * 2);
        assert_eq!(*doubled.inner(), 20);
    }

    #[test]
    fn join_takes_most_restrictive() {
        let a: Labeled<&str, Public, Me> = Labeled::new("hi");
        let b: Labeled<&str, Private, World> = Labeled::new("lo");
        let c: Labeled<String, Private, World> =
            a.join(b, |x, y| format!("{x} {y}"));
        assert_eq!(c.into_inner(), "hi lo");
    }

    #[tokio::test]
    async fn declassify_private_to_public() {
        let data: Labeled<&str, Private, Me> = Labeled::new("secret");
        let public: Labeled<&str, Public, Me> =
            data.declassify().await.unwrap();
        assert_eq!(*public.inner(), "secret");
    }

    #[tokio::test]
    async fn accredit_world_to_friend() {
        let data: Labeled<&str, Public, World> = Labeled::new("untrusted");
        let trusted: Labeled<&str, Public, Friend> =
            data.accredit::<Friend>().await.unwrap();
        assert_eq!(*trusted.inner(), "untrusted");
    }

    #[test]
    fn classification_name() {
        assert_eq!(Public::name(), "Public");
        assert_eq!(Private::name(), "Private");
    }

    #[test]
    fn integrity_name() {
        assert_eq!(Me::name(), "Me");
        assert_eq!(Friend::name(), "Friend");
        assert_eq!(World::name(), "World");
    }

    #[tokio::test]
    async fn declassify_gated_with_allow() {
        let info = crate::gate::CrossingInfo::new(
            "test", 1, "declassify", "",
        );
        info.set_policy(crate::gate::Policy::Allow);
        let data: Labeled<&str, Private, Me> = Labeled::new("secret");
        let public = data.declassify_gated(&info).await.unwrap();
        assert_eq!(*public.inner(), "secret");
    }

    #[tokio::test]
    async fn declassify_gated_with_prompt_approved() {
        let info = crate::gate::CrossingInfo::new(
            "test", 1, "declassify", "",
        );
        info.set_policy(crate::gate::Policy::Prompt);
        crate::gate::inject_response(true);
        let data: Labeled<&str, Private, Me> = Labeled::new("secret");
        let public = data.declassify_gated(&info).await.unwrap();
        assert_eq!(*public.inner(), "secret");
    }

    #[tokio::test]
    async fn accredit_gated_with_allow() {
        let info = crate::gate::CrossingInfo::new(
            "test", 1, "accredit", "Friend",
        );
        info.set_policy(crate::gate::Policy::Allow);
        let data: Labeled<&str, Public, World> =
            Labeled::new("untrusted");
        let trusted: Labeled<&str, Public, Friend> =
            data.accredit_gated::<Friend>(&info).await.unwrap();
        assert_eq!(*trusted.inner(), "untrusted");
    }
}
