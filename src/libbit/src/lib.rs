#![feature(never_type)]
#![feature(decl_macro)]
#![feature(destructuring_assignment)]
#![feature(array_methods)]

#[cfg(test)]
extern crate quickcheck;

#[cfg(test)]
#[macro_use(quickcheck)]
extern crate quickcheck_macros;

#[cfg(test)]
mod test_utils;

pub mod cmd;

pub mod error;
mod hash;
pub mod obj;
pub mod repo;
