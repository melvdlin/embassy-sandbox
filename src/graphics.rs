use core::cmp;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::ops::RangeBounds;
use core::ptr::NonNull;
use core::slice;

use bytemuck::NoUninit;
use bytemuck::Pod;

/// A row-major framebuffer backed by a byte slice.
/// Access to the backing memory is volatile and word/byte-aligned.
#[derive(Debug)]
pub struct Framebuffer<'buf, P> {
    rows: usize,
    cols: usize,
    /// # Safety:
    ///
    /// Additionally, `len` must be equal to `rows * cols * size_of::<P>()`.
    buf: Row<'buf, P>,
}

impl<'buf, P> Framebuffer<'buf, P> {
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
                    Row::new(NonNull::new_unchecked(
                        buf as *mut [MaybeUninit<u8>] as *mut [u8],
                    ))
                },
            }
        }
    }

    /// Create a new framebuffer of size 0 with no backing memory.
    pub const fn empty() -> Self {
        Self::new(&mut [], 0, 0)
    }

    /// Get a single row of the framebuffer.
    ///
    /// # Panics
    ///
    /// Panics if `row > nrows`
    pub fn row(&mut self, row: usize) -> Row<'_, P> {
        assert!(row < self.rows);

        let ncols = self.cols;
        let Framebuffer { rows, cols, buf } = self.rows(row..=row);

        debug_assert_eq!(rows, 1);
        debug_assert_eq!(cols, ncols);

        buf
    }

    /// Get a subslice of rows of the framebuffer.
    ///
    /// # Panics
    ///
    /// Panics if `range` contains nonexistent rows.
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
            let buf_start = self.buf.buf.as_non_null_ptr().add(start_byte);
            let buf = Row::new(NonNull::slice_from_raw_parts(buf_start, len));
            Self {
                rows,
                cols: self.cols,
                buf,
            }
        }
    }

    /// Divide `self` into two adjacent subslices of rows at an index.
    ///
    /// # Panics
    ///
    /// Panics if `mid > nrows`.
    pub fn split_at(&mut self, mid: usize) -> (Framebuffer<'_, P>, Framebuffer<'_, P>) {
        assert!(mid <= self.nrows());
        let mid_byte = mid * self.cols;

        let (left, right): (Row<'_, P>, Row<'_, P>) = self.buf.split_at(mid_byte);
        // # Safety:
        //
        // left.len()   == mid_byte
        //              == mid * cols
        // right.len()  == buf.len() - mid_byte
        //              == rows * cols - mid * cols
        //              == (rows - mid) * cols

        (
            Framebuffer {
                rows: mid,
                cols: self.cols,
                buf: left,
            },
            Framebuffer {
                rows: self.rows - mid,
                cols: self.cols,
                buf: right,
            },
        )
    }

    /// Get a bytewise [`Iterator`]` over the framebuffer's content, starting at an index.
    ///
    /// # Panics
    ///
    /// Panics if `start >= len * size_of::<P>()`.
    pub fn bytes(&self, start: usize) -> Bytes<'_> {
        self.buf.bytes(start)
    }

    /// Get an [`Iterator`]` over the framebuffer's content, starting at an index.
    ///
    /// # Panics
    ///
    /// Panics if `start >= len`.
    pub fn pixels(&self, start: usize) -> PixelData<'_, P> {
        self.buf.pixels(start)
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
        self.rows * self.cols
    }

    /// Returns `true` if `len == 0`
    pub const fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl<P: NoUninit> Framebuffer<'_, P> {
    /// Performs a word/byte-aligned volatile copy
    /// of the binary representation of `data` into this pixel.
    pub fn write(&mut self, data: &[P]) {
        self.buf.write(data)
    }

    /// Performs a word/byte-aligned volatile copy of `data` into this pixel.
    ///
    /// # Panics
    ///
    /// Panics if `data.len() > size_of::<P>()`
    pub fn write_bytes(&mut self, data: &[u8]) {
        self.buf.write_bytes(data)
    }
}

impl<P: Pod> Default for Framebuffer<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

/// A bytewise [`Iterator`] over the contents of a [`Framebuffer`].
#[derive(Debug)]
#[derive(Clone, Copy)]
pub struct Bytes<'a> {
    buf: *const [u8],
    next: usize,
    _phantom: PhantomData<&'a [u8]>,
}

impl Bytes<'_> {
    /// # Safety:
    ///
    /// - `buf` must be valid for reads of `buf.len` bytes.
    /// - `buf.len` must be in-bounds.
    ///
    /// See [`core::ptr`] for details.
    ///
    /// # Panics
    ///
    /// Panics if `start >= buf.len`.
    unsafe fn new(buf: *const [u8], start: usize) -> Self {
        Self {
            buf,
            next: start,
            _phantom: PhantomData,
        }
    }
}

impl Iterator for Bytes<'_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len() == 0 {
            None
        } else {
            // # Safety:
            //
            // as per precondition in [`Framebuffer`],
            // `buf` is valid for reads and `len` is in-bounds.
            // `next` does not exceed `len`.
            let byte = unsafe { self.buf.as_ptr().add(self.next).read_volatile() };
            self.next += 1;
            Some(byte)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for Bytes<'_> {
    fn len(&self) -> usize {
        self.buf.len() - self.next
    }
}

