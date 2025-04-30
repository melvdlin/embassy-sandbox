mod eg_compat;
pub mod ext;

pub mod boxes;
pub mod text;

use core::convert::Infallible;
use core::ops::BitOr;
use core::ops::BitOrAssign;

use embedded_graphics::prelude::Dimensions;
use embedded_graphics::prelude::DrawTarget;
use embedded_graphics::prelude::Point;
use embedded_graphics::primitives::Rectangle;

use super::color::Argb8888;
use super::color::Format;
use super::color::Grayscale;
pub use super::display::dma2d::format::typelevel as format;

type Repr<F> = <F as Format>::Repr;

/// A trait for draw targets with accelerated primitives.
#[allow(async_fn_in_trait)]
pub trait AcceleratedBase: DrawTarget<Color = Argb8888, Error = Infallible> {
    /// Draw a rectangle in the speicifed color.
    async fn fill_rect(&mut self, area: &Rectangle, color: Argb8888);
}

/// A trait for draw targets with hardware accelerated copying.
#[allow(async_fn_in_trait)]
pub trait Accelerated<F: Format>: AcceleratedBase {
    /// Copy the source image into this framebuffer,
    /// optionally blending it with the current framebuffer content.
    ///
    /// # Panics
    ///
    /// Panics if `source.len() != self.len()`
    async fn copy(&mut self, area: &Rectangle, source: &[F::Repr], blend: bool);

    /// Copy the source grayscale image blended with a color
    /// into this framebuffer,
    /// optionally blending it with the current framebuffer content.
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
        F: Grayscale;
}

/// A trait for drawable elements.
#[allow(async_fn_in_trait)]
pub trait Drawable<F: Format>: embedded_graphics::prelude::Dimensions {
    /// Draw `self` onto the active framebuffer layer.
    async fn draw(&self, framebuffer: &mut impl Accelerated<F>, layer: usize);
}

/// Horizontal alignment
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
#[derive(Default)]
pub enum HAlignment {
    /// Aligned to the left edge.
    #[default]
    Left,
    /// Aligned to the center.
    Center,
    /// Aligned to the right edge.
    Right,
}

impl HAlignment {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];

    pub const fn with_v(self, v: VAlignment) -> Alignment {
        Alignment::new(self, v)
    }

    pub const fn top(self) -> Alignment {
        self.with_v(VAlignment::Top)
    }

    pub const fn center(self) -> Alignment {
        self.with_v(VAlignment::Center)
    }

    pub const fn bottom(self) -> Alignment {
        self.with_v(VAlignment::Bottom)
    }
}

/// Vertical alignment.
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
#[derive(Default)]
pub enum VAlignment {
    /// Aligned to the top edge.
    #[default]
    Top,
    /// Aligned to the center.
    Center,
    /// Aligned to the bottom.
    Bottom,
}

impl VAlignment {
    pub const ALL: [Self; 3] = [Self::Top, Self::Center, Self::Bottom];

    pub const fn with_h(self, h: HAlignment) -> Alignment {
        Alignment::new(h, self)
    }

    pub const fn left(self) -> Alignment {
        self.with_h(HAlignment::Left)
    }

    pub const fn center(self) -> Alignment {
        self.with_h(HAlignment::Center)
    }
    pub const fn right(self) -> Alignment {
        self.with_h(HAlignment::Right)
    }
}

/// Horizontal and Vertical Alignment.
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
#[derive(Default)]
pub struct Alignment {
    /// Horizontal alignment.
    pub h: HAlignment,
    /// Vertical Alignment.
    pub v: VAlignment,
}

impl Alignment {
    pub const fn new(h: HAlignment, v: VAlignment) -> Self {
        Self { h, v }
    }

    pub const fn with_h(self, h: HAlignment) -> Self {
        Self { h, ..self }
    }

    pub const fn with_v(self, v: VAlignment) -> Self {
        Self { v, ..self }
    }

    pub const fn left(self) -> Self {
        self.with_h(HAlignment::Left)
    }

    pub const fn h_center(self) -> Self {
        self.with_h(HAlignment::Center)
    }

    pub const fn right(self) -> Self {
        self.with_h(HAlignment::Right)
    }

    pub const fn top(self) -> Self {
        self.with_v(VAlignment::Top)
    }

    pub const fn v_center(self) -> Self {
        self.with_v(VAlignment::Center)
    }

    pub const fn bottom(self) -> Self {
        self.with_v(VAlignment::Bottom)
    }

    pub const TOP_LEFT: Self = Self::new(HAlignment::Left, VAlignment::Top);
    pub const TOP_CENTER: Self = Self::TOP_LEFT.h_center();
    pub const TOP_RIGHT: Self = Self::TOP_LEFT.right();
    pub const CENTER_LEFT: Self = Self::TOP_LEFT.v_center();
    pub const CENTER: Self = Self::CENTER_LEFT.h_center();
    pub const CENTER_RIGHT: Self = Self::CENTER_LEFT.right();
    pub const BOTTOM_LEFT: Self = Self::TOP_LEFT.bottom();
    pub const BOTTOM_CENTER: Self = Self::BOTTOM_LEFT.h_center();
    pub const BOTTOM_RIGHT: Self = Self::BOTTOM_LEFT.right();
    pub const ALL: [Self; 9] = [
        Self::TOP_LEFT,
        Self::TOP_CENTER,
        Self::TOP_RIGHT,
        Self::CENTER_LEFT,
        Self::CENTER,
        Self::CENTER_RIGHT,
        Self::BOTTOM_LEFT,
        Self::BOTTOM_CENTER,
        Self::BOTTOM_RIGHT,
    ];
}

impl BitOr<VAlignment> for HAlignment {
    type Output = Alignment;

    fn bitor(self, rhs: VAlignment) -> Self::Output {
        self.with_v(rhs)
    }
}

impl BitOr<HAlignment> for VAlignment {
    type Output = Alignment;

    fn bitor(self, rhs: HAlignment) -> Self::Output {
        self.with_h(rhs)
    }
}

impl BitOr<HAlignment> for Alignment {
    type Output = Self;

    fn bitor(self, rhs: HAlignment) -> Self::Output {
        self.with_h(rhs)
    }
}

impl BitOr<VAlignment> for Alignment {
    type Output = Self;

    fn bitor(self, rhs: VAlignment) -> Self::Output {
        self.with_v(rhs)
    }
}

impl BitOrAssign<HAlignment> for Alignment {
    fn bitor_assign(&mut self, rhs: HAlignment) {
        self.h = rhs;
    }
}

impl BitOrAssign<VAlignment> for Alignment {
    fn bitor_assign(&mut self, rhs: VAlignment) {
        self.v = rhs;
    }
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
