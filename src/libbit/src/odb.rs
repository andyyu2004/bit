use crate::error::{BitError, BitResult};
use crate::hash::{self, BitHash};
use crate::lockfile::Lockfile;
use crate::obj::{self, BitId, BitObj, BitObjHeader, BitObjKind};
use crate::path::BitPath;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::{prelude::*, BufReader};

pub struct BitObjDb {
    // backends will be searched in order
    backends: Vec<Box<dyn BitObjDbBackend>>,
}

impl BitObjDb {
    pub fn new(objects_path: BitPath) -> Self {
        Self {
            // we want to search the loose backend first
            backends: vec![
                Box::new(BitLooseObjDb::new(objects_path)),
                Box::new(BitPackedObjDb::new(objects_path)),
            ],
        }
    }
}

impl BitObjDbBackend for BitObjDb {
    // can't just pass in trait pointer
    fn read(&self, id: BitId) -> BitResult<BitObjKind> {
        // TODO should only delegate to the packeddb if the error is not found,
        // could do this by returning Result<Option<T>>
        // but that seems a bit painful? or check for existence first
        // before reading the file?
        self.backends[0].read(id)
        // .or_else(|_| self.packed.read(id))
    }

    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader> {
        self.backends[0].read_header(id)
        // .or_else(|_| self.packed.read_header(id))
    }

    fn write(&self, obj: &dyn BitObj) -> BitResult<BitHash> {
        // when to write to packed?
        self.backends[0].write(obj)
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        self.backends[0].exists(id)
    }
}

pub trait BitObjDbBackend {
    fn read(&self, id: BitId) -> BitResult<BitObjKind>;
    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader>;
    fn write(&self, obj: &dyn BitObj) -> BitResult<BitHash>;
    fn exists(&self, id: BitId) -> BitResult<bool>;
}

struct BitLooseObjDb {
    /// path to .git/objects
    objects_path: BitPath,
}

impl BitLooseObjDb {
    pub fn new(objects_path: BitPath) -> Self {
        Self { objects_path }
    }

    fn expand_id(&self, id: BitId) -> BitResult<BitHash> {
        let hash = match id {
            BitId::FullHash(hash) => hash,
            BitId::PartialHash(_) => todo!(),
        };
        Ok(hash)
    }

    fn obj_path(&self, hash: BitHash) -> BitPath {
        let (dir, file) = hash.split();
        self.objects_path.join(dir).join(file)
    }

    fn locate_obj(&self, id: BitId) -> BitResult<BitPath> {
        let hash = self.expand_id(id)?;
        Ok(self.obj_path(hash))
    }

    fn read_stream(&self, id: BitId) -> BitResult<impl BufRead> {
        let reader = File::open(self.locate_obj(id)?)?;
        Ok(BufReader::new(ZlibDecoder::new(reader)))
    }
}

impl BitObjDbBackend for BitLooseObjDb {
    fn read(&self, id: BitId) -> BitResult<BitObjKind> {
        let mut stream = self.read_stream(id)?;
        obj::read_obj(&mut stream)
    }

    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader> {
        let mut stream = self.read_stream(id)?;
        obj::read_obj_header(&mut stream)
    }

    fn write(&self, obj: &dyn BitObj) -> BitResult<BitHash> {
        let bytes = obj.serialize_with_headers()?;
        let hash = hash::hash_bytes(&bytes);
        let path = self.obj_path(hash);
        if path.as_path().exists() {
            #[cfg(debug_assertions)]
            {
                let mut buf = vec![];
                ZlibDecoder::new(File::open(path)?).read_to_end(&mut buf)?;
                assert_eq!(buf, bytes, "same hash, different contents :O");
            }
            return Ok(hash);
        }
        let lockfile = Lockfile::new(&path)?;
        ZlibEncoder::new(lockfile, Compression::default()).write_all(&bytes)?;
        Ok(hash)
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        // TODO this isn't entirely accurate
        // we will need to check the actual error
        // to differentiate between nonexistence and an actual error
        Ok(self.locate_obj(id).is_ok())
    }
}

struct BitPackedObjDb {
    /// path to .git/objects
    objects_path: BitPath,
}

impl BitPackedObjDb {
    pub fn new(objects_path: BitPath) -> Self {
        Self { objects_path }
    }
}

impl BitObjDbBackend for BitPackedObjDb {
    fn read(&self, id: BitId) -> BitResult<BitObjKind> {
        bail!(BitError::ObjectNotFound(id))
    }

    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader> {
        bail!(BitError::ObjectNotFound(id))
    }

    fn write(&self, _obj: &dyn BitObj) -> BitResult<BitHash> {
        todo!()
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        bail!(BitError::ObjectNotFound(id))
    }
}
