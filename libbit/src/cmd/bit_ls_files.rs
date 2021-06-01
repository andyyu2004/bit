use crate::error::BitResult;
use crate::repo::BitRepo;
use fallible_iterator::FallibleIterator;

pub struct BitLsFilesOpts {
    pub stage: bool,
}

impl<'r> BitRepo<'r> {
    pub fn bit_ls_files(&self, opts: BitLsFilesOpts) -> BitResult<()> {
        self.with_index(|index| {
            index.iter().for_each(|entry| {
                if opts.stage {
                    print!("{} {} {}\t", entry.mode, entry.hash, entry.stage())
                }
                println!("{}", entry.path);
                Ok(())
            })
        })
    }
}
