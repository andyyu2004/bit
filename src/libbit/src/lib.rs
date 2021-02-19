#![feature(never_type)]
#![feature(decl_macro)]

#[cfg(test)]
extern crate quickcheck;
#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

pub mod cli;
pub mod cmd;
mod error;
mod obj;
mod repo;

pub use error::{BitError, BitResult};
pub use repo::BitRepo;
