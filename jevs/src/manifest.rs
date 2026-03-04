use linkme::distributed_slice;

pub struct Need {
    pub path: &'static str,
    pub kind: &'static str,
    pub classification: &'static str,
    pub integrity: &'static str,
}

impl Need {
    pub const fn new(
        path: &'static str,
        kind: &'static str,
        classification: &'static str,
        integrity: &'static str,
    ) -> Self {
        Need { path, kind, classification, integrity }
    }
}

#[distributed_slice]
pub static NEEDS: [Need];
