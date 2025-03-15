use core::cmp;
use core::convert::Infallible;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::ops::RangeBounds;
use core::ptr::NonNull;
use core::slice;

use bytemuck::AnyBitPattern;
use bytemuck::NoUninit;
use bytemuck::Zeroable;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::pixelcolor::raw;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::*;

/// A row-major framebuffer backed by a byte slice.
/// Access to the backing memory is volatile and word/byte-aligned.
#[derive(Debug)]
pub struct Framebuffer<'buf, P> {
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    /// # Safety:
    ///
    /// Additionally, `len` must be equal to `rows * cols * size_of::<P>()`.
    pub(crate) buf: Row<'buf, P>,
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
    pub fn row(self, row: usize) -> Row<'buf, P> {
        assert!(row < self.rows);

        let ncols = self.cols;
        let Framebuffer { rows, cols, buf } = self.slice(row..=row);

        debug_assert_eq!(rows, 1);
        debug_assert_eq!(cols, ncols);

        buf
    }

    /// Get a single row of the framebuffer, of `row` is in bounds.
    pub fn try_row(self, row: usize) -> Option<Row<'buf, P>> {
        let ncols = self.cols;
        let Framebuffer { rows, cols, buf } = self.try_slice(row..=row)?;

        debug_assert_eq!(rows, 1);
        debug_assert_eq!(cols, ncols);

        Some(buf)
    }

    /// Get a subslice of rows of the framebuffer.
    ///
    /// # Panics
    ///
    /// Panics if `range` contains nonexistent rows.
    pub fn slice(self, range: impl RangeBounds<usize>) -> Self {
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
    /// Get a subslice of rows of the framebuffer, if `range` is in bounds.
    pub fn try_slice(self, range: impl RangeBounds<usize>) -> Option<Self> {
        let Range { start, end } = slice::try_range(range, ..self.rows)?;
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
            Some(Self {
                rows,
                cols: self.cols,
                buf,
            })
        }
    }

    /// Re-borrow the [`Framebuffer`] with a shorter lifetime.
    pub const fn reborrow<'a>(&'a mut self) -> Framebuffer<'a, P> {
        Framebuffer::<'a, P> {
            buf: self.buf.reborrow(),
            ..*self
        }
    }

    /// Divide `self` into two adjacent subslices of rows at a an index.
    ///
    /// The first will contain all indices from `[0, mid)` (excluding
    /// the index `mid` itself) and the second will contain all
    /// indices from `[mid, len)` (excluding the index `len` itself).
    ///
    /// # Panics
    ///
    /// Panics if `mid > nrows`.
    pub fn split_at(self, mid: usize) -> (Self, Self) {
        assert!(mid <= self.nrows());
        let mid_px = mid * self.cols;

        let (left, right): (Row<'buf, P>, Row<'buf, P>) = self.buf.split_at(mid_px);
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

    /// Get a bytewise [`Iterator`] over the framebuffer's content, starting at an index.
    /// The resulting iterator is empty if `start` is out of bounds.
    pub fn bytes(&self, start: usize) -> Bytes<'_> {
        self.buf.bytes(start)
    }

    /// Get an [`Iterator`] over the framebuffer's pixel data, starting at an index.
    /// The resulting iterator is empty if `start` is out of bounds.
    pub fn pixel_data(&self, start: usize) -> PixelData<'_, P> {
        self.buf.pixel_data(start)
    }

    /// Get an [`Iterator`] over the framebuffer's [`Row`]s, starting at an index.
    /// The resulting iterator is empty if `start` is out of bounds.
    pub fn rows(self, start: usize) -> Rows<'buf, P> {
        Rows {
            rest: self.try_slice(start..),
        }
    }

    /// Get an [`Iterator`] over the framebuffer's [`Pixel`]s, starting at an index.
    /// The resulting iterator  is empty if `start` is out of bounds.
    pub fn pixels(self, start: usize) -> Pixels<'buf, P> {
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

    /// Returns a write-valid pointer to the backing memory.
    pub const fn as_ptr(&self) -> NonNull<[u8]> {
        self.buf.as_ptr()
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

impl<P> Default for Framebuffer<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<P> Dimensions for Framebuffer<'_, P> {
    fn bounding_box(&self) -> Rectangle {
        Rectangle {
            top_left: Point { x: 0, y: 0 },
            size: Size {
                width: u32::try_from(self.cols)
                    .expect("framebuffer width out of bounds for u32"),
                height: u32::try_from(self.rows)
                    .expect("framebuffer height out of bounds for u32"),
            },
        }
    }
}

