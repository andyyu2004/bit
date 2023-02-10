use super::*;
use arrayvec::ArrayVec;
use indexmap::IndexMap;
use std::io::BufWriter;

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
    pub(super) tree_cache: Option<BitTreeCache>,
    reuc: Option<BitReuc>,
}

/// Stores all the conflicts in the index ordered by path
pub type Conflicts = Vec<Conflict>;

#[derive(Debug, PartialEq)]
pub struct Conflict {
    pub path: BitPath,
    pub conflict_type: ConflictType,
}

impl Conflict {
    /// `stages` are what stages exist for `path` in the index
    pub fn new((path, stages): (BitPath, ArrayVec<MergeStage, 3>)) -> Self {
        Self { path, conflict_type: ConflictType::new(stages) }
    }

    #[cfg(test)]
    pub fn new_with_type(path: impl AsRef<std::ffi::OsStr>, conflict_type: ConflictType) -> Self {
        Self { path: BitPath::intern(path), conflict_type }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ConflictType {
    BothModified,
    BothAdded,
    ModifyDelete,
    DeleteModify,
    AddedByUs,
    AddedByThem,
}

impl ConflictType {
    fn new(stages: ArrayVec<MergeStage, 3>) -> Self {
        match &stages[..] {
            [MergeStage::Base, MergeStage::Ours, MergeStage::Theirs] => Self::BothModified,
            [MergeStage::Ours, MergeStage::Theirs] => Self::BothAdded,
            [MergeStage::Base, MergeStage::Ours] => Self::ModifyDelete,
            [MergeStage::Base, MergeStage::Theirs] => Self::DeleteModify,
            [MergeStage::Ours] => Self::AddedByUs,
            [MergeStage::Theirs] => Self::AddedByThem,
            _ => unreachable!("probably missing some cases `{:?}`", stages),
        }
    }
}

impl Display for Conflict {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}:\t{}", self.conflict_type, self.path)
    }
}

impl Display for ConflictType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ConflictType::BothModified => "both modified",
            ConflictType::BothAdded => "both added",
            ConflictType::ModifyDelete => "deleted by them",
            ConflictType::DeleteModify => "deleted by us",
            ConflictType::AddedByUs => "added by us",
            ConflictType::AddedByThem => "added by them",
        };
        write!(f, "{msg}")
    }
}

impl BitIndexInner {
    pub fn new(
        entries: BitIndexEntries,
        tree_cache: Option<BitTreeCache>,
        reuc: Option<BitReuc>,
    ) -> Self {
        Self { entries, tree_cache, reuc }
    }

