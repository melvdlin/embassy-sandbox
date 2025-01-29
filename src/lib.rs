#![no_std]
#![feature(new_range_api)]
#![allow(clippy::manual_range_patterns)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(sync_unsafe_cell)]
#![deny(unused_must_use)]

#[cfg(any())]
pub mod bitbang;
#[cfg(any())]
pub mod flash;
#[cfg(feature = "cross")]
pub mod tftp;

pub mod util;

pub mod log;
