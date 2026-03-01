use jevs::runtime::RuntimeKey;

const MAGIC: u64 = 0x6A65_7673;

pub fn open_file(root: &str) -> jevs::File {
    let key = RuntimeKey::new(MAGIC);
    jevs::File::open(key, root)
}
