#![feature(never_type)]

pub mod cmd;
mod error;
mod obj;
mod repo;

pub use error::{BitError, BitResult};
pub use repo::BitRepo;
