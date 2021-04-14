#![feature(never_type)]
#![feature(decl_macro)]
#![feature(once_cell)]
#![feature(trait_alias)]
#![feature(destructuring_assignment)]
#![feature(type_name_of_val)]
#![feature(map_first_last)]
#![feature(is_sorted)]
#![feature(cstring_from_vec_with_nul)]
#![feature(array_methods)]
#![feature(with_options)]

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

#[macro_use]
extern crate scoped_tls;

#[macro_use]
extern crate anyhow;

#[cfg(test)]
mod test_utils;

pub mod cmd;
pub mod config;
pub mod error;
pub mod hash;
pub mod obj;
pub mod path;
pub mod pathspec;
pub mod repo;

mod diff;
mod index;
mod interner;
mod io_ext;
mod iter;
mod lockfile;
mod odb;
mod serialize;
mod signature;
mod tls;
mod util;
