use core::fmt::Display;
use core::fmt::LowerHex;
use core::fmt::UpperHex;

use embedded_graphics::pixelcolor::Bgr555;
use embedded_graphics::pixelcolor::Bgr565;
use embedded_graphics::pixelcolor::Bgr666;
use embedded_graphics::pixelcolor::Bgr888;
use embedded_graphics::pixelcolor::PixelColor;
use embedded_graphics::pixelcolor::Rgb555;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::Rgb666;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::raw::RawU32;
use embedded_graphics::prelude::RawData;

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(PartialOrd, Ord)]
#[derive(Hash)]
#[derive(bytemuck::Zeroable, bytemuck::Pod)]
#[repr(transparent)]
pub struct Argb8888(pub u32);

impl Argb8888 {
    pub const fn new(alpha: u8, red: u8, green: u8, blue: u8) -> Self {
        Self(u32::from_be_bytes([alpha, red, green, blue]))
    }

    pub const fn from_argb([a, r, g, b]: [u8; 4]) -> Self {
        Self::new(a, r, g, b)
    }

    pub const fn from_u32(value: u32) -> Self {
        Self(value)
    }

    pub const fn into_u32(self) -> u32 {
        self.0
    }

    pub const fn blend(self, other: Self) -> Self {
        const fn blend_component(alpha_a: u8, comp_a: u8, alpha_b: u8, comp_b: u8) -> u8 {
            let a = comp_a as u32 * alpha_a as u32;
            let b = comp_b as u32 * alpha_b as u32 * (0xFF - alpha_a as u32) / 0xFF;

            ((a + b) / 0xFF) as u8
        }

        let [ax, rx, gx, bx] = self.argb();
        let [ay, ry, gy, by] = other.argb();

        Self::new(
            ax + (ay as u32 * (0xFF - ax as u32) / 0xFF) as u8,
            blend_component(ax, rx, ay, ry),
            blend_component(ax, gx, ay, gy),
            blend_component(ax, bx, ay, by),
        )
    }

    pub const fn alpha(self) -> u8 {
        let [alpha, _, _, _] = self.argb();
        alpha
    }

    pub const fn red(self) -> u8 {
        let [_, red, _, _] = self.argb();
        red
    }

    pub const fn green(self) -> u8 {
        let [_, _, green, _] = self.argb();
        green
    }

    pub const fn blue(self) -> u8 {
        let [_, _, _, blue] = self.argb();
        blue
    }

    pub const fn argb(self) -> [u8; 4] {
        self.into_u32().to_be_bytes()
    }
}

impl Display for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        LowerHex::fmt(self, f)
    }
}

impl LowerHex for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:08x}", self.into_u32())
    }
}

impl UpperHex for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:08X}", self.into_u32())
    }
}

impl PixelColor for Argb8888 {
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
                use embedded_graphics::prelude::RgbColor;
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

impl embedded_graphics::prelude::RgbColor for Argb8888 {
    fn r(&self) -> u8 {
        Self::red(*self)
    }

    fn g(&self) -> u8 {
        Self::green(*self)
    }

    fn b(&self) -> u8 {
        Self::blue(*self)
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

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(PartialOrd, Ord)]
#[derive(Hash)]
#[derive(bytemuck::Zeroable, bytemuck::Pod)]
#[repr(transparent)]
pub struct Al88(u16);

impl Al88 {
    pub const fn new(alpha: u8, luma: u8) -> Self {
        Self(u16::from_be_bytes([alpha, luma]))
    }

    pub const fn from_al([a, l]: [u8; 2]) -> Self {
        Self::new(a, l)
    }

    pub const fn from_u16(value: u16) -> Self {
        Self(value)
    }

    pub const fn into_u16(self) -> u16 {
        self.0
    }

    pub const fn blend_argb(self, other: Self) -> Self {
        const fn blend_component(alpha_a: u8, comp_a: u8, alpha_b: u8, comp_b: u8) -> u8 {
            let a = comp_a as u32 * alpha_a as u32;
            let b = comp_b as u32 * alpha_b as u32 * (0xFF - alpha_a as u32) / 0xFF;

            ((a + b) / 0xFF) as u8
        }
        Self::new(
            self.alpha()
                + (other.alpha() as u32 * (0xFF - self.alpha() as u32) / 0xFF) as u8,
            blend_component(self.alpha(), self.luma(), other.alpha(), other.luma()),
        )
    }

    pub const fn alpha(self) -> u8 {
        let [a, _] = self.al();
        a
    }

    pub const fn luma(self) -> u8 {
        let [_, l] = self.al();
        l
    }

    pub const fn al(self) -> [u8; 2] {
        self.into_u16().to_be_bytes()
    }
}

impl Display for Al88 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        LowerHex::fmt(self, f)
    }
}

impl LowerHex for Al88 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:04x}", self.into_u16())
    }
}

impl UpperHex for Al88 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:04X}", self.into_u16())
    }
}

impl PixelColor for Al88 {
    type Raw = RawU16;
}

impl From<u16> for Al88 {
    fn from(value: u16) -> Self {
        Self::from_u16(value)
    }
}

impl From<Al88> for u16 {
    fn from(al: Al88) -> Self {
        al.into_u16()
    }
}

impl From<RawU16> for Al88 {
    fn from(raw: RawU16) -> Self {
        Self::from(raw.into_inner())
    }
}

impl From<Al88> for RawU16 {
    fn from(argb: Al88) -> Self {
        RawU16::new(u16::from(argb))
    }
}

impl embedded_graphics::prelude::GrayColor for Al88 {
    fn luma(&self) -> u8 {
        Self::luma(*self)
    }

    const BLACK: Self = Self::new(u8::MAX, 0);

    const WHITE: Self = Self::new(u8::MAX, u8::MAX);
}

impl From<Al88> for Argb8888 {
    fn from(value: Al88) -> Self {
        let [a, l] = value.al();
        Self::new(a, l, l, l)
    }
}
