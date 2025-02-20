use core::cmp;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::ops::RangeBounds;
use core::ptr::NonNull;
use core::slice;

use bytemuck::Pod;

/// A row-major framebuffer backed by a byte slice.
/// Access to the backing memory is volatile and word/byte-aligned.
#[derive(Debug)]
pub struct Framebuffer<'buf, P> {
    rows: usize,
    cols: usize,
    /// # Safety:
    ///
    /// `buf` must be valid for writes and `len` must be in-bounds.
    /// See [core::ptr] for details.
    ///
    /// Additionally, `len` must be equal to `rows * cols * size_of::<P>()`.
    buf: NonNull<[u8]>,
    _phantom: PhantomData<&'buf mut [P]>,
}

impl<'buf, P: Pod> Framebuffer<'buf, P> {
    /// Create a new framebuffer backed by the provided buffer.
    ///
    /// # Panics
    ///
    /// Panics if `buf.len() != rows * cols * size_of::<P>()`.
    pub const fn new(buf: &'buf mut [MaybeUninit<u8>], rows: usize, cols: usize) -> Self {
        // FIXME: change to `assert_eq` once `assert_eq` is const
        assert!(buf.len() == rows * cols * size_of::<P>());
        if buf.is_empty() {
            Self::empty()
        } else {
            Self {
                rows,
                cols,
                // # Safety:
                //
                // buf is a valid, non-empty slice, and therefore not null
                buf: unsafe {
                    NonNull::new_unchecked(buf as *mut [MaybeUninit<u8>] as *mut [u8])
                },
                _phantom: PhantomData,
            }
        }
    }

    /// Create a new framebuffer of size 0 with no backing memory.
    pub const fn empty() -> Self {
        Self::new(&mut [], 0, 0)
    }

    /// Get a subslice of rows of the framebuffer.
    pub fn rows(&mut self, range: impl RangeBounds<usize>) -> Framebuffer<'_, P> {
        let Range { start, end } = slice::range(range, ..self.rows);
        let rows = end - start;
        let start_byte = start * self.cols * size_of::<P>();
        let end_byte = end * self.cols * size_of::<P>();
        let len = end_byte - start_byte;

        // # Safety:
        //
        // - `buf` is derived from and in-bounds of `self.buf`.
        // - `len`  = end_byte - start_byte
        //          = end * cols * size_of::<P>() - start * cols * size_of::<P>()
        //          = (end - start) * cols * size_of::<P>()
        //          = rows * cols * size_of::<P>()
        unsafe {
            let buf_start = self.buf.as_non_null_ptr().add(start_byte);
            let buf = NonNull::slice_from_raw_parts(buf_start, len);
            Self {
                rows,
                cols: self.cols,
                buf,
                _phantom: PhantomData,
            }
        }
    }

    /// Divide `self` into two adjacent subslices of rows at an index.
    ///
    /// # Panics
    ///
    /// Panics if `mid > rows`.
    pub fn split_at(&mut self, mid: usize) -> (Framebuffer<'_, P>, Framebuffer<'_, P>) {
        assert!(mid <= self.nrows());
        let mid_byte = mid * self.cols;

        // # Safety:
        //
        // We checked that `mid_byte` == `mid * cols` <= `rows * cols` == `buf.len()`.
        let (left, right) = unsafe { self.buf.as_ptr().split_at_mut_unchecked(mid_byte) };
        // # Safety:
        //
        // `left` and `right` are both derived from and in-bounds of a valid `NonNull`.
        // Additionally, they are adjacent and thus non-overlapping,
        // and therefore independently valid for writes.
        //
        // left.len()   == mid_byte
        //              == mid * cols
        // right.len()  == buf.len() - mid_byte
        //              == rows * cols - mid * cols
        //              == (rows - mid) * cols
        unsafe {
            (
                Self {
                    rows: mid,
                    cols: self.cols,
                    buf: NonNull::new_unchecked(left),
                    _phantom: PhantomData,
                },
                Self {
                    rows: self.rows - mid,
                    cols: self.cols,
                    buf: NonNull::new_unchecked(right),
                    _phantom: PhantomData,
                },
            )
        }
    }

    /// Returns the number of columns in the [`Framebuffer`].
    pub const fn nrows(&self) -> usize {
        self.rows
    }

    /// Returns the number of rows in the [`Framebuffer`].
    pub const fn ncols(&self) -> usize {
        self.cols
    }

    /// Returns the number of pixels in the [`Framebuffer`].
    pub const fn len(&self) -> usize {
        self.buf.len()
    }

    /// Returns `true` if `len == 0`
    pub const fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl<P: Pod> Default for Framebuffer<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

