use super::{BitPackObjRawKind, PackfileReader};
use crate::error::BitResult;
use crate::io::{BufReadExt, HashReader, ReadExt};
use crate::obj::{BitPackObjRaw, Oid};
use crate::pack::{PackIndex, MAX_OFFSET, PACK_IDX_EXT};
use crate::repo::BitRepo;
use crate::serialize::Serialize;
use rustc_hash::FxHashMap;
use sha1::Sha1;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

impl<'rcx> BitRepo<'rcx> {
    /// Builds a pack index file (<name>.idx) from the specified `<name>.pack` file.
    /// Overwrites any existing file at the output path
    pub fn index_pack(self, path: impl AsRef<Path>) -> BitResult<()> {
        let path = path.as_ref();
        dbg!(&path);
        let reader = BufReader::new(File::open(&path)?);
        let indexer = PackIndexer::new(reader)?;
        let pack_index = indexer.index_pack()?;
        let mut pack_index_file = File::create(path.with_extension(PACK_IDX_EXT))?;
        pack_index.serialize(&mut pack_index_file)?;
        Ok(())
    }
}

pub(crate) struct PackIndexer<R> {
    pack_reader: PackfileReader<HashReader<Sha1, R>>,
    raw_objects: FxHashMap<u64, BitPackObjRaw>,
    oid_to_offset: FxHashMap<Oid, u64>,
    /// oid -> (offset, crc)
    sorted: BTreeMap<Oid, (u64, u32)>,
}

impl<R: BufRead> PackIndexer<R> {
    pub fn new(reader: R) -> BitResult<Self> {
        let hash_reader = HashReader::new_sha1(reader);
        Ok(Self {
            pack_reader: PackfileReader::new(hash_reader)?,
            raw_objects: Default::default(),
            oid_to_offset: Default::default(),
            sorted: Default::default(),
        })
    }

    pub fn index_pack(mut self) -> BitResult<PackIndex> {
        for _ in 0..self.pack_reader.objectc {
            let offset = self.pack_reader.bytes_hashed() as u64;
            let (crc, raw_pack_obj_kind) = self.pack_reader.read_pack_obj_with_crc()?;
            let raw_pack_obj = self.expand_obj(raw_pack_obj_kind, offset)?;
            let oid = raw_pack_obj.oid();
            self.raw_objects.insert(offset, raw_pack_obj);
            self.oid_to_offset.insert(oid, offset);
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

        let n = self.pack_reader.objectc as usize;
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

    fn expand_obj(&mut self, obj: BitPackObjRawKind, base_offset: u64) -> BitResult<BitPackObjRaw> {
        let (offset, delta) = match obj {
            BitPackObjRawKind::Raw(raw) => return Ok(raw),
            BitPackObjRawKind::Ofs(offset, delta) => (base_offset - offset, delta),
            BitPackObjRawKind::Ref(base_oid, delta) => (self.oid_to_offset[&base_oid], delta),
        };
        let base = &self.raw_objects[&offset];
        base.expand_with_delta_bytes(&delta)
    }
}
