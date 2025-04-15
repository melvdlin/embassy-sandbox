mod eg_compat;
pub mod ext;

pub mod boxes;
pub mod text;

use core::convert::Infallible;

pub use dma2d::format::typelevel as format;
use embedded_graphics::prelude::Dimensions;
use embedded_graphics::prelude::DrawTarget;
use embedded_graphics::prelude::Point;
use embedded_graphics::primitives::Rectangle;

use super::color::Argb8888;
use super::display::dma2d;

type Repr<Format> = <Format as format::Format>::Repr;

/// A trait for hardware accelerated draw targets.
#[allow(async_fn_in_trait)]
pub trait Accelerated: DrawTarget<Color = Argb8888, Error = Infallible> {
    /// Draw a rectangle in the speicifed color.
    async fn fill_rect(&mut self, area: &Rectangle, color: Argb8888);

    /// Copy the source image into this framebuffer.
    ///
    /// # Panics
    ///
    /// Panics if `source.len() != self.len()`
    async fn copy<Format>(&mut self, area: &Rectangle, source: &[Format::Repr])
    where
        Format: format::Format;

    /// Copy the source grayscale image blended with a color
    /// into this framebuffer.
    ///
    /// # Panics
    ///
    /// Panics if `source.len() != self.len()`
    async fn copy_with_color<Format>(
        &mut self,
        area: &Rectangle,
        source: &[Format::Repr],
        color: Argb8888,
    ) where
        Format: format::Grayscale;
}

/// A trait for drawable elements.
#[allow(async_fn_in_trait)]
pub trait Drawable: embedded_graphics::prelude::Dimensions {
    /// Draw `self` onto the active framebuffer layer.
    async fn draw(&self, framebuffer: &mut impl Accelerated, layer: usize);
}

/// Horizontal alignment
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub enum HAlignment {
    /// Aligned to the left edge.
    Left,
    /// Aligned to the center.
    Center,
    /// Aligned to the right edge.
    Right,
}

/// Vertical alignment.
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub enum VAlignment {
    /// Aligned to the top edge.
    Top,
    /// Aligned to the center.
    Center,
    /// Aligned to the bottom.
    Bottom,
}

/// A trait for elements that process touch inputs.
#[allow(async_fn_in_trait)]
pub trait TouchInput: Dimensions {
    /// Input event handler. The position is relative.
    async fn on_input(&mut self, event: TouchEvent);
}

/// A touch input event and position.
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub struct TouchEvent {
    /// The kind of input event.
    pub kind: TouchEventKind,
    /// The position of the input.
    pub positon: Point,
}

/// The kind of input event.
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub enum TouchEventKind {
    /// A new press.
    Press,
    /// A press was lifted.
    Lift,
    /// The screen remains pressed.
    Contact,
}
