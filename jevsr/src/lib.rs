use jevs::runtime::RuntimeKey;

const MAGIC: u64 = 0x6A65_7673;

pub fn open_file(root: &str) -> jevs::file::File {
    let key = RuntimeKey::new(MAGIC);
    jevs::file::File::open(key, root)
}
