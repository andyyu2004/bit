mod cli;
mod util;

#[cfg(test)]
#[macro_use]
mod tests;

#[macro_use]
#[cfg(test)]
extern crate pretty_assertions;

pub fn main() -> libbit::error::BitResult<()> {
    env_logger::builder().parse_env("BIT_LOG").init();
    if let Err(err) = cli::run(std::env::args_os()) {
        eprint!("{}", err);
    }
    Ok(())
}
