mod cli;
mod util;

#[macro_use]
extern crate anyhow;

pub fn main() -> libbit::error::BitResult<()> {
    if let Err(err) = cli::run() {
        eprintln!("{}", err)
    }

    Ok(())
}
