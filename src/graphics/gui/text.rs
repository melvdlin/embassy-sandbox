pub mod font;
pub mod textbox;

use core::ops::Range;

use embedded_graphics::prelude::PixelColor;
use embedded_graphics::prelude::Size;

use crate::graphics::color::Storage;

/// A trait for mapping ascii characters to a corresponding image.
pub trait CharMap {
    /// The pixel format.
    type Format: PixelColor;
    /// The pixel dimensions of a single char.
    fn char_size(&self) -> Size;
    /// Get the image of a character, if the character is supported.
    ///
    /// This image must have the dimensions specified in [`AsciiMap::char_dimensions`].
    fn char(&self, char: char) -> Option<&[Storage<Self::Format>]>;
    /// Get the fallback character image.
    ///
    /// This image must have the dimensions specified by [`AsciiMap::char_dimensions`].
    fn fallback(&self) -> &[Storage<Self::Format>];
}

impl<T> CharMap for &T
where
    T: CharMap,
{
    type Format = T::Format;

    fn char_size(&self) -> Size {
        (*self).char_size()
    }

    fn char(&self, char: char) -> Option<&[Storage<Self::Format>]> {
        (*self).char(char)
    }

    fn fallback(&self) -> &[Storage<Self::Format>] {
        (*self).fallback()
    }
}

pub struct CharRangeMap<'a, F>
where
    F: PixelColor,
{
    range: Range<u32>,
    width: u16,
    height: u16,
    chars: &'a [Storage<F>],
    fallback: &'a [Storage<F>],
}

impl<'a, F> CharRangeMap<'a, F>
where
    F: PixelColor,
{
    /// Create a new ascii char map.
    ///
    /// # Panics
    ///
    /// Panics if
    /// + `codepoints.len() * width * height != chars.len()`.
    /// + `width * height != fallback.len()`.
    /// + `width´ does not fit into `usize`.
    /// + `height` does not fit into `usize`.
    pub const fn new(
        codepoints: Range<u32>,
        width: u16,
        height: u16,
        chars: &'a [Storage<F>],
        fallback: &'a [Storage<F>],
    ) -> Self {
        // FIXME: change to `assert_eq` once supported in const
        // FIXME: change to `try_from().expect()` once supported in const
        assert!(width as usize as u16 == width);
        assert!(height as usize as u16 == height);
        let supported_len = codepoints.end.saturating_sub(codepoints.start) as usize;
        let char_pixels = (width as usize).strict_mul(height as usize);

        // FIXME: change to `assert_eq` once supported in const
        assert!(supported_len.strict_mul(char_pixels) == chars.len());
        assert!(char_pixels == fallback.len());

        Self {
            range: codepoints,
            width,
            height,
            chars,
            fallback,
        }
    }
}

impl<F> CharMap for CharRangeMap<'_, F>
where
    F: PixelColor,
{
    type Format = F;

    fn char_size(&self) -> Size {
        Size {
            width: self.width.into(),
            height: self.height.into(),
        }
    }

    fn char(&self, char: char) -> Option<&[Storage<Self::Format>]> {
        let codepoint = u32::from(char);
        if !self.range.contains(&codepoint) {
            return None;
        }
        let idx = (codepoint - self.range.start) as usize;
        let size = self.width as usize * self.height as usize;
        let start = idx * size;
        let end = (idx + 1) * size;
        Some(&self.chars[start..end])
    }

    fn fallback(&self) -> &[Storage<Self::Format>] {
        self.fallback
    }
}
