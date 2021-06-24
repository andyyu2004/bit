use super::*;
use fallible_iterator::FallibleIterator;

#[test]
fn bench_index_tree_iterator() -> BitResult<()> {
    let bytes = include_bytes!("../../tests/files/lg2index") as &[u8];
    let index = BitIndexInner::deserialize_unbuffered(bytes)?;
    dbg!(index.tree_iter().count()?);
    Ok(())
}
