use crate::error::{BitError, BitGenericError, BitResult, BitResultExt};
use crate::hash;
use crate::iter::DirIter;
use crate::lockfile::{Lockfile, LockfileFlags};
use crate::obj::{self, *};
use crate::pack::Pack;
use crate::path::BitPath;
use arrayvec::ArrayVec;
use fallible_iterator::FallibleIterator;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use parking_lot::{Mutex, RwLock};
use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use smallvec::SmallVec;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::str::FromStr;

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
    backends: ArrayVec<Box<dyn BitObjDbBackend>, 2>,
}

impl BitObjDb {
    pub fn new(objects_path: BitPath) -> BitResult<Self> {
        Ok(Self {
            //? we want to search the loose backend first as its cheaper (at least intuitively)
            backends: ArrayVec::from([
                Box::new(BitLooseObjDb::new(objects_path)) as Box<dyn BitObjDbBackend>,
                Box::new(BitPackedObjDb::new(objects_path)?),
            ]),
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

// returns the first success or if everything failed, then one of the errors
macro_rules! backend_method_parallel_any {
    ($f:ident: $arg_ty:ty => $ret_ty:ty) => {
        fn $f(&self, arg: $arg_ty) -> BitResult<$ret_ty> {
            let error = Mutex::new(None);
            let output = self
                .backends
                .par_iter()
                .filter_map(|backend| match backend.$f(arg) {
                    Ok(raw) => Some(raw),
                    Err(err) => {
                        *error.lock() = Some(err);
                        None
                    }
                })
                .find_any(|_| true);

            match output {
                // if anything succeeded return that
                Some(result) => Ok(result),
                // otherwise there must have been an error we just arbitrarily return one of the errors
                None => Err(error.into_inner().unwrap()),
            }
        }
    };
}

impl BitObjDbBackend for BitObjDb {
    backend_method!(read_header: BitId => BitObjHeader);

    // not much point making write parallel as pack backend is not writable anyway
    backend_method!(write: &dyn WritableObject => Oid);

    backend_method!(read_raw: BitId => BitRawObj);

    fn prefix_candidates(&self, prefix: PartialOid) -> BitResult<Vec<Oid>> {
        self.backends
            .par_iter()
            .try_fold(Vec::new, |mut candidates, backend| {
                candidates.extend(backend.prefix_candidates(prefix)?);
                Ok::<_, BitGenericError>(candidates)
            })
            .try_reduce(Vec::new, |mut a, b| {
                a.extend(b);
                Ok(a)
            })
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        Ok(self.backends.par_iter().any(|backend| backend.exists(id).unwrap_or_default()))
    }
}

pub trait BitObjDbBackend: Send + Sync {
    fn read_raw(&self, id: BitId) -> BitResult<BitRawObj>;
    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader>;
    fn write(&self, obj: &dyn WritableObject) -> BitResult<Oid>;
    fn exists(&self, id: BitId) -> BitResult<bool>;
    /// return a vector of oids that have a matching prefix
    // this method should NOT return an error if no candidates are found,
    // but instead represent that as an empty list of candidates
    fn prefix_candidates(&self, prefix: PartialOid) -> BitResult<Vec<Oid>>;

    fn expand_prefix(&self, prefix: PartialOid) -> BitResult<Oid> {
        trace!("expand_prefix(prefix: {})", prefix);
        let candidates = self.prefix_candidates(prefix)?;
        trace!("expand_prefix(..) :: candidates = {:?}", candidates);
        match candidates.len() {
            0 => Err(anyhow!(BitError::ObjectNotFound(prefix.into()))),
            1 => Ok(candidates[0]),
            _ => Err(anyhow!(BitError::AmbiguousPrefix(prefix, candidates))),
        }
    }

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

    fn locate_obj(&self, id: impl Into<BitId>) -> BitResult<BitPath> {
        let oid = self.expand_id(id.into())?;
        let path = self.obj_path(oid);
        if path.exists() { Ok(path) } else { Err(anyhow!(BitError::ObjectNotFound(oid.into()))) }
    }

    fn read_stream(&self, id: impl Into<BitId>) -> BitResult<impl BufRead> {
        let reader = File::open(self.locate_obj(id)?)?;
        Ok(BufReader::new(ZlibDecoder::new(reader)))
    }
}

impl BitObjDbBackend for BitLooseObjDb {
    fn read_raw(&self, id: BitId) -> BitResult<BitRawObj> {
        trace!("BitLooseObjDb::read_odb_obj(id: {})", id);
        let oid = self.expand_id(id)?;
        let mut stream = self.read_stream(oid)?;
        let BitObjHeader { obj_type, size } = obj::read_obj_header(&mut stream)?;
        Ok(BitRawObj::new(oid, obj_type, size, Box::new(stream)))
    }

    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader> {
        let mut stream = self.read_stream(id)?;
        obj::read_obj_header(&mut stream)
    }

    fn write(&self, obj: &dyn WritableObject) -> BitResult<Oid> {
        let bytes = obj.serialize_with_headers()?;
        let oid = hash::hash_bytes(&bytes);
        let path = self.obj_path(oid);

        if path.as_path().exists() {
            #[cfg(debug_assertions)]
            {
                let mut buf = vec![];
                ZlibDecoder::new(File::open(path)?).read_to_end(&mut buf)?;
                assert_eq!(buf, bytes, "same hash, different contents :O");
            }
        } else {
            Lockfile::with_mut(&path, LockfileFlags::SET_READONLY, |lockfile| {
                Ok(ZlibEncoder::new(lockfile, Compression::default()).write_all(&bytes)?)
            })?;
        }

        Ok(oid)
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        match self.locate_obj(id) {
            Ok(..) => Ok(true),
            Err(err) if err.is_not_found_err() => Ok(false),
            Err(err) => Err(err),
        }
    }

    fn prefix_candidates(&self, prefix: PartialOid) -> BitResult<Vec<Oid>> {
        let (dir, file_prefix) = prefix.split();
        let full_dir = self.objects_path.as_path().join(dir);
        if !full_dir.exists() {
            return Ok(vec![]);
        }

        // looks into the relevant folder (determined by the two hash digit prefix)
        // create oids by concatenating dir and the filename
        DirIter::new(full_dir)
            // it includes the "base" directory so we just explicitly filter that out for now
            // is that intentional behaviour?
            .filter(|entry| Ok(entry.path().is_file()))
            .filter_map(|entry| {
                let filename = entry.file_name().to_str().unwrap();
                // we must use `str::start_with` not `path::starts_with` as the latter only considers it component wise
                if !filename.starts_with(file_prefix) {
                    Ok(None)
                } else {
                    debug_assert_eq!(filename.len(), 38);
                    let oid = format!("{}{}", dir, filename);
                    debug_assert_eq!(oid.len(), 40);
                    Oid::from_str(&oid).map(Some)
                }
            })
            .collect::<Vec<_>>()
    }
}

struct BitPackedObjDb {
    /// [(packfile, idxfile)]
    packs: RwLock<SmallVec<[Pack; 1]>>,
}

impl BitPackedObjDb {
    pub fn new(objects_path: BitPath) -> BitResult<Self> {
        let pack_dir = objects_path.join("pack");
        let packs = Default::default();

        if !pack_dir.exists() {
            return Ok(Self { packs });
        }

        for entry in std::fs::read_dir(pack_dir)? {
            let entry = entry?;
            let pack = BitPath::intern(entry.path());
            if pack.extension() != Some("pack".as_ref()) {
                continue;
            }

            let idx = pack.with_extension("idx");
            ensure!(idx.exists(), "packfile `{}` is missing a corresponding index file", pack);
            packs.write().push(Pack::new(pack, idx)?);
        }

        Ok(Self { packs })
    }

    fn read_raw_pack_obj(&self, oid: Oid) -> BitResult<BitPackObjRaw> {
        trace!("BitPackedObjDb::read_raw(id: {})", oid);
        // for pack in self.packs.write().iter_mut() {
        //     process!(pack.read_obj_raw(oid));
        // }
        // the issue with the following is that we lose the real error and we just assume it's an object not found error
        match self.packs.write().iter_mut().flat_map(|pack| pack.read_obj_raw(oid)).find(|_| true) {
            Some(raw) => Ok(raw),
            None => bail!(BitError::ObjectNotFound(oid.into())),
        }
        // match self
        //     .packs
        //     .write()
        //     .par_iter_mut()
        //     .flat_map(|pack| pack.read_obj_raw(oid))
        //     .find_any(|_| true)
        // {
        //     Some(raw) => Ok(raw),
        //     None => bail!(BitError::ObjectNotFound(oid.into())),
        // }
    }
}

impl BitObjDbBackend for BitPackedObjDb {
    fn read_raw(&self, id: BitId) -> BitResult<BitRawObj> {
        trace!("BitPackedObjDb::read_odb_obj(id: {})", id);
        let oid = self.expand_id(id)?;
        self.read_raw_pack_obj(oid).map(|raw| BitRawObj::from_raw_pack_obj(oid, raw))
    }

    fn read_header(&self, id: BitId) -> BitResult<BitObjHeader> {
        let oid = self.expand_id(id)?;
        for pack in self.packs.write().iter_mut() {
            process!(pack.read_obj_header(oid));
        }
        bail!(BitError::ObjectNotFound(id))
    }

    fn write(&self, _obj: &dyn WritableObject) -> BitResult<Oid> {
        bail!("cannot write to pack odb backend")
    }

    fn exists(&self, id: BitId) -> BitResult<bool> {
        let oid = self.expand_id(id)?;
        Ok(self.packs.write().iter_mut().any(|pack| pack.obj_exists(oid).unwrap_or_default()))
    }

    fn prefix_candidates(&self, prefix: PartialOid) -> BitResult<Vec<Oid>> {
        Ok(self
            .packs
            .write()
            .par_iter_mut()
            .map(|pack| pack.prefix_matches(prefix))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect())
    }
}

#[cfg(test)]
mod tests;
