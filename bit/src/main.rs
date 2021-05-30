mod cli;
mod util;

#[cfg(test)]
#[macro_use]
mod tests;

#[macro_use]
#[cfg(test)]
extern crate pretty_assertions;

#[macro_use]
extern crate anyhow;

#[macro_use]
extern crate libbit;

pub fn main() -> libbit::error::BitResult<()> {
    env_logger::builder().parse_env("BIT_LOG").init();
    cli::run()
}