impl FusedIterator for Bytes<'_> {}
/// An [`Iterator`] over the contents of a [`Framebuffer`].
#[derive(Debug)]
#[derive(Clone, Copy)]
pub struct PixelData<'a, P> {
    bytes: Bytes<'a>,
    _phantom: PhantomData<P>,
}

impl<'a, P> PixelData<'a, P> {
    fn new(bytes: Bytes<'a>) -> Self {
        Self {
            bytes,
            _phantom: PhantomData,
        }
    }
}

impl<P: Pod> Iterator for PixelData<'_, P> {
    type Item = P;

    fn next(&mut self) -> Option<Self::Item> {
        debug_assert!(self.bytes.len().is_multiple_of(size_of::<P>()));

        let mut next = P::zeroed();
        let next_bytes = bytemuck::bytes_of_mut(&mut next);

        for byte in next_bytes {
            *byte = self.bytes.next()?
        }

        Some(next)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<P: Pod> ExactSizeIterator for PixelData<'_, P> {
    fn len(&self) -> usize {
        self.bytes.len() / size_of::<P>()
    }
}

impl<P: Pod> FusedIterator for PixelData<'_, P> {}

/// A slice of a [`Framebuffer`] row.
#[derive(Debug)]
pub struct Row<'buf, P> {
    /// # Safety:
    ///
    /// - `buf` must be valid for writes of [`buf.len`] bytes.
    /// - `buf.len` must be in-bounds.
    ///
    /// See [`core::ptr`] for details.
    buf: NonNull<[u8]>,
    _phantom: PhantomData<&'buf mut [P]>,
}

impl<P> Row<'_, P> {
    /// Create a new [`Row`] from a given byte slice.
    ///
    /// # Safety:
    ///
    /// - `buf` must be valid for writes of [`buf.len`] bytes.
    /// - `buf.len` must be in-bounds.
    ///
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
                Row::new(NonNull::new_unchecked(left)),
                Row::new(NonNull::new_unchecked(right)),
            )
        }
    }
    /// Get a bytewise [`Iterator`]` over the row's content, starting at an index.
    ///
    /// # Panics
    ///
    /// Panics if `start >= len * size_of::<P>()`.
    pub fn bytes(&self, start: usize) -> Bytes<'_> {
        assert!(start < self.buf.len());

        // # Safety:
        //
        // `buf` is valid and in-bounds as per `Framebuffer` precondition.
        unsafe { Bytes::new(self.buf.as_ptr(), start) }
    }

    /// Get an [`Iterator`]` over the row's content, starting at an index.
    ///
    /// # Panics
    ///
    /// Panics if `start >= len`.
    pub fn pixels(&self, start: usize) -> PixelData<'_, P> {
        assert!(start < self.len());
        PixelData::new(self.bytes(start * size_of::<P>()))
    }
}

impl<P: NoUninit> Row<'_, P> {
    /// Performs a word/byte-aligned volatile copy
    /// of the binary representation of `data` into this pixel.
    pub fn write(&mut self, data: &[P]) {
        assert!(data.len() <= self.len());
        let data_bytes = bytemuck::cast_slice(data);
        // # Safety:
        //
        // - `self.buf` is valid for writes of `buf.len` bytes
        // - we asserted that `data.len` does not exceed `self.len`,
        //   and therefore that data_bytes.len does not exceed buf.len
        unsafe { aligned_volatile_copy(data_bytes, self.buf.as_non_null_ptr()) };
    }

    /// Performs a word/byte-aligned volatile copy of `data` into this pixel.
    ///
    /// # Panics
    ///
    /// Panics if `data.len() > size_of::<P>()`
    pub fn write_bytes(&mut self, data: &[u8]) {
        assert!(data.len() <= self.buf.len());
        // # Safety:
        //
        // - `self.buf` is valid for writes of `buf.len` bytes
        // - we asserted that `data.len` does not exceed `buf.len`
        unsafe { aligned_volatile_copy(data, self.buf.as_non_null_ptr()) };
    }

    pub fn write_from_iter(&mut self, data: impl IntoIterator<Item = P>) {
        todo!()
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
        // and points to an allocation at least as large as P
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
        // and points to an allocation at least as large as `P`;
        // we asserted that `buf` is no longer than `size_of::<P>()`
        unsafe { aligned_volatile_copy(data, self.buf) };
    }
}

/// # Safety:
///
/// if `src` is non-empty,
/// then `dst` must be valid for writes and
/// point to an allocation of at least `src.len()` bytes
unsafe fn bytewise_volatile_copy(src: &[u8], dst: NonNull<u8>) {
    for (&byte, offset) in src.iter().zip(0..) {
        // # Safety:
        //
        // `offset` does not exceed src.len().
        // As per preconditon, `dst` is thus valid for writes at `offset`.
        unsafe {
            dst.add(offset).write_volatile(byte);
        }
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
