use crate::cli::Cmd;
use clap::Clap;
use libbit::error::BitResult;
use libbit::repo::BitRepo;
use libbit::reset::ResetKind;
use libbit::rev::Revspec;

// soft,mixed,hard all conflict with each other which I've declared in a circular manner for less noise
#[derive(Clap, Debug)]
pub struct BitResetCliOpts {
    target: Revspec,
    #[clap(long = "--soft", conflicts_with("mixed"))]
    soft: bool,
    #[clap(long = "--mixed", conflicts_with("hard"))]
    mixed: bool,
    #[clap(long = "--hard", conflicts_with("soft"))]
    hard: bool,
}

impl Cmd for BitResetCliOpts {
    fn exec(self, repo: BitRepo<'_>) -> BitResult<()> {
        // assert exactly zero or one of them are true
        assert!((self.soft as u8 + self.mixed as u8 + self.hard as u8) < 2);
        let kind = if self.soft {
            ResetKind::Soft
        } else if self.hard {
            ResetKind::Hard
        } else {
            // defaults to mixed reset
            ResetKind::Mixed
        };

        repo.reset(&self.target, kind)?;
        println!("HEAD is now at `{}`", repo.resolve_head()?);
        Ok(())
    }
}
