use crate::error::{BitError, BitErrorExt, BitResult};
use crate::hash;
use crate::iter::DirIter;
use crate::lockfile::Lockfile;
use crate::obj::{self, BitId, BitObj, BitObjHeader, BitObjKind, Oid, PartialOid};
use crate::pack::Pack;
use crate::path::BitPath;
use fallible_iterator::FallibleIterator;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use itertools::Itertools;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::fs::File;
use std::io::{prelude::*, BufReader};

//? questionable name, questionable macro is there a better way to express this pattern
macro_rules! process {
    ($expr:expr) => {
        match $expr {
            Ok(obj) => return Ok(obj),
            Err(err) if err.is_fatal() => return Err(err),
            Err(..) => continue,
        }
    };
}

pub struct BitObjDb {
    // backends will be searched in order
    backends: Vec<Box<dyn BitObjDbBackend>>,
}

impl BitObjDb {
    pub fn new(objects_path: BitPath) -> BitResult<Self> {
        Ok(Self {
            // we want to search the loose backend first
            backends: vec![
                Box::new(BitLooseObjDb::new(objects_path)),
                Box::new(BitPackedObjDb::new(objects_path)?),
            ],
        })
    }
}

macro_rules! backend_method {
    ($f:ident: $arg_ty:ty => $ret_ty:ty) => {
        fn $f(&self, arg: $arg_ty) -> BitResult<$ret_ty> {
            //? does it make sense to return the last non_fatal error? or any particular error?
            // probably doesn't really matter
            let mut last_err = None;
            for backend in &self.backends {
                match backend.$f(arg) {
                    Ok(ret) => return Ok(ret),
                    Err(err) if err.is_fatal() => return Err(err),
                    Err(err) => {
                        last_err = Some(err);
                        continue;
                    }
                }
            }
            Err(last_err.unwrap_or_else(|| {
                anyhow!("all backends failed to execute method `{}`", stringify!($f))
            }))
        }
    };
}

impl BitObjDbBackend for BitObjDb {
    backend_method!(read: BitId => BitObjKind);

    backend_method!(read_header: BitId => BitObjHeader);

    backend_method!(write: &dyn BitObj => Oid);

    backend_method!(expand_prefix: PartialOid => Oid);

    fn exists(&self, id: BitId) -> BitResult<bool> {
        Ok(self.backends.iter().any(|backend| backend.exists(id).unwrap_or_default()))
    }
}

pub trait BitObjDbBackend {
    fn read(&self, id: BitId) -> BitResult<BitObjKind>;
    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader>;
    fn write(&self, obj: &dyn BitObj) -> BitResult<Oid>;
    fn exists(&self, id: BitId) -> BitResult<bool>;
    fn expand_prefix(&self, prefix: PartialOid) -> BitResult<Oid>;

    fn expand_id(&self, id: BitId) -> BitResult<Oid> {
        match id {
            BitId::Full(oid) => Ok(oid),
            BitId::Partial(partial) => self.expand_prefix(partial),
        }
    }
}

struct BitLooseObjDb {
    /// path to .git/objects
    objects_path: BitPath,
}

impl BitLooseObjDb {
    pub fn new(objects_path: BitPath) -> Self {
        Self { objects_path }
    }

    // this should be infallible as it is used by write
    // in particular, this should *not* check for the existence of the path
    fn obj_path(&self, oid: Oid) -> BitPath {
        let (dir, file) = oid.split();
        self.objects_path.join(dir).join(file)
    }

    fn locate_obj(&self, id: BitId) -> BitResult<BitPath> {
        let hash = self.expand_id(id)?;
        let path = self.obj_path(hash);
        if path.exists() { Ok(path) } else { Err(anyhow!(BitError::ObjectNotFound(hash.into()))) }
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

    fn expand_prefix(&self, prefix: PartialOid) -> BitResult<Oid> {
        let (dir, file) = prefix.split();
        let dir = self.objects_path.join(dir);
        DirIter::new(dir).filter(|entry| Ok(entry.file_name().to_str().unwrap().starts_with(file)));
        // .collect::<Result<Vec<_>, _>>()?;
        todo!()
    }

    fn write(&self, obj: &dyn BitObj) -> BitResult<Oid> {
        let bytes = obj.serialize_with_headers()?;
        let hash = hash::hash_bytes(&bytes);
        let path = self.obj_path(hash);

        #[cfg(debug_assertions)]
        if path.as_path().exists() {
            {
                let mut buf = vec![];
                ZlibDecoder::new(File::open(path)?).read_to_end(&mut buf)?;
                assert_eq!(buf, bytes, "same hash, different contents :O");
            }
            return Ok(hash);
        }

        Lockfile::with_mut(&path, |lockfile| {
            Ok(ZlibEncoder::new(lockfile, Compression::default()).write_all(&bytes)?)
        })?;

        Ok(hash)
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        match self.locate_obj(id) {
            Ok(..) => Ok(true),
            Err(err) if err.is_not_found_err() => Ok(false),
            Err(err) => Err(err),
        }
    }
}

struct BitPackedObjDb {
    /// path to .git/objects
    objects_path: BitPath,
    /// [(packfile, idxfile)]
    packs: Vec<Pack>,
}

impl BitPackedObjDb {
    pub fn new(objects_path: BitPath) -> BitResult<Self> {
        let pack_dir = objects_path.join("pack");
        let mut packs = vec![];

        if !pack_dir.exists() {
            return Ok(Self { objects_path, packs });
        }

        for entry in std::fs::read_dir(pack_dir)? {
            let entry = entry?;
            let pack = BitPath::intern(entry.path());
            if pack.extension() != Some("pack".as_ref()) {
                continue;
            }

            let idx = pack.with_extension("idx");
            ensure!(idx.exists(), "packfile `{}` is missing a corresponding index file", pack);
            packs.push(Pack { pack, idx });
        }

        Ok(Self { objects_path, packs })
    }
}

impl BitObjDbBackend for BitPackedObjDb {
    fn read(&self, id: BitId) -> BitResult<BitObjKind> {
        let oid = self.expand_id(id)?;
        for &pack in &self.packs {
            process!(pack.read_obj(oid));
        }
        bail!(BitError::ObjectNotFound(id))
    }

    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader> {
        let oid = self.expand_id(id)?;
        for &pack in &self.packs {
            process!(pack.read_obj_header(oid));
        }
        bail!(BitError::ObjectNotFound(id))
    }

    fn write(&self, _obj: &dyn BitObj) -> BitResult<Oid> {
        todo!()
    }

    fn expand_prefix(&self, prefix: PartialOid) -> BitResult<Oid> {
        todo!()
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        let oid = self.expand_id(id)?;
        Ok(self.packs.par_iter().any(|pack| pack.obj_exists(oid).unwrap_or_default()))
    }
}
