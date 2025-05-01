use core::cmp;
use core::iter::FusedIterator;

use embedded_graphics::prelude::OriginDimensions;
use embedded_graphics::prelude::Point;
use embedded_graphics::prelude::Size;
use embedded_graphics::primitives::Rectangle;
use itertools::Either;

use super::CharMap;
use crate::graphics::color::AlphaColor;
use crate::graphics::color::Argb8888;
use crate::graphics::gui::Accelerated;
use crate::graphics::gui::Alignment;
use crate::graphics::gui::Drawable;
use crate::graphics::gui::HAlignment;
use crate::graphics::gui::VAlignment;

// TODO: add alignment support
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Default)]
pub struct Layout<C> {
    pub char_map: C,
    pub cols: usize,
    pub rows: usize,
}

impl<C> Layout<C> {
    pub const fn new(char_map: C, cols: usize, rows: usize) -> Self {
        Self {
            char_map,
            cols,
            rows,
        }
    }
}

impl<C> Layout<C>
where
    C: CharMap,
{
    pub fn position(&self, i: usize) -> Option<Point> {
        let row = i / self.cols;
        let col = i % self.cols;
        let Size { width, height } = self.char_map.char_size();
        (row < self.rows).then_some(Point {
            x: col as i32 * width as i32,
            y: row as i32 * height as i32,
        })
    }

    pub fn positions(&self, first: usize) -> Positions<'_, C> {
        Positions {
            layout: self,
            i: first,
        }
    }

    pub fn positions_from(&self, row: usize, col: usize) -> PositionsFrom<'_, C> {
        PositionsFrom {
            layout: self,
            row,
            col,
        }
    }
}

impl<C> OriginDimensions for Layout<C>
where
    C: CharMap,
{
    fn size(&self) -> Size {
        self.char_map.char_size().component_mul(Size {
            width: self.cols as _,
            height: self.rows as _,
        })
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Default)]
pub struct AlignedLayout<C> {
    pub layout: Layout<C>,
    pub align: Alignment,
}

impl<C> AlignedLayout<C> {
    pub fn new(layout: Layout<C>, align: Alignment) -> Self {
        Self { layout, align }
    }
}

impl<C> AlignedLayout<C>
where
    C: CharMap,
{
    pub fn positions<L>(
        &self,
        line_lengths: impl IntoIterator<IntoIter = L>,
    ) -> AlignedPositions<'_, C, L>
    where
        C: CharMap,
        L: Iterator<Item = usize>,
        L: Clone,
    {
        AlignedPositions::new(self, line_lengths)
    }
}

impl<C> OriginDimensions for AlignedLayout<C>
where
    C: CharMap,
{
    fn size(&self) -> Size {
        self.layout.size()
    }
}

pub struct AlignedPositions<'a, C, L> {
    layout: &'a AlignedLayout<C>,
    lines: L,
    current_line_len: usize,
    row: usize,
    col: usize,
    v_half_shift: bool,
    h_half_shift: bool,
}

impl<C, L> AlignedPositions<'_, C, L> {
    fn is_exhausted(&self) -> bool {
        #[allow(clippy::nonminimal_bool)]
        !(self.row < self.layout.layout.rows)
    }

    fn exhaust(&mut self) {
        self.row = self.layout.layout.rows;
    }
}

impl<'a, C, L> AlignedPositions<'a, C, L>
where
    C: CharMap,
    L: Iterator<Item = usize>,
    L: Clone,
{
    pub fn new(
        layout: &'a AlignedLayout<C>,
        line_lengths: impl IntoIterator<IntoIter = L>,
    ) -> Self {
        let mut line_lengths = line_lengths.into_iter();
        let (row, v_half_shift) = if layout.align.v == VAlignment::Top {
            (0, false)
        } else {
            let total_height = line_lengths
                .clone()
                // empty lines don't get discarded
                .map(|len| cmp::max(1, len).div_ceil(layout.layout.cols))
                .sum::<usize>();
            let free = layout.layout.rows.saturating_sub(total_height);
            if layout.align.v == VAlignment::Bottom {
                (free, false)
            } else {
                (free / 2, free % 2 != 0)
            }
        };

        let mut iter = Self {
            layout,
            current_line_len: line_lengths.next().unwrap_or(0),
            lines: line_lengths,
            row,
            col: 0,
            v_half_shift,
            h_half_shift: false,
        };
        if iter.layout.layout.cols == 0 {
            iter.exhaust();
        } else {
            (iter.col, iter.h_half_shift) = iter.first_col();
        }

        iter
    }
}

