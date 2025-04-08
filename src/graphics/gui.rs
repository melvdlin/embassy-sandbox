mod eg_compat;
pub mod ext;

use core::iter::FusedIterator;
use core::ops::Range;

use ascii::AsciiChar;
use dma2d::format::typelevel as format;
use embedded_graphics::prelude::DrawTarget;
use embedded_graphics::prelude::Point;
use embedded_graphics::prelude::Size;
use embedded_graphics::primitives::Rectangle;

use super::accelerated;
use super::color::Argb8888;
use super::display::dma2d;

type Repr<Format> = <Format as format::Format>::Repr;

#[allow(async_fn_in_trait)]
pub trait Accelerated: DrawTarget {
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

#[allow(async_fn_in_trait)]
pub trait Drawable: embedded_graphics::prelude::Dimensions {
    async fn draw(
        &self,
        framebuffer: &mut accelerated::Framebuffer<'_, &mut [Argb8888]>,
        layer: usize,
    );
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub enum HAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub enum VAlignment {
    Left,
    Center,
    Right,
}

/// A trait for mapping ascii characters to a corresponding image.
pub trait AsciiMap {
    /// The pixel format.
    type Format: format::Format;
    /// The pixel dimensions of a single char.
    fn char_dimensions(&self) -> Size;
    /// Get the image of a character, if the character is supported.
    ///
    /// This image must have the dimensions specified in [`AsciiMap::char_dimensions`].
    fn char(&self, c: AsciiChar) -> Option<&[Repr<Self::Format>]>;
    /// Get the fallback character image.
    ///
    /// This image must have the dimensions specified by [`AsciiMap::char_dimensions`].
    fn fallback(&self) -> &[Repr<Self::Format>];
}

pub struct PrintableAsciiMap<'a, F>
where
    F: format::Format,
{
    range: Range<u8>,
    width: u16,
    height: u16,
    chars: &'a [Repr<F>],
    fallback: &'a [Repr<F>],
}

impl<'a, F> PrintableAsciiMap<'a, F>
where
    F: format::Format,
{
    /// Create a new ascii char map.
    ///
    /// # Panics
    ///
    /// Panics if
    /// + `supported.len() * width * height != chars.len()`.
    /// + `width * height != fallback.len()`.
    /// + `width´ does not fit into `usize`.
    /// + `height` does not fit into `usize`.
    pub const fn new(
        supported: Range<u8>,
        width: u16,
        height: u16,
        chars: &'a [Repr<F>],
        fallback: &'a [Repr<F>],
    ) -> Self {
        // FIXME: change to `assert_eq` once supported in const
        // FIXME: change to `try_from().expect()` once supported in const
        assert!(width as usize as u16 == width);
        assert!(height as usize as u16 == height);
        let supported_len = supported.end.saturating_sub(supported.start) as usize;
        let char_pixels = (width as usize).strict_mul(height as usize);

        // FIXME: change to `assert_eq` once supported in const
        assert!(supported_len.strict_mul(char_pixels) == chars.len());
        assert!(char_pixels == fallback.len());

        Self {
            range: supported,
            width,
            height,
            chars,
            fallback,
        }
    }
}

impl<F> AsciiMap for PrintableAsciiMap<'_, F>
where
    F: format::Format,
{
    type Format = F;

    fn char_dimensions(&self) -> Size {
        Size {
            width: self.width.into(),
            height: self.height.into(),
        }
    }

    fn char(&self, char: AsciiChar) -> Option<&[Repr<Self::Format>]> {
        if !self.range.contains(&char.as_byte()) {
            return None;
        }
        let idx = (char.as_byte() - self.range.start) as usize;
        let size = self.width as usize * self.height as usize;
        Some(&self.chars[idx..idx + size])
    }

    fn fallback(&self) -> &[Repr<Self::Format>] {
        self.fallback
    }
}

pub trait TouchInput {
    fn on_input(&mut self, event: TouchEvent);
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub struct TouchEvent {
    pub kind: TouchEventKind,
    pub positon: Point,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord)]
#[derive(Hash)]
pub enum TouchEventKind {
    Press,
    Lift,
    Contact,
}

pub struct GridLayout<const ROWS: usize, const COLS: usize> {
    pub heights: [u32; ROWS],
    pub widths: [u32; COLS],
    pub h_spacing: u32,
    pub v_spacing: u32,
}

impl<const ROWS: usize, const COLS: usize> GridLayout<ROWS, COLS> {
    const ASSERT_NONZERO: () = {
        assert!(ROWS > 0);
        assert!(COLS > 0);
    };

