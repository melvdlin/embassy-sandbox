#![no_std]
#![feature(new_range_api)]
#![feature(sync_unsafe_cell)]
#![feature(impl_trait_in_assoc_type)]
#![feature(layout_for_ptr)]
#![feature(slice_range)]
#![feature(slice_ptr_get)]
#![feature(raw_slice_split)]
#![feature(non_null_from_ref)]
#![feature(unsigned_is_multiple_of)]
#![feature(iter_next_chunk)]
#![feature(iter_array_chunks)]
#![feature(array_chunks)]
#![feature(breakpoint)]
#![feature(pointer_is_aligned_to)]
#![feature(strict_overflow_ops)]

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
