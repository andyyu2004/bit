#![feature(never_type)]
#![feature(decl_macro)]

pub mod cli;
pub mod cmd;
mod error;
mod obj;
mod repo;

pub use error::{BitError, BitResult};
pub use repo::BitRepo;