impl DrawTarget for Framebuffer<'_, [u8; 3]> {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for Pixel(Point { x, y }, color) in pixels {
            let (Ok(row), Ok(col)) = (usize::try_from(y), usize::try_from(x)) else {
                continue;
            };

            let framebuf = self.reborrow();
            let Some(row) = framebuf.try_row(row) else {
                continue;
            };
            let Some(mut pixel) = row.try_pixel(col) else {
                continue;
            };

            pixel.write(color.to_ne_bytes());
        }

        Ok(())
    }

    fn fill_contiguous<I>(
        &mut self,
        &Rectangle {
            top_left: Point { x, y },
            size: Size { width, height },
        }: &Rectangle,
        colors: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let (Ok(row), Ok(col)) = (usize::try_from(y), usize::try_from(x)) else {
            return Ok(());
        };
        let width =
            usize::try_from(width).expect("framebuffer width out of bounds for u32");
        let height =
            usize::try_from(height).expect("framebuffer height out of bounds for u32");

        // for (mut pixel, color) in self
        //     .reborrow()
        //     .rows(row)
        //     .take(height)
        //     .flat_map(|row| row.pixels(col).take(width))
        //     .zip(colors)
        // {
        //     pixel.write(color.to_be_bytes());
        // }

        let mut colors = colors.into_iter().map(raw::ToBytes::to_ne_bytes);
        for row in self.reborrow().rows(row).take(height) {
            row.try_slice(col..)
                .unwrap_or(Row::empty())
                .write_from_iter(colors.by_ref().take(width));
        }

        Ok(())
    }
}

/// An [`Iterator`] over the [`Row`]s of a [`Framebuffer`].
pub struct Rows<'buf, P> {
    pub(crate) rest: Option<Framebuffer<'buf, P>>,
}

impl<P> Rows<'_, P> {
    /// Get an empty [`Row`] iterator.
    pub const fn empty() -> Self {
        Self { rest: None }
    }
}

impl<P> Default for Rows<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<'buf, P> Iterator for Rows<'buf, P> {
    type Item = Row<'buf, P>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rest.take() {
            | None => None,
            | Some(rest) if rest.is_empty() => None,
            | Some(rest) => {
                let (Framebuffer { buf, .. }, tail) = rest.split_at(1);
                self.rest = Some(tail);

                Some(buf)
            }
        }
    }
}

/// A slice of a [`Framebuffer`] row.
#[derive(Debug)]
pub struct Row<'buf, P> {
    /// # Safety:
    ///
    /// - `buf` must be valid for writes of [`buf.len`] bytes.
    /// - `buf.len` must be in-bounds of the underlying allocated object.
    ///
    /// See [`core::ptr`] for details.
    pub(crate) buf: NonNull<[u8]>,
    pub(crate) _phantom: PhantomData<&'buf mut [P]>,
}

impl<'buf, P> Row<'buf, P> {
    /// Create a new [`Row`] from a given byte slice.
    ///
    /// # Safety:
    ///
    /// - `buf` must be valid for writes of [`buf.len`] bytes.
    /// - `buf.len` must be in-bounds.
    ///
    /// See [`core::ptr`] for details.
    pub(crate) const unsafe fn new(buf: NonNull<[u8]>) -> Self {
        Self {
            buf,
            _phantom: PhantomData,
        }
    }

    /// Create an empty [`Row`].
    pub const fn empty() -> Self {
        Self {
            buf: NonNull::from_mut(&mut []),
            _phantom: PhantomData,
        }
    }

    /// Returns the length of the [`Framebuffer`] [`Row`].
    pub fn len(&self) -> usize {
        self.buf.len() / size_of::<P>()
    }

