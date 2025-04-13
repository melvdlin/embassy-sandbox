use core::ops::Range;

use ascii::AsciiChar;
use embedded_graphics::prelude::Size;

use super::Repr;
use super::format;

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
