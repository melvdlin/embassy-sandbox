use core::borrow::BorrowMut;
use core::convert::Infallible;
use core::iter::FusedIterator;
use core::mem;
use core::ops::Range;
use core::ops::RangeBounds;
use core::slice;

use embedded_graphics::Pixel;
use embedded_graphics::prelude::Dimensions;
use embedded_graphics::prelude::DrawTarget;
use embedded_graphics::prelude::OriginDimensions;
use embedded_graphics::prelude::Point;
use embedded_graphics::prelude::Size;
use embedded_graphics::primitives::Rectangle;

use super::color::Argb8888;
use super::color::Grayscale;
use super::display::dma2d;
use super::display::dma2d::Dma2d;
use super::display::dma2d::InputConfig;
use super::display::dma2d::OutputConfig;
use super::display::dma2d::format::typelevel as format;
use super::gui::Accelerated;
use super::gui::AcceleratedBase;

#[cfg(not(target_pointer_width = "32"))]
compile_error!("targets with pointer width other than 32 not supported");

pub struct Framebuffer<B, D> {
    buf: B,
    width: u16,
    height: u16,
    dma: D,
}

/// A backing buffer that can be upgraded to a [`Framebuffer`]
/// by providing a DMA accelerator.
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct Backing<B> {
    buf: B,
    width: u16,
    height: u16,
}

impl<B, D> Framebuffer<B, D> {
    /// Get the number of pixels in this framebuffer.
    pub fn len(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Returns `true` iff `self.len() == 0`.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width as usize + x
    }

    fn range_and_offset(&self, area: &Rectangle) -> (Range<usize>, u16) {
        debug_assert_eq!(&self.bounding_box().intersection(area), area);
        let area = self.clamp_area(area);
        let start = self.index(area.top_left.x as usize, area.top_left.y as usize);
        let end = if let Some(bottom_right) = area.bottom_right() {
            self.index(bottom_right.x as usize, bottom_right.y as usize) + 1
        } else {
            start
        };
        let offset = self.width - area.size.width as u16;
        (start..end, offset)
    }

    fn output_cfg(&self, area: &Rectangle) -> (dma2d::OutputConfig, Range<usize>) {
        let area = self.clamp_area(area);
        let (range, offset) = self.range_and_offset(&area);
        let width = area.size.width as u16;
        let height = area.size.height as u16;
        (dma2d::OutputConfig::new(width, height, offset), range)
    }

    #[inline]
    fn clamp_area(&self, area: &Rectangle) -> Rectangle {
        area.intersection(&Rectangle::new(
            Point::zero(),
            Size::new(self.width.into(), self.height.into()),
        ))
    }
}

impl<B> Backing<B>
where
    B: AsMut<[Argb8888]>,
{
    /// Create a new backing buffer.
    ///
    /// # Panics
    ///
    /// Panics if `width * height != buf.as_mut().len()`.
    pub fn new(buf: B, width: u16, height: u16) -> Self {
        let mut buf = buf;
        assert_eq!(width as usize * height as usize, buf.as_mut().len());
        Self { buf, width, height }
    }

    /// Create a new framebuffer borrowing `self` as backing buffer
    /// and using the provided DMA accelerator.
    pub fn with_dma<D>(&mut self, dma: D) -> Framebuffer<&mut B, D> {
        Framebuffer::new(&mut self.buf, self.width, self.height, dma)
    }
}

impl<B, D> Framebuffer<B, D>
where
    B: AsMut<[Argb8888]>,
{
    /// Create a new framebuffer with a DMA accelerator.
    ///
    /// # Panics
    ///
    /// Panics if `width * height != buf.as_mut().len()`.
    pub fn new(buf: B, width: u16, height: u16, dma: D) -> Self {
        let mut buf = buf;
        assert_eq!(width as usize * height as usize, buf.as_mut().len());
        Self {
            buf,
            width,
            height,
            dma,
        }
    }

    /// Swap out the backing buffer,
    ///
    /// # Panics
    ///
    /// Panics iff `buf.len() != self.buf.len()`.
    pub fn swap_buf(&mut self, mut buf: B) -> B {
        assert_eq!(buf.as_mut().len(), self.buf.as_mut().len());
        mem::replace(&mut self.buf, buf)
    }

    /// Get an iterator over the framebuffer's rows, subsliced to a specified range.
    ///
    /// # Panics
    ///
    /// Panics if `active_range` is out of bounds for `..self.width`.
    /// Panics if `first_row >= self.height`.
    pub fn rows(
        &mut self,
        first_row: usize,
        active_range: impl RangeBounds<usize>,
    ) -> Rows<'_> {
        assert!(first_row < self.height as usize);
        let rows = &mut self.buf.as_mut()[first_row * self.width as usize..];
        let cols = slice::range(active_range, ..self.width as usize);
        Rows::new(rows, self.width as usize, cols)
    }
}