    /// Returns `true` if `len == 0`.
    pub const fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Re-borrow the [`Row`] with a shorter lifetime.
    pub const fn reborrow<'a>(&'a mut self) -> Row<'a, P> {
        Row::<'a, P> { ..*self }
    }

    /// Get a subslice of the [`Framebuffer`] [`Row`].
    ///
    /// # Panics
    ///
    /// Panics if `range` is out of bounds of `self`.
    pub fn slice(self, range: impl RangeBounds<usize>) -> Self {
        let Range { start, end } = slice::range(range, ..self.len());
        let len = end - start;
        // # Safety:
        // We checked that `range` is in-bounds of `self.buf`.
        unsafe {
            let start = self.buf.as_non_null_ptr().add(start * size_of::<P>());
            Self::new(NonNull::slice_from_raw_parts(start, len))
        }
    }

    /// Get a subslice of the [`Framebuffer`] [`Row`], if `range` is in bounds.
    pub fn try_slice(self, range: impl RangeBounds<usize>) -> Option<Self> {
        let Range { start, end } = slice::try_range(range, ..self.len())?;
        let len = end - start;
        let start_byte = start * size_of::<P>();
        let byte_len = len * size_of::<P>();
        // # Safety:
        // We checked that `range` is in-bounds of `self.buf`.
        unsafe {
            let start = self.buf.as_non_null_ptr().add(start_byte);
            Some(Self::new(NonNull::slice_from_raw_parts(start, byte_len)))
        }
    }

    /// Divide `self` into two adjacent subslices at a pixel index.
    ///
    /// The first will contain all indices from `[0, mid)` (excluding
    /// the index `mid` itself) and the second will contain all
    /// indices from `[mid, len)` (excluding the index `len` itself).
    ///
    /// # Panics
    ///
    /// Panics if `mid > len`.
    pub fn split_at(self, mid: usize) -> (Self, Self) {
        assert!(mid <= self.len());
        let mid_byte = mid * size_of::<P>();
        // # Safety:
        //
        // `self.buf.len()` is in-bounds.
        let (left, right) = unsafe { self.buf.as_ptr().split_at_mut(mid_byte) };
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

    /// Get a bytewise [`Iterator`] over the row's content, starting at an index.
    /// The resulting iterator  is empty if `start` is out of bounds.
    pub fn bytes(&self, start: usize) -> Bytes<'_> {
        if start >= self.buf.len() {
            return Bytes::empty();
        }

        // # Safety:
        //
        // `buf` is valid and in-bounds as per `Framebuffer` precondition.
        unsafe { Bytes::new(self.buf.as_ptr(), start) }
    }

    /// Get an [`Iterator`] over the row's pixel data, starting at an index.
    /// The resulting iterator  is empty if `start` is out of bounds.
    pub fn pixel_data(&self, start: usize) -> PixelData<'_, P> {
        if start >= self.len() {
            return PixelData::empty();
        }
        PixelData::new(self.bytes(start * size_of::<P>()))
    }

    /// Get an [`Iterator`] over the row's [`Pixel`]s, starting at an index.
    /// The resulting iterator  is empty if `start` is out of bounds.
    pub fn pixels(self, start: usize) -> Pixels<'buf, P> {
        Pixels {
            rest: self.try_slice(start..),
        }
    }

    /// Get a [`Pixel`] from this row, if the column index is in range.
    pub fn try_pixel(&self, col: usize) -> Option<Pixel<'_, P>> {
        if col >= self.len() {
            return None;
        }

        Some(Pixel {
            // # Safety:
            //
            //     `col` < `len`
            // <=> `size_of::<P>()` * `col` < `size_of::<P>()` * `len`
            // <=> `size_of::<P>()` * `col` < `buf.len`
            buf: unsafe {
                NonNull::new_unchecked(self.buf.as_mut_ptr().add(size_of::<P>() * col))
            },
            _phantom: PhantomData,
        })
    }

    /// Get a [`Pixel`] from this row.
    ///
    /// # Panics
    ///
    /// Panics if `col >= len`.
    pub fn pixel(&self, col: usize) -> Pixel<'_, P> {
        match self.try_pixel(col) {
            | Some(pixel) => pixel,
            | None => panic!(
                "column index `{col}` is out of range of len `{}`",
                self.len()
            ),
        }
    }

    /// Returns a write-valid pointer to the backing memory.
    pub const fn as_ptr(&self) -> NonNull<[u8]> {
        self.buf
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

    /// Performs a word/byte-aligned volatile copy
    /// of up to `self.len` pixels into the row.
    pub fn write_from_iter(&mut self, data: impl IntoIterator<Item = P>) {
        // # Safety:
        //
        // As per `self.buf` precondition:
        // - `self.buf` is valid for writes of `buf.len` bytes
        // - `buf.len` is be in-bounds of the underlying allocated object.
        unsafe {
            aligned_volatile_copy_from_iter(
                data.into_iter().flat_map(|v| {
                    (0..).map_while(move |i| bytemuck::bytes_of(&v).get(i).copied())
                }),
                self.buf,
            )
        }
    }
}

