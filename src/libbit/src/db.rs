use std::path::PathBuf;

use crate::error::BitResult;
use crate::hash::BitHash;
use crate::obj::{self, BitObjHeader, BitObjKind};

struct BitObjDb {
    backends: Vec<Box<dyn BitObjDbBackend>>,
}
trait BitObjDbBackend {
    fn read(&self, hash: BitHash) -> BitResult<BitObjKind>;
    // prefix is represented as hash having trailing zeroes?
    fn read_prefix(&self, hash: BitHash) -> BitResult<BitObjKind>;
    fn read_header(&self, hash: BitHash) -> BitResult<BitObjHeader>;

    fn write(&mut self, obj: &BitObjKind) -> BitResult<BitHash>;

    fn exists(&self, hash: BitHash) -> bool;
    fn exists_prefix(&self, hash: BitHash) -> bool;
}

struct BitLooseObjDb {
    objects_path: PathBuf,
}

impl BitObjDbBackend for BitLooseObjDb {
    fn read(&self, hash: BitHash) -> BitResult<BitObjKind> {
        todo!()
    }

    fn read_prefix(&self, hash: BitHash) -> BitResult<BitObjKind> {
        todo!()
    }

    fn read_header(&self, hash: BitHash) -> BitResult<BitObjHeader> {
        todo!()
    }

    fn write(&mut self, obj: &BitObjKind) -> BitResult<BitHash> {
        todo!()
    }

    fn exists(&self, hash: BitHash) -> bool {
        todo!()
    }

    fn exists_prefix(&self, hash: BitHash) -> bool {
        todo!()
    }
}
