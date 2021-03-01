#![feature(never_type)]
#![feature(decl_macro)]
#![feature(destructuring_assignment)]
#![feature(array_methods)]
#![feature(str_split_once)]

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
