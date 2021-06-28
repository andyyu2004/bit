use super::*;

/// representation of the index file
// refer to https://github.com/git/git/blob/master/Documentation/technical/index-format.txt
// for the format of the index
/// WARNING: this struct *must not* have any interior mutability inside it
/// as it is stored inside a [crate::lockfile::Filelock]
#[derive(Debug, PartialEq, Clone, Default)]
#[cfg_attr(test, derive(BitArbitrary))]
pub struct BitIndexInner {
    /// sorted by ascending by filepath (interpreted as unsigned bytes)
    /// ties broken by stage (a part of flags)
    // the link says `name` which usually refers to the hash
    // but it is sorted by filepath
    // DO NOT mutate this field directly
    // instead use one of the mutators
    entries: BitIndexEntries,
    tree_cache: Option<BitTreeCache>,
    reuc: Option<BitReuc>,
}

impl BitIndexInner {
    pub fn new(
        entries: BitIndexEntries,
        tree_cache: Option<BitTreeCache>,
        reuc: Option<BitReuc>,
    ) -> Self {
        Self { entries, tree_cache, reuc }
    }

    pub fn tree_iter(&self) -> IndexTreeIter<'_> {
        IndexTreeIter::new(self)
    }

    pub fn tree_cache(&self) -> Option<&BitTreeCache> {
        self.tree_cache.as_ref()
    }

    pub fn entries(&self) -> &BitIndexEntries {
        &self.entries
    }

    pub fn insert_entry(&mut self, entry: BitIndexEntry) {
        self.entries.insert(entry.key(), entry);
        self.invalidate_tree_cache_path(entry.path)
    }

    pub fn remove_entry(&mut self, key @ (path, _): (BitPath, MergeStage)) {
        self.entries.remove(&key);
        self.invalidate_tree_cache_path(path)
    }

    pub fn invalidate_tree_cache_path(&mut self, path: BitPath) {
        if let Some(tree_cache) = &mut self.tree_cache {
            tree_cache.invalidate_path(path)
        }
    }

    pub fn std_iter(&self) -> IndexStdIterator {
        // this is pretty nasty, but I'm uncertain of a better way to dissociate the lifetime of
        // `self` from the returned iterator
        self.entries.values().cloned().collect_vec().into_iter()
    }

    pub fn iter(&self) -> IndexEntryIterator {
        fallible_iterator::convert(self.std_iter().map(Ok))
    }

    /// find entry by path
    pub fn find_entry(&self, path: BitPath, stage: MergeStage) -> Option<BitIndexEntry> {
        self.entries.get(&(path, stage)).copied()
    }

    /// removes collisions where there was originally a file but was replaced by a directory
    fn remove_file_dir_collisions(&mut self, entry: &BitIndexEntry) -> BitResult<()> {
        //? only removing entries with no merge stage (may need changes)
        for component in entry.path.cumulative_components() {
            self.remove_entry((component, MergeStage::None));
        }
        Ok(())
    }

    /// removes collisions where there was originally a directory but was replaced by a file
    fn remove_dir_file_collisions(&mut self, index_entry: &BitIndexEntry) -> BitResult<()> {
        //? unsure which implementation is better
        // doesn't seem to be a nice way to remove a range of a btreemap
        // self.entries.retain(|(path, _), _| !path.starts_with(index_entry.path));
        let mut to_remove = vec![];

        for (&(path, stage), _) in self.entries.range((index_entry.path, MergeStage::None)..) {
            if !path.starts_with(index_entry.path) {
                break;
            }
            to_remove.push((path, stage));
        }

        for key in to_remove {
            self.remove_entry(key)
        }
        Ok(())
    }

    /// remove directory/file and file/directory collisions that are possible in the index
    pub(super) fn remove_collisions(&mut self, entry: &BitIndexEntry) -> BitResult<()> {
        self.remove_file_dir_collisions(entry)?;
        self.remove_dir_file_collisions(entry)?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn has_conflicts(&self) -> bool {
        self.entries.keys().any(|(_, stage)| stage.is_merging())
    }
}

