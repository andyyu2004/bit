#![feature(iter_intersperse)]

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

pub fn main() -> ! {
    env_logger::builder().parse_env("BIT_LOG").init();
    if let Err(err) = cli::run(std::env::args_os()) {
        eprintln!("{}", err);
        std::process::exit(1)
    } else {
        std::process::exit(0)
    }
}
