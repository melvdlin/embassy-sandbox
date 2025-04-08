use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

use super::Accelerated;
use super::dma2d::format::typelevel as format;
use crate::graphics::color::Argb8888;

pub trait AcceleratedExt {
    fn translated(&mut self, offset: Point) -> Translated<'_, Self>;
}

pub struct Translated<'a, A: ?Sized> {
    pub offset: Point,
    pub surface: &'a mut A,
}

impl<A> Dimensions for Translated<'_, A>
where
    A: Dimensions,
{
    fn bounding_box(&self) -> Rectangle {
        self.surface.bounding_box().translate(-self.offset)
    }
}

impl<A> DrawTarget for Translated<'_, A>
where
    A: DrawTarget,
{
    type Color = A::Color;

    type Error = A::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        <A as embedded_graphics::draw_target::DrawTargetExt>::translated(
            self.surface,
            self.offset,
        )
        .draw_iter(pixels)
    }
    fn fill_contiguous<I>(
        &mut self,
        area: &Rectangle,
        colors: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        <A as embedded_graphics::draw_target::DrawTargetExt>::translated(
            self.surface,
            self.offset,
        )
        .fill_contiguous(area, colors)
    }

    fn fill_solid(
        &mut self,
        area: &Rectangle,
        color: Self::Color,
    ) -> Result<(), Self::Error> {
        <A as embedded_graphics::draw_target::DrawTargetExt>::translated(
            self.surface,
            self.offset,
        )
        .fill_solid(area, color)
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        <A as embedded_graphics::draw_target::DrawTargetExt>::translated(
            self.surface,
            self.offset,
        )
        .clear(color)
    }
}

impl<A> Accelerated for Translated<'_, A>
where
    A: Accelerated,
{
    async fn copy<Format>(&mut self, area: &Rectangle, source: &[Format::Repr])
    where
        Format: format::Format,
    {
        self.surface.copy::<Format>(&area.translate(self.offset), source).await
    }

    async fn copy_with_color<Format>(
        &mut self,
        area: &Rectangle,
        source: &[Format::Repr],
        color: Argb8888,
    ) where
        Format: format::Grayscale,
    {
        self.surface
            .copy_with_color::<Format>(&area.translate(self.offset), source, color)
            .await
    }
}