    pub fn new(elements: &[[Size; COLS]; ROWS], h_spacing: u32, v_spacing: u32) -> Self {
        #[allow(clippy::let_unit_value)]
        {
            _ = Self::ASSERT_NONZERO;
        }

        let rows = elements
            .map(|row| row.into_iter().map(|size| size.height).max().unwrap_or(0));
        let cols = core::array::from_fn(|i| {
            elements.map(|row| row[i].width).into_iter().max().unwrap_or(0)
        });
        Self {
            heights: rows,
            widths: cols,
            h_spacing,
            v_spacing,
        }
    }

    pub fn total_height(&self) -> u32 {
        Self::total_size(&self.heights, self.v_spacing)
    }

    pub fn total_width(&self) -> u32 {
        Self::total_size(&self.widths, self.h_spacing)
    }

    fn total_size(elements: &[u32], spacing: u32) -> u32 {
        elements.iter().sum::<u32>() + (elements.len() - 1) as u32 * spacing
    }

    pub fn row_offsets(&self, starting_offset: u32) -> BoxOffsets<'_> {
        BoxOffsets::new(starting_offset, &self.widths, self.h_spacing)
    }

    pub fn column_offsets(&self, starting_offset: u32) -> BoxOffsets<'_> {
        BoxOffsets::new(starting_offset, &self.heights, self.v_spacing)
    }

    pub fn layout(&self, start: Point) -> GridLayoutPoints<'_> {
        GridLayoutPoints::new(
            start,
            &self.widths,
            &self.heights,
            self.h_spacing,
            self.v_spacing,
        )
    }
}

#[derive(Debug)]
#[derive(Clone)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[derive(Default)]
pub struct GridLayoutPoints<'a> {
    widths: &'a [u32],
    heights: &'a [u32],
    h_spacing: u32,
    v_spacing: u32,
    origin_x: i32,
    next: Point,
    current_row: &'a [u32],
}

impl<'a> GridLayoutPoints<'a> {
    pub const fn new(
        start: Point,
        widths: &'a [u32],
        heights: &'a [u32],
        h_spacing: u32,
        v_spacing: u32,
    ) -> Self {
        Self {
            widths,
            heights,
            h_spacing,
            v_spacing,
            origin_x: start.x,
            next: start,
            current_row: widths,
        }
    }
}

impl Iterator for GridLayoutPoints<'_> {
    type Item = Point;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.next;
        if let Some((&head, tail)) = self.current_row.split_first() {
            self.current_row = tail;
            self.next.x += (head + self.h_spacing) as i32;
        } else if let Some((&head, tail)) = self.heights.split_first() {
            self.heights = tail;
            self.current_row = self.widths;
            self.next.x = self.origin_x;
            self.next.y += (head + self.v_spacing) as i32;
        } else {
            return None;
        }

        Some(next)
    }
}

#[derive(Debug)]
#[derive(Clone)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[derive(Default)]
pub struct BoxOffsets<'a> {
    elements: &'a [u32],
    spacing: u32,
    next: u32,
}

impl<'a> BoxOffsets<'a> {
    pub const fn empty() -> Self {
        Self {
            elements: &[],
            spacing: 0,
            next: 0,
        }
    }

    pub const fn new(start: u32, elements: &'a [u32], spacing: u32) -> Self {
        Self {
            elements,
            spacing,
            next: start,
        }
    }
}

impl Iterator for BoxOffsets<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        let (head, tail) = self.elements.split_first()?;
        let next = self.next;
        self.next = next + head + self.spacing;
        self.elements = tail;

        Some(next)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl ExactSizeIterator for BoxOffsets<'_> {
    fn len(&self) -> usize {
        self.elements.len()
    }
}

impl FusedIterator for BoxOffsets<'_> {}
