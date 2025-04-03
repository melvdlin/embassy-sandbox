use core::marker::PhantomData;
use core::ops::RangeBounds;

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub struct Image<'a, P> {
    buf: &'a [P],
    rows: usize,
    cols: usize,
    first_row: usize,
    first_col: usize,
}

impl<'a, P> Image<'a, P> {
    pub const fn new(buf: &'a [P], rows: usize, cols: usize) -> Self {
        /// FIXME: use `assert_eq` once it's const
        assert!(buf.len() == rows * cols);
        Self {
            buf,
            rows,
            cols,
            first_row: 0,
            first_col: 0,
        }
    }

    pub fn slice(
        &self,
        rows: impl RangeBounds<usize>,
        cols: impl RangeBounds<usize>,
    ) -> Self {
        let rows = core::slice::range(rows, ..self.rows);
        let cols = core::slice::range(cols, ..self.rows);

        let first_row = self.first_row + rows.start;
        let rows = self.first_row + rows.len();
        let first_col = self.first_row + cols.start;
        let cols = self.first_row + cols.len();

        Self {
            rows,
            cols,
            first_row,
            first_col,
            ..*self
        }
    }

    pub fn rows(&self, rows: impl RangeBounds<usize>) -> Self {
        self.slice(rows, ..)
    }

    pub fn cols(&self, rows: impl RangeBounds<usize>) -> Self {
        self.slice(rows, ..)
    }

    pub fn try_slice(
        &self,
        rows: impl RangeBounds<usize>,
        cols: impl RangeBounds<usize>,
    ) -> Option<Self> {
        let rows = core::slice::try_range(rows, ..self.rows)?;
        let cols = core::slice::try_range(cols, ..self.rows)?;

        let first_row = self.first_row + rows.start;
        let rows = self.first_row + rows.len();
        let first_col = self.first_row + cols.start;
        let cols = self.first_row + cols.len();

        Some(Self {
            rows,
            cols,
            first_row,
            first_col,
            ..*self
        })
    }

    pub fn try_rows(&self, rows: impl RangeBounds<usize>) -> Option<Self> {
        self.try_slice(rows, ..)
    }

    pub fn try_cols(&self, rows: impl RangeBounds<usize>) -> Option<Self> {
        self.try_slice(rows, ..)
    }

    pub const fn nrows(&self) -> usize {
        self.rows
    }

    pub const fn ncols(&self) -> usize {
        self.cols
    }

    pub const fn len(&self) -> usize {
        self.rows * self.cols
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
