#![deny(rust_2018_idioms)]
#![feature(pattern)]
#![feature(never_type)]
#![feature(const_transmute_copy)]
#![feature(bindings_after_at)]
#![feature(exact_size_is_empty)]
#![feature(maybe_uninit_uninit_array, maybe_uninit_slice)]
#![feature(associated_type_defaults)]
#![feature(min_type_alias_impl_trait)]
#![feature(decl_macro)]
#![feature(once_cell)]
#![feature(bool_to_option)]
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
extern crate maplit;

#[macro_use]
extern crate num_derive;

#[macro_use]
extern crate bit_derive;

#[macro_use]
extern crate anyhow;

#[cfg(test)]
#[macro_use]
mod test_utils;
mod peel;

#[macro_use]
extern crate log;

#[macro_use]
mod macros;

#[macro_use]
mod debug;

pub mod checkout;
pub mod cmd;
pub mod config;
pub mod diff;
pub mod error;
pub mod hash;
pub mod index;
pub mod obj;
pub mod path;
pub mod pathspec;
pub mod refs;
pub mod repo;
pub mod rev;
pub mod serialize;
pub mod status;
pub mod xdiff;

mod core;
mod delta;
mod interner;
mod io;
mod iter;
mod lockfile;
mod odb;
mod pack;
mod signature;
mod time;
mod tls;
mod util;
