#![deny(rust_2018_idioms)]
#![feature(async_closure)]
#![feature(associated_type_bounds)]
#![feature(iter_intersperse)]
#![feature(hash_raw_entry)]
#![feature(thread_local)]
#![feature(pattern)]
#![feature(never_type)]
#![feature(const_transmute_copy)]
#![feature(exact_size_is_empty)]
#![feature(maybe_uninit_uninit_array, maybe_uninit_slice)]
#![feature(associated_type_defaults)]
#![feature(type_alias_impl_trait)]
#![feature(decl_macro)]
#![feature(once_cell_try)]
#![feature(trait_alias)]
#![feature(type_name_of_val)]
#![feature(is_sorted)]
#![feature(array_methods)]
#![feature(lazy_cell)]

extern crate self as libbit;

#[macro_use]
extern crate async_trait;

#[macro_use]
extern crate smallvec;

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

#[cfg(test)]
#[macro_use]
extern crate indexmap;

#[macro_use]
extern crate maplit;

#[macro_use]
extern crate bitflags;

#[macro_use]
extern crate num_derive;

#[macro_use]
extern crate bit_derive;

#[macro_use]
extern crate anyhow;

#[cfg(test)]
#[macro_use]
pub mod test_utils;
mod cache;
mod cherrypick;
mod fs;
mod graph;
mod protocol;
mod transport;

#[macro_use]
extern crate log;

#[macro_use]
mod macros;

#[macro_use]
mod debug;

pub mod checkout;
pub mod cmd;
pub mod commit;
pub mod config;
pub mod diff;
pub mod error;
pub mod format;
pub mod hash;
pub mod index;
pub mod iter;
pub mod merge;
pub mod obj;
pub mod pack;
pub mod path;
pub mod pathspec;
pub mod refs;
pub mod repo;
pub mod reset;
pub mod rev;
pub mod serialize;
pub mod status;

pub mod remote;
pub mod upload_pack;
pub mod xdiff;

mod core;
mod delta;
mod interner;
mod io;
mod lockfile;
mod odb;
mod peel;
mod signature;
mod time;
mod tls;