/// A slice of a [`Framebuffer`] row.
#[derive(Debug)]
pub struct Row<'buf, P> {
    /// # Safety:
    ///
    /// `buf` must be valid for writes and `len` must be in-bounds.
    /// See [`core::ptr`] for details.
    buf: NonNull<[u8]>,
    _phantom: PhantomData<&'buf mut [P]>,
}

impl<P> Row<'_, P> {
    /// Create a new [`Row`] from a given byte slice.
    ///
    /// # Safety:
    ///
    /// `buf` must be valid for writes and `len` must be in-bounds.
    /// See [`core::ptr`] for details.
    const unsafe fn new(buf: NonNull<[u8]>) -> Self {
        Self {
            buf,
            _phantom: PhantomData,
        }
    }

    /// Returns the length of the [`Framebuffer`] [`Row`].
    pub const fn len(&self) -> usize {
        self.buf.len()
    }

    /// Returns `true` if `len == 0`.
    pub const fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Get a subslice of the [`Framebuffer`] [`Row`].
    ///
    /// # Panics
    ///
    /// Panics if `range` is out of bounds of `self`.
    pub fn slice(&mut self, range: impl RangeBounds<usize>) -> Row<'_, P> {
        let Range { start, end } = slice::range(range, ..self.buf.len());
        let len = end - start;
        // # Safety:
        // We checked that `range` is in-bounds of `self.buf`.
        unsafe {
            let start = self.buf.as_non_null_ptr().add(start);
            Self::new(NonNull::slice_from_raw_parts(start, len))
        }
    }

    /// Divide `self` into two adjacent subslices at an index.
    ///
    /// # Panics
    ///
    /// Panics if `mid > len`.
    pub fn split_at(&mut self, mid: usize) -> (Row<'_, P>, Row<'_, P>) {
        // # Safety:
        //
        // `self.buf.len()` is in-bounds.
        let (left, right) = unsafe { self.buf.as_ptr().split_at_mut(mid) };
        // # Safety:
        //
        // `left` and `right` are both derived from and in-bounds of a valid `NonNull`.
        // Additionally, they are adjacent and thus non-overlapping,
        // and therefore independently valid for writes.
        unsafe {
            (
                Self::new(NonNull::new_unchecked(left)),
                Self::new(NonNull::new_unchecked(right)),
            )
        }
    }
}

/// A single pixel in a [`Framebuffer`].
#[derive(Debug)]
pub struct Pixel<'buf, P> {
    /// # Safety:
    ///
    /// `buf` must be valid for writes at least as large as P.
    buf: NonNull<u8>,
    _phantom: PhantomData<&'buf mut P>,
}

impl<P: Pod> Pixel<'_, P> {
    /// Performs a word/byte-aligned volatile copy
    /// of the binary representation of `data` into this pixel.
    pub fn write(&mut self, data: P) {
        // # Safety:
        //
        // `self.buf` is valid for writes
        // and point to an allocation at least as large as P
        unsafe { aligned_volatile_copy(bytemuck::bytes_of(&data), self.buf) };
    }

    /// Performs a word/byte-aligned volatile copy of `data` into this pixel.
    ///
    /// # Panics
    ///
    /// Panics if `data.len() > size_of::<P>()`
    pub fn write_bytes(&mut self, data: &[u8]) {
        assert!(data.len() <= size_of::<P>());
        // Safety:
        // `self.buf` is valid for writes
        // and point to an allocation at least as large as `P`;
        // we asserted that `buf` is no longer than `size_of::<P>()`
        unsafe { aligned_volatile_copy(data, self.buf) };
    }
}

/// # Safety:
///
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
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(*src);
        }
        src = src.wrapping_add(1);
        dst = dst.wrapping_add(1);
    }

    let mut src = src.cast::<[u8; 4]>();
    let mut dst = dst.cast::<u32>();
    for _ in 0..body_len {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(u32::from_ne_bytes(*src));
        }
        src = src.wrapping_add(1);
        dst = dst.wrapping_add(1);
    }

    let mut src = src.cast::<u8>();
    let mut dst = dst.cast::<u8>();
    for _ in 0..tail_len {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(*src);
        }
        src = src.wrapping_add(1);
        dst = dst.wrapping_add(1);
    }
}
