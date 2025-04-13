use core::iter::FusedIterator;

use embedded_graphics::prelude::Point;
use embedded_graphics::prelude::Size;

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
