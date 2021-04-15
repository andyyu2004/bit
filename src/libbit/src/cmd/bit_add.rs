use crate::error::BitResult;
use crate::pathspec::Pathspec;
use crate::repo::BitRepo;
use crate::tls;
use fallible_iterator::FallibleIterator;

#[derive(Debug)]
pub struct BitAddOpts {
    pub pathspecs: Vec<Pathspec>,
    pub dryrun: bool,
}

impl Default for BitAddOpts {
    fn default() -> Self {
        Self { pathspecs: Default::default(), dryrun: false }
    }
}

impl BitAddOpts {
    pub fn add_pathspec(self, pathspec: Pathspec) -> Self {
        let mut pathspecs = self.pathspecs;
        pathspecs.push(pathspec);
        Self { pathspecs, ..self }
    }
}

impl BitRepo {
    pub fn bit_add(&self, opts: BitAddOpts) -> BitResult<()> {
        tls::with_index_mut(|index| {
            dbg!(&index);
            for pathspec in opts.pathspecs {
                if opts.dryrun {
                    pathspec
                        .match_worktree()?
                        .for_each(|entry| Ok(println!("add `{}`", entry.filepath)))?;
                } else {
                    index.add(&pathspec)?;
                }
            }
            dbg!(&index);
            Ok(())
        })
    }
}
