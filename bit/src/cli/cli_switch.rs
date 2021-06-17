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
        dbg!(repo.partially_resolve_ref(self.branch)?);
        dbg!(&self);
        todo!()
    }
}
