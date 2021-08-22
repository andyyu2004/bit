use criterion::{criterion_group, criterion_main, Criterion};
use fallible_iterator::FallibleIterator;
use libbit::index::BitIndexInner;
use libbit::repo::BitRepo;
use libbit::serialize::Deserialize;

pub fn bench_index_tree_iter(c: &mut Criterion) {
    let bytes = include_bytes!("../tests/files/lg2index") as &[u8];
    let index = BitIndexInner::deserialize_unbuffered(bytes).unwrap();
    c.bench_function("index_tree_iter", |b| b.iter(|| index.index_tree_iter().count().unwrap()));
}

macro_rules! test_files_dir {
    () => {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/files")
    };
    ($path:expr) => {
        test_files_dir!().join($path)
    };
}

pub fn bench_write_tree(c: &mut Criterion) {
    let path = tempfile::tempdir().unwrap().into_path();
    BitRepo::init(&path).unwrap();
    std::fs::copy(test_files_dir!("lg2index"), path.join(".git/index")).unwrap();
    c.bench_function("index_write_tree", |b| {
        b.iter(|| BitRepo::find(&path, |repo| repo.with_index_mut(|index| index.write_tree())))
    });
}

criterion_group!(benches, bench_write_tree, bench_index_tree_iter);
criterion_main!(benches);
