#![feature(never_type)]
#![feature(decl_macro)]
#![feature(destructuring_assignment)]
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
pub mod repo;

mod hash;
mod tls;