impl<C, L> AlignedPositions<'_, C, L>
where
    C: CharMap,
    L: Iterator<Item = usize>,
{
    /// Returns the column index and horizontal half-column shift
    /// of the first character in the current line and row.
    fn first_col(&self) -> (usize, bool) {
        if self.layout.align.h == HAlignment::Left {
            (0, false)
        } else {
            let free = self.layout.layout.cols.saturating_sub(self.current_line_len);
            if self.layout.align.h == HAlignment::Right {
                (free, false)
            } else {
                (free / 2, free % 2 != 0)
            }
        }
    }

    /// Set the cursor to the start of the next, non-empty line that fits within the layout.
    /// Exhausts `self` and returns `false` iff no such line exists.
    fn next_nonempty_line(&mut self) -> bool {
        while self.current_line_len == 0 && !self.is_exhausted() {
            let Some(line) = self.lines.next() else {
                self.exhaust();
                break;
            };
            self.current_line_len = line;
            self.row += 1;
        }

        if self.is_exhausted() {
            return false;
        }

        (self.col, self.h_half_shift) = self.first_col();

        true
    }

    /// Advance the cursor to the next position.
    /// Returns `false` iff this results in `self` being exhausted.
    fn advance(&mut self) -> bool {
        if self.current_line_len != 0 {
            self.col += 1;
            if self.col + self.h_half_shift as (usize) < self.layout.layout.cols {
                return true;
            }
            self.row += 1;
            if self.is_exhausted() {
                return false;
            }
            (self.col, self.h_half_shift) = self.first_col();
            true
        } else {
            self.next_nonempty_line()
        }
    }
}

impl<C, L> Iterator for AlignedPositions<'_, C, L>
where
    C: CharMap,
    L: Iterator<Item = usize>,
{
    type Item = Point;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_exhausted() {
            return None;
        }

        self.current_line_len -= 1;

        let size = self.layout.layout.char_map.char_size();
        let size = Point {
            x: size.width as i32,
            y: size.height as i32,
        };
        let base = size.component_mul(Point {
            x: self.col as i32,
            y: self.row as i32,
        });
        let shift = size.component_mul(Point {
            x: self.h_half_shift as i32,
            y: self.v_half_shift as i32,
        }) / 2;
        let next = base + shift;

        self.advance();

        Some(next)
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct Positions<'a, C> {
    layout: &'a Layout<C>,
    i: usize,
}

impl<C> Iterator for Positions<'_, C>
where
    C: CharMap,
{
    type Item = Point;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.layout.position(self.i)?;
        self.i += 1;
        Some(next)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<C> ExactSizeIterator for Positions<'_, C>
where
    Self: Iterator,
{
    fn len(&self) -> usize {
        let total = self.layout.cols * self.layout.rows;
        total - self.i
    }
}

impl<C> FusedIterator for Positions<'_, C> where Self: Iterator {}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct PositionsFrom<'a, C> {
    layout: &'a Layout<C>,
    row: usize,
    col: usize,
}

impl<C> PositionsFrom<'_, C> {
    pub fn line_break(&mut self) {
        if self.row < self.layout.rows {
            self.row += 1;
            self.col = 0;
        }
    }
}

impl<C> Iterator for PositionsFrom<'_, C>
where
    C: CharMap,
{
    type Item = Point;

    fn next(&mut self) -> Option<Self::Item> {
        if self.row >= self.layout.rows || self.col >= self.layout.cols {
            return None;
        }

        let Size { width, height } = self.layout.char_map.char_size();
        let next = Point {
            x: self.col as i32 * width as i32,
            y: self.row as i32 * height as i32,
        };

        self.col += 1;
        if self.col >= self.layout.cols {
            self.line_break();
        }

        Some(next)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<C> ExactSizeIterator for PositionsFrom<'_, C>
where
    Self: Iterator,
{
    fn len(&self) -> usize {
        let total = self.layout.cols * self.layout.rows;
        let next = self.row * self.layout.cols + self.col;
        total.saturating_sub(next)
    }
}

impl<C> FusedIterator for PositionsFrom<'_, C> where Self: Iterator {}

// TODO: add alignment support
pub struct TextBox<C, S> {
    pub content: S,
    pub color: Argb8888,
    pub layout: AlignedLayout<C>,
    pub layer: usize,
    pub line_break_aware: bool,
}

impl<C, S> TextBox<C, S> {
    pub const fn new(
        content: S,
        color: Argb8888,
        layout: AlignedLayout<C>,
        layer: usize,
        line_break_aware: bool,
    ) -> Self {
        Self {
            layout,
            color,
            content,
            layer,
            line_break_aware,
        }
    }
}

impl<C, S> OriginDimensions for TextBox<C, S>
where
    C: CharMap,
{
    fn size(&self) -> Size {
        self.layout.size()
    }
}

impl<C, S> Drawable<C::Format> for TextBox<C, S>
where
    C: CharMap,
    C::Format: AlphaColor,
    S: AsRef<str>,
{
    async fn draw(&self, framebuffer: &mut impl Accelerated<C::Format>, layer: usize) {
        if layer != self.layer {
            return;
        }
        let layout = &self.layout;
        let char_map = &layout.layout.char_map;

        let positions = self.layout.positions(if self.line_break_aware {
            Either::Left(self.content.as_ref().lines().map(str::len))
        } else {
            Either::Right(core::iter::once(self.content.as_ref().len()))
        });

        for (char, position) in self
            .content
            .as_ref()
            .chars()
            .filter(|char| !(self.line_break_aware && matches!(char, '\n' | '\r')))
            .zip(positions)
        {
            let char = char_map.char(char).unwrap_or(char_map.fallback());

            framebuffer
                .copy_with_color(
                    &Rectangle::new(position, char_map.char_size()),
                    char,
                    self.color,
                    true,
                )
                .await
        }
    }
}
