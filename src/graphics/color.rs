use core::fmt::Display;
use core::fmt::LowerHex;
use core::fmt::UpperHex;

use embedded_graphics::pixelcolor::Bgr555;
use embedded_graphics::pixelcolor::Bgr565;
use embedded_graphics::pixelcolor::Bgr666;
use embedded_graphics::pixelcolor::Bgr888;
use embedded_graphics::pixelcolor::Rgb555;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::Rgb666;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::pixelcolor::raw::RawU32;
use embedded_graphics::prelude::RawData;
use embedded_graphics::prelude::RgbColor;

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(PartialOrd, Ord)]
#[derive(Hash)]
#[repr(C)]
pub struct Argb8888 {
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Argb8888 {
    pub const fn new(alpha: u8, red: u8, green: u8, blue: u8) -> Self {
        Self {
            alpha,
            red,
            green,
            blue,
        }
    }

    pub const fn from_u32(value: u32) -> Self {
        let [alpha, red, green, blue] = value.to_ne_bytes();
        Self {
            alpha,
            red,
            green,
            blue,
        }
    }

    pub const fn into_u32(self) -> u32 {
        let Self {
            alpha,
            red,
            green,
            blue,
        } = self;
        u32::from_ne_bytes([alpha, red, green, blue])
    }

    pub const fn blend(self, other: Self) -> Self {
        const fn blend_component(alpha_a: u8, comp_a: u8, alpha_b: u8, comp_b: u8) -> u8 {
            let a = comp_a as u32 * alpha_a as u32;
            let b = comp_b as u32 * alpha_b as u32 * (0xFF - alpha_a as u32) / 0xFF;

            ((a + b) / 0xFF) as u8
        }
        Self {
            red: blend_component(self.alpha, self.red, other.alpha, other.red),
            green: blend_component(self.alpha, self.green, other.alpha, other.green),
            blue: blend_component(self.alpha, self.blue, other.alpha, other.blue),
            alpha: self.alpha
                + (other.alpha as u32 * (0xFF - self.alpha as u32) / 0xFF) as u8,
        }
    }

    pub fn a(&self) -> u8 {
        self.alpha
    }
}

impl Display for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        LowerHex::fmt(self, f)
    }
}

impl LowerHex for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let &Self {
            alpha,
            red,
            green,
            blue,
        } = self;
        write!(f, "#{:02x}{:02x}{:02x}{:02x}", alpha, red, green, blue)
    }
}

impl UpperHex for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let &Self {
            alpha,
            red,
            green,
            blue,
        } = self;
        write!(f, "#{:02X}{:02X}{:02X}{:02X}", alpha, red, green, blue)
    }
}

impl embedded_graphics::pixelcolor::PixelColor for Argb8888 {
    type Raw = RawU32;
}

impl From<u32> for Argb8888 {
    fn from(value: u32) -> Self {
        Self::from_u32(value)
    }
}

impl From<Argb8888> for u32 {
    fn from(argb: Argb8888) -> Self {
        argb.into_u32()
    }
}

impl From<RawU32> for Argb8888 {
    fn from(raw: RawU32) -> Self {
        Self::from(raw.into_inner())
    }
}

impl From<Argb8888> for RawU32 {
    fn from(argb: Argb8888) -> Self {
        RawU32::new(u32::from(argb))
    }
}

macro_rules! impl_from_rgb {
    ($type:ty, $from:ty) => {
        impl From<$from> for $type {
            fn from(rgb: $from) -> Self {
                let red_shift = <$from>::MAX_R.leading_zeros();
                let green_shift = <$from>::MAX_G.leading_zeros();
                let blue_shift = <$from>::MAX_B.leading_zeros();
                Self::new(
                    u8::MAX,
                    rgb.r() << red_shift,
                    rgb.g() << green_shift,
                    rgb.b() << blue_shift,
                )
            }
        }
    };
}

impl_from_rgb!(Argb8888, Rgb888);
impl_from_rgb!(Argb8888, Rgb666);
impl_from_rgb!(Argb8888, Rgb555);
impl_from_rgb!(Argb8888, Rgb565);
impl_from_rgb!(Argb8888, Bgr888);
impl_from_rgb!(Argb8888, Bgr666);
impl_from_rgb!(Argb8888, Bgr555);
impl_from_rgb!(Argb8888, Bgr565);

impl RgbColor for Argb8888 {
    fn r(&self) -> u8 {
        self.red
    }

    fn g(&self) -> u8 {
        self.green
    }

    fn b(&self) -> u8 {
        self.blue
    }

    const MAX_R: u8 = u8::MAX;

    const MAX_G: u8 = u8::MAX;

    const MAX_B: u8 = u8::MAX;

    const BLACK: Self = Self::new(u8::MAX, 0, 0, 0);

    const RED: Self = Self::new(u8::MAX, Self::MAX_R, 0, 0);

    const GREEN: Self = Self::new(u8::MAX, 0, Self::MAX_G, 0);

    const BLUE: Self = Self::new(u8::MAX, 0, 0, Self::MAX_B);

    const YELLOW: Self = Self::new(u8::MAX, Self::MAX_R, Self::MAX_G, 0);

    const MAGENTA: Self = Self::new(u8::MAX, Self::MAX_R, 0, Self::MAX_B);

    const CYAN: Self = Self::new(u8::MAX, 0, Self::MAX_G, Self::MAX_B);

    const WHITE: Self = Self::new(u8::MAX, Self::MAX_R, Self::MAX_G, Self::MAX_B);
}
