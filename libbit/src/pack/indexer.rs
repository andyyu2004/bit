use super::*;
use crate::error::BitResult;
use crate::io::{BufReadExt, HashReader, ReadExt};
use crate::obj::{BitPackObjRaw, Oid};
use crate::serialize::Serialize;
use rustc_hash::FxHashMap;
use sha1::Sha1;
use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

pub struct PackIndexer<R> {
    pack_reader: PackfileReader<HashReader<Sha1, R>>,
    raw_objects: FxHashMap<u64, BitPackObjRaw>,
    oid_to_offset: FxHashMap<Oid, u64>,
    /// oid -> (offset, crc)
    sorted: BTreeMap<Oid, (u64, u32)>,
}

#[derive(Debug, Clone, Default)]
pub struct IndexPackOpts {
    pub index_file_path: Option<PathBuf>,
}

impl PackIndexer<FileBufferReader> {
    /// Builds a pack index file (<name>.idx) from the specified `<name>.pack` file.
    /// Overwrites any existing file at the output path
    pub fn write_pack_index(path: impl AsRef<Path>, opts: IndexPackOpts) -> BitResult<PackIndex> {
        let path = path.as_ref();
        let reader = FileBufferReader::new(path)?;
        let indexer = PackIndexer::new(reader)?;
        let pack_index = indexer.index_pack()?;
        let mut tmp_file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        pack_index.serialize(&mut tmp_file)?;
        let index_file_path = match opts.index_file_path {
            Some(path) => path,
            None => path.with_extension(PACK_IDX_EXT),
        };
        tmp_file.persist(index_file_path)?;
        Ok(pack_index)
    }
}

impl<R: BufRead> PackIndexer<R> {
    /// Marking this as crate private now to avoid triggering the bug where someone passes in
    pub(crate) fn new(reader: R) -> BitResult<Self> {
        let hash_reader = HashReader::new_sha1(reader);
        Ok(Self {
            pack_reader: PackfileReader::new(hash_reader)?,
            raw_objects: Default::default(),
            oid_to_offset: Default::default(),
            sorted: Default::default(),
        })
    }

    /// TODO parallelize
    pub(crate) fn index_pack(mut self) -> BitResult<PackIndex> {
        let n = self.pack_reader.objectc as usize;
        for _ in 0..self.pack_reader.objectc {
            let offset = self.pack_reader.bytes_hashed() as u64;
            let (crc, deltified) = self.pack_reader.read_pack_obj_with_crc()?;
            let raw_pack_obj = self.expand_deltas(deltified, offset)?;
            let oid = raw_pack_obj.oid();
            self.oid_to_offset.insert(oid, offset);
            self.raw_objects.insert(offset, raw_pack_obj);
            self.sorted.insert(oid, (offset, crc));
        }

        let mut reader = self.pack_reader.reader;
        let pack_hash = reader.finalize_sha1();
        let expected_hash = reader.read_oid()?;
        assert!(reader.is_at_eof()?);
        ensure_eq!(
            expected_hash,
            pack_hash,
            "corrupted packfile: expected hash of `{}` found `{}`",
            expected_hash,
            pack_hash
        );

        let mut oids = Vec::with_capacity(n);
        let mut offsets = Vec::with_capacity(n);
        let mut crcs = Vec::with_capacity(n);
        for (oid, (offset, crc)) in self.sorted {
            oids.push(oid);
            crcs.push(crc);
            assert!(offset < MAX_OFFSET, "todo ext layer");
            offsets.push(offset as u32);
        }
        let fanout = PackIndex::build_fanout(&oids);
        Ok(PackIndex { fanout, oids, crcs, offsets, pack_hash })
    }

    fn expand_deltas(
        &mut self,
        obj: BitPackObjRawDeltified,
        base_offset: u64,
    ) -> BitResult<BitPackObjRaw> {
        let (offset, delta) = match obj {
            BitPackObjRawDeltified::Raw(raw) => return Ok(raw),
            BitPackObjRawDeltified::Ofs(offset, delta) => (base_offset - offset, delta),
            BitPackObjRawDeltified::Ref(base_oid, delta) => (self.oid_to_offset[&base_oid], delta),
        };
        let base = &self.raw_objects[&offset];
        base.expand_with_delta_bytes(&delta)
    }
}
