const MAGIC: u64 = 0x6A65_7673;

pub struct RuntimeKey(());

impl RuntimeKey {
    pub fn new(magic: u64) -> Self {
        assert_eq!(magic, MAGIC, "invalid runtime key");
        RuntimeKey(())
    }
}
