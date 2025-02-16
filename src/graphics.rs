use core::cmp;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::Index;
use core::ops::IndexMut;
use core::ptr::NonNull;

use bytemuck::Pod;

#[derive(Debug)]
pub struct Framebuffer<'buf, P> {
    rows: usize,
    cols: usize,
    buf: NonNull<u8>,
    _phantom: PhantomData<&'buf mut [P]>,
}

impl<'buf, P: Pod> Framebuffer<'buf, P> {
    pub const fn new(buf: &'buf mut [MaybeUninit<u8>], rows: usize, cols: usize) -> Self {
        // FIXME: change to `assert_eq` once `assert_eq` is const
        assert!(buf.len() == rows * cols * size_of::<P>());
        if buf.is_empty() {
            Self::empty()
        } else {
            Self {
                rows,
                cols,
                // Safety:
                // buf is a valid, non-empty slice, and therefore not null
                buf: unsafe { NonNull::new_unchecked(buf.as_mut_ptr().cast()) },
                _phantom: PhantomData,
            }
        }
    }

    pub const fn empty() -> Self {
        Self {
            rows: 0,
            cols: 0,
            buf: NonNull::dangling(),
            _phantom: PhantomData,
        }
    }
}

impl<P: Pod> Default for Framebuffer<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug)]
pub struct Row<'buf, P> {
    buf: NonNull<u8>,
    cols: usize,
    _phantom: PhantomData<&'buf mut [P]>,
}

#[derive(Debug)]
pub struct Pixel<'buf, P> {
    // `buf` must be valid for writes
    // and point to an allocation at least as large as P
    buf: NonNull<u8>,
    _phantom: PhantomData<&'buf mut P>,
}

impl<P: Pod> Pixel<'_, P> {
    pub fn write(&mut self, data: P) {
        // Safety:
        // `self.buf` is valid for writes
        // and point to an allocation at least as large as P
        unsafe { aligned_volatile_copy(bytemuck::bytes_of(&data), self.buf) };
    }

    pub fn write_bytes(&mut self, data: &[u8]) {
        assert!(data.len() <= size_of::<P>());
        // Safety:
        // `self.buf` is valid for writes
        // and point to an allocation at least as large as `P`;
        // we asserted that `buf` is no longer than `size_of::<P>()`
        unsafe { aligned_volatile_copy(data, self.buf) };
    }
}

/// Safety:
/// if `src` is non-empty,
/// then `dst` must be valid for writes and
/// point to an allocation of at least `src.len()` bytes
unsafe fn aligned_volatile_copy(src: &[u8], dst: NonNull<u8>) {
    let head_len = cmp::min(dst.align_offset(size_of::<u32>()), src.len());
    let body_len = (src.len() - head_len) / size_of::<u32>();
    let tail_len = (src.len() - head_len) % size_of::<u32>();

    let mut src = src.as_ptr();
    let mut dst = dst.as_ptr();
    for _ in 0..head_len {
        unsafe {
            // Safety: trust me bro
            dst.write_volatile(*src);
        }
        src = src.wrapping_add(1);
        dst = dst.wrapping_add(1);
    }

    let mut src = src.cast::<[u8; 4]>();
    let mut dst = dst.cast::<u32>();
    for _ in 0..body_len {
        unsafe {
            // Safety: trust me bro
            dst.write_volatile(u32::from_ne_bytes(*src));
        }
        src = src.wrapping_add(1);
        dst = dst.wrapping_add(1);
    }

    let mut src = src.cast::<u8>();
    let mut dst = dst.cast::<u8>();
    for _ in 0..tail_len {
        unsafe {
            // Safety: trust me bro
            dst.write_volatile(*src);
        }
        src = src.wrapping_add(1);
        dst = dst.wrapping_add(1);
    }
}
