use core::iter::FusedIterator;

use ascii::AsciiChar;
use embedded_graphics::prelude::OriginDimensions;
use embedded_graphics::prelude::Point;
use embedded_graphics::prelude::Size;
use embedded_graphics::primitives::Rectangle;

use super::AsciiMap;
use crate::graphics::color::Argb8888;
use crate::graphics::gui::Accelerated;
use crate::graphics::gui::Drawable;
use crate::graphics::gui::format::Grayscale;

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
    C: AsciiMap,
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
    C: AsciiMap,
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
pub struct Positions<'a, C> {
    layout: &'a Layout<C>,
    i: usize,
}

impl<C> Iterator for Positions<'_, C>
where
    C: AsciiMap,
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
    C: AsciiMap,
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
    pub layout: Layout<C>,
    pub layer: usize,
    pub line_break_aware: bool,
}

impl<C, S> TextBox<C, S> {
    pub const fn new(
        content: S,
        color: Argb8888,
        layout: Layout<C>,
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
    C: AsciiMap,
{
    fn size(&self) -> Size {
        self.layout.size()
    }
}

impl<C, S> Drawable for TextBox<C, S>
where
    C: AsciiMap,
    C::Format: Grayscale,
    S: AsRef<str>,
{
    async fn draw(&self, framebuffer: &mut impl Accelerated, layer: usize) {
        if layer != self.layer {
            return;
        }
        let layout = &self.layout;
        let char_map = &layout.char_map;

        let mut positions = self.layout.positions_from(0, 0);

        for char in self.content.as_ref().chars() {
            if self.line_break_aware {
                if char == '\n' {
                    positions.line_break();
                    continue;
                }
                if char == '\r' {
                    continue;
                }
            }

            let Some(position) = positions.next() else {
                break;
            };

            let char = AsciiChar::from_ascii(char)
                .ok()
                .and_then(|char| char_map.char(char))
                .unwrap_or(char_map.fallback());

            framebuffer
                .copy_with_color::<C::Format>(
                    &Rectangle::new(position, char_map.char_size()),
                    char,
                    self.color,
                )
                .await
        }
    }
}