impl<B, D> AcceleratedBase for Framebuffer<B, D>
where
    B: AsMut<[Argb8888]>,
    D: BorrowMut<Dma2d>,
{
    /// Draw a rectangle in the speicifed color.
    async fn fill_rect(&mut self, area: &Rectangle, color: Argb8888) {
        let (out_cfg, range) = self.output_cfg(area);
        let buf = bytemuck::must_cast_slice_mut(&mut self.buf.as_mut()[range]);
        self.dma.borrow_mut().fill::<format::Argb8888>(buf, &out_cfg, color).await
    }
}

impl<F, B, D> Accelerated<F> for Framebuffer<B, D>
where
    F: dma2d::Format,
    B: AsMut<[Argb8888]>,
    D: BorrowMut<Dma2d>,
{
    /// Copy the source image into this framebuffer.
    ///
    /// # Panics
    ///
    /// Panics if `source.len() != self.len()`
    async fn copy(&mut self, area: &Rectangle, source: &[F::Repr], blend: bool) {
        let (out_cfg, range) = self.output_cfg(area);
        let buf = bytemuck::must_cast_slice_mut(&mut self.buf.as_mut()[range]);
        let fg = InputConfig::<F>::copy(source, 0);

        if blend {
            self.dma
                .borrow_mut()
                .transfer_onto::<format::Argb8888, F>(buf, &out_cfg, &fg, None)
                .await
        } else {
            self.dma
                .borrow_mut()
                .transfer_memory::<format::Argb8888, F>(buf, &out_cfg, &fg)
                .await
        }
    }

    /// Copy the source grayscale image blended with a color
    /// into this framebuffer.
    ///
    /// # Panics
    ///
    /// Panics if `source.len() != self.len()`
    async fn copy_with_color(
        &mut self,
        area: &Rectangle,
        source: &[F::Repr],
        color: Argb8888,
        blend: bool,
    ) where
        F: Grayscale,
    {
        let (out_cfg, range) = self.output_cfg(area);
        let buf = bytemuck::must_cast_slice_mut(&mut self.buf.as_mut()[range]);
        let fg = InputConfig::<F>::copy(source, 0).blend_color(color);

        if blend {
            self.dma
                .borrow_mut()
                .transfer_onto::<format::Argb8888, F>(buf, &out_cfg, &fg, None)
                .await
        } else {
            self.dma
                .borrow_mut()
                .transfer_memory::<format::Argb8888, F>(buf, &out_cfg, &fg)
                .await
        }
    }
}

impl<B, D> OriginDimensions for Framebuffer<B, D> {
    fn size(&self) -> Size {
        Size {
            width: self.width.into(),
            height: self.height.into(),
        }
    }
}

impl<B> OriginDimensions for Backing<B> {
    fn size(&self) -> Size {
        Size {
            width: self.width.into(),
            height: self.height.into(),
        }
    }
}

impl<B, D> DrawTarget for Framebuffer<B, D>
where
    B: AsMut<[Argb8888]>,
    D: BorrowMut<Dma2d>,
{
    type Color = Argb8888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let bounds = self.bounding_box();
        for Pixel(Point { x, y }, color) in
            pixels.into_iter().filter(|Pixel(point, _)| bounds.contains(*point))
        {
            let index = self.index(x as usize, y as usize);
            self.buf.as_mut()[index] = color;
        }

        Ok(())
    }

    fn fill_contiguous<I>(
        &mut self,
        area: &Rectangle,
        colors: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let area = self.clamp_area(area);

        let width = area.size.width as usize;
        let height = area.size.height as usize;
        let first_row = area.top_left.y as usize;
        let first_col = area.top_left.x as usize;

        for (pixel, color) in self
            .rows(first_row, first_col..first_col + width)
            .take(height)
            .flatten()
            .zip(colors)
        {
            *pixel = color;
        }

        Ok(())
    }

    fn fill_solid(
        &mut self,
        area: &Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        let area = self.clamp_area(area);
        let (range, offset) = self.range_and_offset(&area);
        let width = area.size.width as u16;
        let height = area.size.height as u16;
        self.dma.borrow_mut().fill_blocking::<format::Argb8888>(
            bytemuck::must_cast_slice_mut(&mut self.buf.as_mut()[range]),
            &OutputConfig::new(width, height, offset),
            color,
        );
        Ok(())
    }
}

/// See [`Framebuffer::rows`].
pub struct Rows<'a> {
    range: Range<usize>,
    inner: core::slice::ChunksExactMut<'a, Argb8888>,
}

impl<'a> Rows<'a> {
    pub fn new(
        buf: &'a mut [Argb8888],
        width: usize,
        active_range: Range<usize>,
    ) -> Self {
        Self {
            range: active_range,
            inner: buf.chunks_exact_mut(width),
        }
    }
}

impl<'a> Iterator for Rows<'a> {
    type Item = &'a mut [Argb8888];

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|row| &mut row[self.range.clone()])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for Rows<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back().map(|row| &mut row[self.range.clone()])
    }
}

impl ExactSizeIterator for Rows<'_> {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl FusedIterator for Rows<'_> {}
