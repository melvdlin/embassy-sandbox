use core::convert::Infallible;
use core::iter::FusedIterator;
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
use super::display::dma2d;
use super::display::dma2d::InputConfig;
use super::display::dma2d::OutputConfig;
use super::display::dma2d::format::typelevel as format;

#[cfg(not(target_pointer_width = "32"))]
compile_error!("targets with pointer width other than 32 not supported");

pub struct Framebuffer<'a, B> {
    framebuffer: B,
    width: u16,
    height: u16,
    dma: &'a mut super::display::dma2d::Dma2d,
}

impl<B> Framebuffer<'_, B> {
    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        y * self.width as usize + x
    }

    fn range_and_offset(&self, area: &Rectangle) -> (Range<usize>, u16) {
        debug_assert_eq!(&self.bounding_box().intersection(area), area);
        let area = self.clamp_area(area);
        let start = self.index(area.top_left.x as usize, area.top_left.y as usize);
        let end = if let Some(bottom_right) = area.bottom_right() {
            self.index(bottom_right.x as usize, bottom_right.y as usize)
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

impl<B> Framebuffer<'_, B>
where
    B: AsMut<[Argb8888]>,
{
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
        let rows = &mut self.framebuffer.as_mut()[first_row * self.width as usize..];
        let cols = slice::range(active_range, ..self.width as usize);
        Rows::new(rows, self.width as usize, cols)
    }

    pub async fn fill_rect(&mut self, area: &Rectangle, color: Argb8888) {
        let (out_cfg, range) = self.output_cfg(area);
        let buf = bytemuck::must_cast_slice_mut(&mut self.framebuffer.as_mut()[range]);
        self.dma.fill::<format::Argb8888>(buf, &out_cfg, color).await
    }

    pub async fn copy<Format>(&mut self, area: &Rectangle, source: &[Format::Repr])
    where
        Format: format::Format,
    {
        let (out_cfg, range) = self.output_cfg(area);
        let buf = bytemuck::must_cast_slice_mut(&mut self.framebuffer.as_mut()[range]);
        let fg = InputConfig::<Format>::copy(source, 0);

        self.dma.transfer_memory::<format::Argb8888, Format>(buf, &out_cfg, &fg).await
    }

    pub async fn copy_with_color<Format>(
        &mut self,
        area: &Rectangle,
        source: &[Format::Repr],
        color: Argb8888,
    ) where
        Format: format::Grayscale,
    {
        let (out_cfg, range) = self.output_cfg(area);
        let buf = bytemuck::must_cast_slice_mut(&mut self.framebuffer.as_mut()[range]);
        let fg = InputConfig::<Format>::copy(source, 0).blend_color(color);

        self.dma.transfer_memory::<format::Argb8888, Format>(buf, &out_cfg, &fg).await
    }
}

impl<B> OriginDimensions for Framebuffer<'_, B> {
    fn size(&self) -> Size {
        Size {
            width: self.width.into(),
            height: self.height.into(),
        }
    }
}

impl<B> DrawTarget for Framebuffer<'_, B>
where
    B: AsMut<[Argb8888]>,
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
            self.framebuffer.as_mut()[index] = color;
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
        self.dma.fill_blocking::<format::Argb8888>(
            bytemuck::must_cast_slice_mut(&mut self.framebuffer.as_mut()[range]),
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