impl<P> Default for Row<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<P> Dimensions for Row<'_, P> {
    fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
        use embedded_graphics::prelude::*;
        use embedded_graphics::primitives::*;

        Rectangle {
            top_left: Point { x: 0, y: 0 },
            size: Size {
                width: u32::try_from(self.len())
                    .expect("framebuffer width out of bounds for u32"),
                height: 1,
            },
        }
    }
}

impl DrawTarget for Row<'_, [u8; 3]> {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for Pixel(Point { x, y }, color) in pixels {
            if y != 0 {
                continue;
            }
            let Ok(col) = usize::try_from(x) else {
                continue;
            };
            let Some(mut pixel) = self.try_pixel(col) else {
                continue;
            };
            pixel.write(color.to_ne_bytes());
        }

        Ok(())
    }

    fn fill_contiguous<I>(
        &mut self,
        &Rectangle {
            top_left: Point { x, y },
            size: Size { width, height },
        }: &Rectangle,
        colors: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        if y != 0 {
            return Ok(());
        }
        let Ok(col) = usize::try_from(x) else {
            return Ok(());
        };
        if height == 0 {
            return Ok(());
        }
        let width =
            usize::try_from(width).expect("framebuffer width out of bounds for u32");

        // for (mut pixel, color) in
        //     self.reborrow().pixels(col).take(width as usize).zip(colors)
        // {
        //     pixel.write(color.to_be_bytes());
        // }

        self.reborrow().try_slice(col..).unwrap_or(Row::empty()).write_from_iter(
            colors.into_iter().map(raw::ToBytes::to_ne_bytes).take(width),
        );

        Ok(())
    }
}

/// A bytewise [`Iterator`] over the contents of a [`Framebuffer`].
#[derive(Debug)]
#[derive(Clone, Copy)]
pub struct Bytes<'a> {
    pub(crate) buf: *const [u8],
    pub(crate) next: usize,
    pub(crate) _phantom: PhantomData<&'a [u8]>,
}

impl Bytes<'_> {
    /// Get an empty [`Bytes`] iterator.
    pub const fn empty() -> Self {
        Self {
            buf: &[],
            next: 0,
            _phantom: PhantomData,
        }
    }

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
    pub(crate) unsafe fn new(buf: *const [u8], start: usize) -> Self {
        Self {
            buf,
            next: start,
            _phantom: PhantomData,
        }
    }
}

impl Default for Bytes<'_> {
    fn default() -> Self {
        Self::empty()
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
    pub(crate) bytes: Bytes<'a>,
    pub(crate) _phantom: PhantomData<P>,
}

impl<'a, P> PixelData<'a, P> {
    /// Get an empty [`PixelData`] iterator.
    pub const fn empty() -> Self {
        Self {
            bytes: Bytes::empty(),
            _phantom: PhantomData,
        }
    }

    pub(crate) fn new(bytes: Bytes<'a>) -> Self {
        Self {
            bytes,
            _phantom: PhantomData,
        }
    }
}

impl<P> Default for PixelData<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<P> Iterator for PixelData<'_, P>
where
    P: Zeroable + NoUninit + AnyBitPattern,
{
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

impl<P> ExactSizeIterator for PixelData<'_, P>
where
    P: Zeroable + NoUninit + AnyBitPattern,
{
    fn len(&self) -> usize {
        self.bytes.len() / size_of::<P>()
    }
}

impl<P> FusedIterator for PixelData<'_, P> where P: Zeroable + NoUninit + AnyBitPattern {}

/// An [`Iterator`] over the Pixels in a [`Framebuffer`].
pub struct Pixels<'buf, P> {
    pub(crate) rest: Option<Row<'buf, P>>,
}

impl<P> Pixels<'_, P> {
    /// Get an empty [`Pixel`] iterator.
    pub const fn empty() -> Self {
        Self { rest: None }
    }
}

impl<P> Default for Pixels<'_, P> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<'buf, P: NoUninit> Iterator for Pixels<'buf, P> {
    type Item = Pixel<'buf, P>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rest.take() {
            | None => None,
            | Some(rest) if rest.is_empty() => None,
            | Some(rest) => {
                let (head, tail) = rest.split_at(1);
                self.rest = Some(tail);

                debug_assert_eq!(head.buf.len(), size_of::<P>());
                // # Safety:
                //
                // head is exactly one pixel
                Some(Pixel {
                    buf: head.buf.as_non_null_ptr(),
                    _phantom: PhantomData,
                })
            }
        }
    }
}

