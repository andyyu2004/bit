use crate::error::BitResult;
use crate::repo::BitRepo;

pub struct BitLsFilesOpts {
    pub stage: bool,
}

impl BitRepo {
    pub fn bit_ls_files(self, opts: BitLsFilesOpts) -> BitResult<()> {
        let index = self.index()?;
        index.std_iter().for_each(|entry| {
            if opts.stage {
                print!("{} {} {}\t", entry.mode, entry.oid, entry.stage());
            }
            println!("{}", entry.path);
        });
        Ok(())
    }
}
