#![feature(never_type)]
#![feature(decl_macro)]
#![feature(type_alias_impl_trait)]
#![feature(once_cell)]
#![feature(destructuring_assignment)]
#![feature(map_first_last)]
#![feature(is_sorted)]
#![feature(cstring_from_vec_with_nul)]
#![feature(array_methods)]

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

#[macro_use]
extern crate scoped_tls;

#[cfg(test)]
mod test_utils;

pub mod cmd;
pub mod error;
pub mod obj;
pub mod path;
pub mod pathspec;
pub mod repo;

mod diff;
mod hash;
mod index;
mod interner;
mod io_ext;
mod serialize;
mod tls;
mod util;
