use super::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::refs::SymbolicRef;
use libbit::repo::BitRepo;

#[derive(Clap, Debug)]
pub struct BitSwitchCliOpts {
    branch: SymbolicRef,
}

impl Cmd for BitSwitchCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        dbg!(self.branch.partially_resolve(repo)?);
        dbg!(&self);
        todo!()
    }
}
