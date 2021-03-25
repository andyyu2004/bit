use crate::error::BitResult;
use crate::hash::{self, BitHash};
use crate::lockfile::Lockfile;
use crate::obj::{self, BitObj, BitObjHeader, BitObjKind};
use flate2::read::ZlibDecoder;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path::PathBuf;

struct BitObjDb {
    backends: Vec<Box<dyn BitObjDbBackend>>,
}
trait BitObjDbBackend {
    fn read(&self, hash: BitHash) -> BitResult<BitObjKind>;
    // prefix is represented as hash having trailing zeroes?
    fn read_prefix(&self, hash: BitHash) -> BitResult<BitObjKind>;
    fn read_header(&self, hash: BitHash) -> BitResult<BitObjHeader>;

    fn write(&mut self, obj: &BitObjKind) -> BitResult<BitHash>;

    fn exists(&self, hash: BitHash) -> BitResult<bool>;
    fn exists_prefix(&self, hash: BitHash) -> BitResult<bool>;
}

struct BitLooseObjDb {
    objects_path: PathBuf,
}

impl BitLooseObjDb {
    fn obj_path(&self, hash: BitHash) -> PathBuf {
        let (dir, file) = hash.split();
        self.objects_path.join(dir).join(file)
    }

    fn read_stream(&self, hash: BitHash) -> BitResult<impl BufRead> {
        let reader = File::open(self.obj_path(hash))?;
        Ok(BufReader::new(ZlibDecoder::new(reader)))
    }
}

impl BitObjDbBackend for BitLooseObjDb {
    fn read(&self, hash: BitHash) -> BitResult<BitObjKind> {
        let mut stream = self.read_stream(hash)?;
        obj::read_obj_buffered(&mut stream)
    }

    fn read_prefix(&self, hash: BitHash) -> BitResult<BitObjKind> {
        todo!()
    }

    fn read_header(&self, hash: BitHash) -> BitResult<BitObjHeader> {
        let mut stream = self.read_stream(hash)?;
        obj::read_obj_header_buffered(&mut stream)
    }

    fn write(&mut self, obj: &BitObjKind) -> BitResult<BitHash> {
        let bytes = obj.serialize_with_headers()?;
        let hash = hash::hash_bytes(&bytes);
        let path = self.obj_path(hash);
        if path.exists() {
            debug_assert_eq!(std::fs::read(path)?, bytes, "same hash, different contents :O");
            return Ok(hash);
        }
        let mut lockfile = Lockfile::new(&path)?;
        lockfile.write(&bytes)?;
        Ok(hash)
    }

    fn exists(&self, hash: BitHash) -> BitResult<bool> {
        Lockfile::exists(self.obj_path(hash))
    }

    fn exists_prefix(&self, hash: BitHash) -> BitResult<bool> {
        todo!()
    }
}