impl BitIndexInner {
    pub(super) fn parse_header(mut r: impl BufRead) -> BitResult<BitIndexHeader> {
        let mut signature = [0u8; 4];
        r.read_exact(&mut signature)?;
        assert_eq!(&signature, BIT_INDEX_HEADER_SIG);
        let version = r.read_u32()?;
        ensure!(
            version == 2 || version == 3,
            "Only index formats v2 and v3 are supported (found version `{}`)",
            version
        );
        let entryc = r.read_u32()?;

        Ok(BitIndexHeader { signature, version, entryc })
    }

    fn parse_extensions(mut buf: &[u8]) -> BitResult<HashMap<[u8; 4], BitIndexExtension>> {
        let mut extensions = HashMap::new();
        while buf.len() > BIT_HASH_SIZE {
            let signature: [u8; 4] = buf[0..4].try_into().unwrap();
            let size = u32::from_be_bytes(buf[4..8].try_into().unwrap());
            let data = buf[8..8 + size as usize].to_vec();
            extensions.insert(signature, BitIndexExtension { signature, size, data });
            buf = &buf[8 + size as usize..];
        }
        Ok(extensions)
    }
}

impl Serialize for BitIndexInner {
    fn serialize(&self, writer: &mut dyn Write) -> BitResult<()> {
        let mut hash_writer = HashWriter::new_sha1(writer);

        let header = BitIndexHeader {
            signature: *BIT_INDEX_HEADER_SIG,
            version: BIT_INDEX_VERSION,
            entryc: self.entries.len() as u32,
        };
        header.serialize(&mut hash_writer)?;

        for entry in self.entries.values() {
            entry.serialize(&mut hash_writer)?;
        }

        if let Some(tree_cache) = &self.tree_cache {
            hash_writer.write_all(BIT_INDEX_TREECACHE_SIG)?;
            hash_writer.write_with_size(tree_cache)?;
        }

        if let Some(reuc) = &self.reuc {
            hash_writer.write_all(BIT_INDEX_REUC_SIG)?;
            hash_writer.write_with_size(reuc)?;
        }

        let hash = hash_writer.finalize_sha1_hash();
        hash_writer.write_oid(hash)?;
        Ok(())
    }
}

impl Deserialize for BitIndexInner {
    fn deserialize(mut r: impl BufRead) -> BitResult<Self>
    where
        Self: Sized,
    {
        // this impl currently is not ideal as it basically has to read it twice
        // although the second time is reading from memory so maybe its not that bad?
        // its a bit awkward to use hashreader to read the extensions because we don't
        // know how long the extensions are
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;

        let mut r = BufReader::new(&buf[..]);
        let header = Self::parse_header(&mut r)?;
        let entries = (0..header.entryc)
            .map(|_| BitIndexEntry::deserialize(&mut r))
            .collect::<Result<BitIndexEntries, _>>()?;

        let mut remainder = vec![];
        assert!(r.read_to_end(&mut remainder)? >= BIT_HASH_SIZE);

        let mut extensions = Self::parse_extensions(&remainder)?;

        let tree_cache = extensions
            .remove(BIT_INDEX_TREECACHE_SIG)
            .map(|ext| BitTreeCache::deserialize_unbuffered(&ext.data[..]))
            .transpose()?;

        let reuc = extensions
            .remove(BIT_INDEX_REUC_SIG)
            .map(|ext| BitReuc::deserialize_unbuffered(&ext.data[..]))
            .transpose()?;

        debug_assert!(
            extensions.is_empty(),
            "unhandled index extension (its fine to ignore in practice as no extension is mandantory but its good to know)"
        );

        let bit_index = Self::new(entries, tree_cache, reuc);

        let (bytes, hash) = buf.split_at(buf.len() - BIT_HASH_SIZE);
        let mut hasher = sha1::Sha1::new();
        hasher.update(bytes);
        let actual_hash = Oid::from(hasher.finalize());
        let expected_hash = Oid::new(hash.try_into().unwrap());
        ensure_eq!(actual_hash, expected_hash, "corrupted index (bad hash)");

        Ok(bit_index)
    }
}
