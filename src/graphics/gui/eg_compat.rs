use embedded_graphics::prelude::Dimensions;
use embedded_graphics::prelude::Drawable as EgDrawable;
use embedded_graphics::primitives::Arc;
use embedded_graphics::primitives::Circle;
use embedded_graphics::primitives::Ellipse;
use embedded_graphics::primitives::Line;
use embedded_graphics::primitives::Polyline;
use embedded_graphics::primitives::PrimitiveStyle;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::primitives::Sector;

use super::Accelerated;
use super::Drawable;
use crate::graphics::accelerated;
use crate::graphics::color::Argb8888;

impl Drawable
    for embedded_graphics::primitives::Styled<Rectangle, PrimitiveStyle<Argb8888>>
{
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        _layer: usize,
    ) {
        if let Some(fill) = self.style.fill_color {
            framebuffer.fill_rect(&self.primitive, fill).await;
        }
        if let Some(stroke) = self.style.stroke_color {
            use embedded_graphics::geometry::AnchorX;
            use embedded_graphics::geometry::AnchorY;
            let bounds = self.bounding_box();
            let width = self.style.stroke_width;
            let up = bounds.resized_height(width, AnchorY::Top);
            let down = bounds.resized_height(width, AnchorY::Top);
            let left = bounds.resized_width(width, AnchorX::Left);
            let right = bounds.resized_width(width, AnchorX::Right);
            framebuffer.fill_rect(&up, stroke).await;
            framebuffer.fill_rect(&down, stroke).await;
            framebuffer.fill_rect(&left, stroke).await;
            framebuffer.fill_rect(&right, stroke).await;
        }
    }
}

impl Drawable
    for embedded_graphics::primitives::Styled<Circle, PrimitiveStyle<Argb8888>>
{
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        _layer: usize,
    ) {
        Ok(()) = <Self as EgDrawable>::draw(self, framebuffer);
    }
}

impl Drawable
    for embedded_graphics::primitives::Styled<Ellipse, PrimitiveStyle<Argb8888>>
{
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        _layer: usize,
    ) {
        Ok(()) = <Self as EgDrawable>::draw(self, framebuffer);
    }
}

impl Drawable
    for embedded_graphics::primitives::Styled<Sector, PrimitiveStyle<Argb8888>>
{
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        _layer: usize,
    ) {
        Ok(()) = <Self as EgDrawable>::draw(self, framebuffer);
    }
}

impl Drawable for embedded_graphics::primitives::Styled<Arc, PrimitiveStyle<Argb8888>> {
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        _layer: usize,
    ) {
        Ok(()) = <Self as EgDrawable>::draw(self, framebuffer);
    }
}

impl Drawable
    for embedded_graphics::primitives::Styled<Polyline<'_>, PrimitiveStyle<Argb8888>>
{
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        _layer: usize,
    ) {
        Ok(()) = <Self as EgDrawable>::draw(self, framebuffer);
    }
}

impl Drawable for embedded_graphics::primitives::Styled<Line, PrimitiveStyle<Argb8888>> {
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        _layer: usize,
    ) {
        if self.primitive.delta().y == 0 {
            let Some(stroke) = self.style.stroke_color else {
                return;
            };
            framebuffer.fill_rect(&self.bounding_box(), stroke).await;
        } else {
            Ok(()) = <Self as EgDrawable>::draw(self, framebuffer);
        }
    }
}
