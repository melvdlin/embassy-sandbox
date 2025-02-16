#![no_std]
#![feature(new_range_api)]
#![feature(sync_unsafe_cell)]
#![feature(impl_trait_in_assoc_type)]
#![feature(layout_for_ptr)]

#[cfg(any())]
pub mod bitbang;

#[cfg(any())]
pub mod flash;

#[cfg(feature = "cross")]
pub mod tftp;

pub mod util;

pub mod cli;
pub mod log;

pub mod net;
pub mod sdram;

pub mod graphics;
