use crate::error::BitResult;
use crate::repo::BitRepo;

pub struct BitLsFilesOpts {
    pub stage: bool,
}

impl BitRepo {
    pub fn bit_ls_files(&self, opts: BitLsFilesOpts) -> BitResult<()> {
        self.with_index(|index| {
            index.entries.values().for_each(|entry| {
                if opts.stage {
                    print!("{} {} {}\t", entry.mode, entry.hash, entry.stage())
                }
                println!("{}", entry.filepath)
            });
            Ok(())
        })
    }
}
