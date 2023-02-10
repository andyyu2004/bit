use clap::Parser;
use libbit::error::BitResult;
use libbit::pack::{IndexPackOpts, PackIndexer};
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct BitIndexPackCliOpts {
    /// Write the generated pack index into the specified file.
    /// Without this option the name of pack index file is constructed from the name of packed archive file by replacing .pack with .idx (and the program fails if the name of packed archive does not end with .pack).
    #[arg(short = 'o')]
    index_file: Option<PathBuf>,
    /// Path of the pack-file
    path: PathBuf,
    #[arg(short = 'v')]
    verbose: bool,
}

impl BitIndexPackCliOpts {
    pub fn exec(self) -> BitResult<()> {
        let Self { path, index_file, verbose } = self;
        let opts = IndexPackOpts { index_file_path: index_file, verbose };
        PackIndexer::write_pack_index(path, opts)?;
        Ok(())
    }
}
