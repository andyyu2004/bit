use super::PackfileReader;
use crate::error::BitResult;
use crate::io::{BufReadExt, HashReader, ReadExt};
use crate::obj::Oid;
use crate::repo::BitRepo;
use sha1::{Digest, Sha1};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;

impl<'rcx> BitRepo<'rcx> {
    /// Builds a pack index file (<name>.idx) from the specified `<name>.pack` file.
    pub fn index_pack(self, path: impl AsRef<Path>) -> BitResult<()> {
        dbg!(path.as_ref());
        let reader = BufReader::new(File::open(&path)?);
        let indexer = PackIndexer::new(self, reader)?;
        indexer.index_pack()?;
        // removing for now
        std::fs::remove_file(&path)?;
        todo!()
    }
}

pub(super) struct PackIndexer<'rcx, R> {
    repo: BitRepo<'rcx>,
    pack_reader: PackfileReader<HashReader<Sha1, R>>,
}

impl<'rcx, R: BufRead> PackIndexer<'rcx, R> {
    pub fn new(repo: BitRepo<'rcx>, reader: R) -> BitResult<Self> {
        let hash_reader = HashReader::new_sha1(reader);
        Ok(Self { repo, pack_reader: PackfileReader::new(hash_reader)? })
    }

    pub fn index_pack(mut self) -> BitResult<()> {
        for i in 0..self.pack_reader.objectc {
            print!("\r{}", i);
            let raw_pack_obj = self.pack_reader.read_pack_obj()?;
            dbg!(raw_pack_obj);
        }

        let mut reader = self.pack_reader.reader;
        let actual_hash = reader.finalize_sha1_hash();
        let expected_hash = reader.read_oid()?;
        assert!(reader.is_at_eof()?);
        ensure_eq!(
            expected_hash,
            actual_hash,
            "corrupted packfile: expected hash of `{}` found `{}`",
            expected_hash,
            actual_hash
        );
        Ok(())
    }

    pub fn commit(&mut self) -> BitResult<()> {
        // rename the tmp file etc
        todo!()
    }
}
