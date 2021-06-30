use crate::error::BitResult;
use crate::repo::BitRepo;

pub struct BitLsFilesOpts {
    pub stage: bool,
}

impl<'rcx> BitRepo<'rcx> {
    pub fn bit_ls_files(&self, opts: BitLsFilesOpts) -> BitResult<()> {
        self.with_index(|index| {
            index.std_iter().for_each(|entry| {
                if opts.stage {
                    print!("{} {} {}\t", entry.mode, entry.oid, entry.stage());
                }
                println!("{}", entry.path);
            });
            Ok(())
        })
    }
}
