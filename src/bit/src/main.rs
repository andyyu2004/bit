mod cli;
mod util;

#[macro_use]
extern crate anyhow;

pub fn main() -> libbit::error::BitResult<()> {
    env_logger::builder().parse_env("BIT_LOG").init();
    if let Err(err) = cli::run() {
        eprintln!("{}", err)
    }

    Ok(())
}
