use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

use super::Accelerated;
use super::AcceleratedBase;
use crate::graphics::color::Argb8888;
use crate::graphics::color::Format;
use crate::graphics::color::Grayscale;

pub trait AcceleratedExt {
    fn translated(&mut self, offset: Point) -> Translated<'_, Self>;
}

impl<A> AcceleratedExt for A
where
    A: AcceleratedBase,
{
    fn translated(&mut self, offset: Point) -> Translated<'_, Self> {
        Translated {
            offset,
            surface: self,
        }
    }
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

impl<A> AcceleratedBase for Translated<'_, A>
where
    A: AcceleratedBase,
{
    async fn fill_rect(&mut self, area: &Rectangle, color: Argb8888) {
        self.surface.fill_rect(&area.translate(self.offset), color).await
    }
}

impl<F, A> Accelerated<F> for Translated<'_, A>
where
    F: Format,
    A: Accelerated<F>,
{
    async fn copy(&mut self, area: &Rectangle, source: &[F::Repr], blend: bool) {
        self.surface.copy(&area.translate(self.offset), source, blend).await
    }

    async fn copy_with_color(
        &mut self,
        area: &Rectangle,
        source: &[F::Repr],
        color: Argb8888,
        blend: bool,
    ) where
        F: Grayscale,
    {
        self.surface
            .copy_with_color(&area.translate(self.offset), source, color, blend)
            .await
    }
}
