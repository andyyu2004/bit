use clap::Clap;
use libbit::error::BitResult;
use libbit::pack::{IndexPackOpts, PackIndexer};
use std::path::PathBuf;

#[derive(Clap, Debug)]
pub struct BitIndexPackCliOpts {
    /// Write the generated pack index into the specified file.
    /// Without this option the name of pack index file is constructed from the name of packed archive file by replacing .pack with .idx (and the program fails if the name of packed archive does not end with .pack).
    #[clap(short = 'o')]
    index_file: Option<PathBuf>,
    /// Path of the pack-file
    path: PathBuf,
}

impl BitIndexPackCliOpts {
    pub fn exec(self) -> BitResult<()> {
        let opts = IndexPackOpts { index_file_path: self.index_file };
        PackIndexer::write_pack_index(&self.path, opts)?;
        Ok(())
    }
}