/// A single pixel in a [`Framebuffer`].
#[derive(Debug)]
pub struct Pixel<'buf, P> {
    /// # Safety:
    ///
    /// `buf` must be valid for writes at least as large as P.
    pub(crate) buf: NonNull<u8>,
    pub(crate) _phantom: PhantomData<&'buf mut P>,
}

impl<P: NoUninit> Pixel<'_, P> {
    /// Performs a word/byte-aligned volatile copy
    /// of the binary representation of `data` into this pixel.
    pub fn write(&mut self, data: P) {
        // # Safety:
        //
        // `self.buf` is valid for writes
        // and points to an allocation at least as large as P
        unsafe {
            if size_of::<P>() >= 2 * align_of::<u32>() {
                aligned_volatile_copy(bytemuck::bytes_of(&data), self.buf)
            } else {
                bytewise_volatile_copy(bytemuck::bytes_of(&data), self.buf)
            }
        }
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
pub(crate) unsafe fn bytewise_volatile_copy(src: &[u8], dst: NonNull<u8>) {
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
pub(crate) unsafe fn aligned_volatile_copy(src: &[u8], dst: NonNull<u8>) {
    let head_len = cmp::min(dst.align_offset(align_of::<u32>()), src.len());
    let body_len = (src.len() - head_len) / size_of::<u32>();
    let tail_len = (src.len() - head_len) % size_of::<u32>();

    let mut src = src.as_ptr();
    let mut dst = dst.as_ptr();
    for _ in 0..head_len {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(*src);
            src = src.add(1);
            dst = dst.add(1);
        }
    }

    debug_assert_eq!(dst.align_offset(align_of::<u32>()), 0);
    let mut src = src.cast::<[u8; 4]>();
    let mut dst = dst.cast::<u32>();
    for _ in 0..body_len {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(u32::from_ne_bytes(*src));
            src = src.add(1);
            dst = dst.add(1);
        }
    }

    let mut src = src.cast::<u8>();
    let mut dst = dst.cast::<u8>();
    for _ in 0..tail_len {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(*src);
            src = src.add(1);
            dst = dst.add(1);
        }
    }
}

/// # Safety:
///
/// - `dst` must be write-valid
/// - `dst.len` must be in-bounds of the underlying allocated object.
#[allow(unused)]
pub(crate) unsafe fn bytewise_volatile_copy_from_iter(
    src: impl IntoIterator<Item = u8>,
    dst: NonNull<[u8]>,
) {
    for (byte, offset) in src.into_iter().take(dst.len()).zip(0..) {
        // # Safety:
        //
        // `offset` does not exceed src.len().
        // As per preconditon, `dst` is thus valid for writes at `offset`.
        unsafe {
            dst.as_non_null_ptr().add(offset).write_volatile(byte);
        }
    }
}

/// # Safety:
///
/// - `dst` must be write-valid
/// - `dst.len` must be in-bounds of the underlying allocated object.
pub(crate) unsafe fn aligned_volatile_copy_from_iter(
    src: impl IntoIterator<Item = u8>,
    dst: NonNull<[u8]>,
) {
    let head_len = cmp::min(dst.as_mut_ptr().align_offset(align_of::<u32>()), dst.len());
    let body_len = (dst.len() - head_len) / size_of::<u32>();
    let tail_len = (dst.len() - head_len) % size_of::<u32>();

    let mut dst = dst.as_mut_ptr();
    let mut src = src.into_iter();
    for byte in src.by_ref().take(head_len) {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(byte);
            dst = dst.add(1);
        }
    }

    debug_assert_eq!(dst.align_offset(align_of::<u32>()), 0);
    let mut dst = dst.cast::<u32>();
    let mut src_words = src.by_ref().take(body_len * size_of::<u32>()).array_chunks();
    for word in src_words.by_ref() {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(u32::from_ne_bytes(word));
            dst = dst.add(1);
        }
    }

    let mut dst = dst.cast::<u8>();

    let tail = src_words.into_remainder().into_iter().flatten().chain(src.take(tail_len));
    for byte in tail {
        // Safety: trust me bro
        unsafe {
            dst.write_volatile(byte);
            dst = dst.add(1)
        }
    }
}