    pub fn index_tree_iter(&self) -> IndexTreeIter<'_> {
        IndexTreeIter::new(self)
    }

    #[inline]
    pub fn tree_cache(&self) -> Option<&BitTreeCache> {
        self.tree_cache.as_ref()
    }

    pub fn entries(&self) -> &BitIndexEntries {
        &self.entries
    }

    pub(super) fn insert_entry(&mut self, entry: BitIndexEntry) {
        self.entries.insert(entry.key(), entry);
        self.invalidate_tree_cache_path(entry.path)
    }

    // remove an entry with the given key if it exists
    pub fn remove_entry(&mut self, key @ (path, _): (BitPath, MergeStage)) {
        let exists = self.entries.remove(&key).is_some();
        if exists {
            self.invalidate_tree_cache_path(path)
        }
    }

    fn invalidate_tree_cache_path(&mut self, path: BitPath) {
        if let Some(tree_cache) = &mut self.tree_cache {
            tree_cache.invalidate_path(path)
        }
    }

    pub(super) fn remove_conflicted(&mut self, path: BitPath) {
        self.remove_entry((path, MergeStage::Base));
        self.remove_entry((path, MergeStage::Ours));
        self.remove_entry((path, MergeStage::Theirs));
    }

    pub fn conflicts(&self) -> Conflicts {
        let mut conflict_map = IndexMap::<BitPath, ArrayVec<MergeStage, 3>>::new();
        self.entries.values().filter(|entry| entry.stage().is_unmerged()).for_each(|entry| {
            conflict_map.entry(entry.path).or_default().push(entry.stage());
        });

        conflict_map.into_iter().map(Conflict::new).collect()
    }

    pub fn std_iter(&self) -> IndexStdIterator {
        // This is pretty nasty, but I'm uncertain of a better way to dissociate the lifetime of
        // `self` from the returned iterator
        // Filtering out submodules
        self.entries.values().filter(|entry| !entry.is_gitlink()).copied().collect_vec().into_iter()
    }

    pub fn iter(&self) -> IndexEntryIterator {
        fallible_iterator::convert(self.std_iter().map(Ok))
    }

    /// Find entry by path and stage
    pub fn find_entry(&self, key: (BitPath, MergeStage)) -> Option<&BitIndexEntry> {
        self.entries.get(&key)
    }

    /// removes collisions where there was originally a file but was replaced by a directory
    fn remove_file_dir_collisions(&mut self, path: BitPath) -> BitResult<()> {
        //? only removing entries with no merge stage (may need changes)
        for component in path.cumulative_components() {
            self.remove_entry((component, MergeStage::None));
        }
        Ok(())
    }

    /// remove directory and all subentries (recursively)
    pub fn remove_directory(&mut self, entry_path: BitPath) -> BitResult<()> {
        debug_assert!(entry_path.is_relative());

        let subentries = self
            .entries
            // We need the do the trailing slash hack to create the correct range
            .range((entry_path.join(""), MergeStage::None)..)
            .take_while(|(&(path, _), _)| path.starts_with(entry_path))
            .filter(|((_, stage), _)| *stage == MergeStage::None)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();

        for key in subentries {
            self.remove_entry(key);
        }

        Ok(())
    }

    /// removes collisions where there was originally a directory but was replaced by a file
    // implemented by just removing the directory
    fn remove_dir_file_collisions(&mut self, path: BitPath) -> BitResult<()> {
        // there is a collision if there exists an entry in the index whose path starts with the new file's path
        let has_collision = self
            .entries
            .range(
                (path, MergeStage::None)
                    ..(path.approximate_lexicographical_successor(), MergeStage::None),
            )
            .skip(1)
            .any(|((p, _), _)| p.starts_with(path));

        if has_collision {
            self.remove_directory(path)?;
        }
        Ok(())
    }

    /// remove directory/file and file/directory collisions that are possible in the index
    pub(super) fn remove_collisions(&mut self, path: BitPath) -> BitResult<()> {
        self.remove_file_dir_collisions(path)?;
        self.remove_dir_file_collisions(path)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn has_conflicts(&self) -> bool {
        self.entries.keys().any(|(_, stage)| stage.is_unmerged())
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
        while buf.len() > OID_SIZE {
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
        let mut hash_writer = BufWriter::new(HashWriter::new_sha1(writer));

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

        match hash_writer.into_inner() {
            Ok(writer) => writer.write_hash()?,
            Err(..) => bail!("hash writer flush failed while writing index"),
        };
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
        assert!(r.read_to_end(&mut remainder)? >= OID_SIZE);

        let mut extensions = Self::parse_extensions(&remainder)?;

        let tree_cache = extensions
            .remove(BIT_INDEX_TREECACHE_SIG)
            .map(|ext| BitTreeCache::deserialize(&ext.data[..]))
            .transpose()?;

        let reuc = extensions
            .remove(BIT_INDEX_REUC_SIG)
            .map(|ext| BitReuc::deserialize(&ext.data[..]))
            .transpose()?;

        debug_assert!(
            extensions.is_empty(),
            "unhandled index extension (its fine to ignore in practice as no extension is mandantory but its good to know)"
        );

        let bit_index = Self::new(entries, tree_cache, reuc);

        let (bytes, hash) = buf.split_at(buf.len() - OID_SIZE);
        let mut hasher = sha1::Sha1::new();
        hasher.update(bytes);
        let actual_hash = Oid::from(hasher.finalize());
        let expected_hash = Oid::new(hash.try_into().unwrap());
        ensure_eq!(actual_hash, expected_hash, "corrupted index (bad hash)");

        Ok(bit_index)
    }
}
