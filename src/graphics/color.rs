use core::fmt::Display;
use core::fmt::LowerHex;
use core::fmt::UpperHex;

use embedded_graphics::pixelcolor::Bgr555;
use embedded_graphics::pixelcolor::Bgr565;
use embedded_graphics::pixelcolor::Bgr666;
use embedded_graphics::pixelcolor::Bgr888;
use embedded_graphics::pixelcolor::Gray4;
use embedded_graphics::pixelcolor::Gray8;
use embedded_graphics::pixelcolor::GrayColor;
use embedded_graphics::pixelcolor::PixelColor;
use embedded_graphics::pixelcolor::Rgb555;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::Rgb666;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::pixelcolor::RgbColor;
use embedded_graphics::pixelcolor::raw::RawU4;
use embedded_graphics::pixelcolor::raw::RawU8;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::raw::RawU32;
use embedded_graphics::prelude::RawData;

pub type Storage<F> = <<F as PixelColor>::Raw as RawData>::Storage;

pub trait AlphaColor {
    const MAX_A: u8;
    fn a(&self) -> u8;
}

macro_rules! argb_color {
    ($ty:ident, $raw:ty, $storage:ty, $a:literal $r:literal $g:literal $b:literal) => {
        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(PartialOrd, Ord)]
        #[derive(Hash)]
        #[derive(bytemuck::Zeroable, bytemuck::Pod)]
        #[repr(transparent)]
        pub struct $ty(pub $storage);

        impl $ty {
            const A_SHIFT: $storage = Self::R_SHIFT + $r;
            const R_SHIFT: $storage = Self::G_SHIFT + $g;
            const G_SHIFT: $storage = Self::B_SHIFT + $b;
            const B_SHIFT: $storage = 0;

            const A_MASK: $storage = (Self::MAX_A as $storage) << Self::A_SHIFT;
            const R_MASK: $storage = (Self::MAX_R as $storage) << Self::R_SHIFT;
            const G_MASK: $storage = (Self::MAX_G as $storage) << Self::G_SHIFT;
            const B_MASK: $storage = (Self::MAX_B as $storage) << Self::B_SHIFT;
            const ARGB_MASK: $storage =
                Self::A_MASK | Self::R_MASK | Self::G_MASK | Self::B_MASK;

            pub const fn new(a: u8, r: u8, g: u8, b: u8) -> Self {
                Self(
                    (((a as $storage) << Self::A_SHIFT) & Self::A_MASK)
                        | (((r as $storage) << Self::R_SHIFT) & Self::R_MASK)
                        | (((g as $storage) << Self::G_SHIFT) & Self::G_MASK)
                        | (((b as $storage) << Self::B_SHIFT) & Self::B_MASK),
                )
            }

            pub const fn from_argb([a, r, g, b]: [u8; 4]) -> Self {
                Self::new(a, r, g, b)
            }

            pub const fn from_storage(value: $storage) -> Self {
                Self(value & Self::ARGB_MASK)
            }

            pub const fn into_storage(self) -> $storage {
                self.0
            }

            pub const fn a(self) -> u8 {
                ((self.0 & Self::A_MASK) >> Self::A_SHIFT) as u8
            }

            pub const fn r(self) -> u8 {
                ((self.0 & Self::R_MASK) >> Self::R_SHIFT) as u8
            }

            pub const fn g(self) -> u8 {
                ((self.0 & Self::G_MASK) >> Self::G_SHIFT) as u8
            }

            pub const fn b(self) -> u8 {
                ((self.0 & Self::B_MASK) >> Self::B_SHIFT) as u8
            }

            pub const fn argb(self) -> [u8; 4] {
                [self.a(), self.r(), self.g(), self.b()]
            }

            // a_c = a_a + a_b
            pub const fn blend(self, other: Self) -> Self {
                let [ax, rx, gx, bx] = self.argb();
                let [ay, ry, gy, by] = other.argb();
                let a = blend_alpha(ax, ay, Self::MAX_A);
                Self::new(
                    a,
                    blend_component(ax, rx, ay, ry, a, Self::MAX_A),
                    blend_component(ax, gx, ay, gy, a, Self::MAX_A),
                    blend_component(ax, bx, ay, by, a, Self::MAX_A),
                )
            }
        }

        impl AlphaColor for $ty {
            fn a(&self) -> u8 {
                Self::a(*self)
            }

            const MAX_A: u8 = !u8::MAX.wrapping_shl($a);
        }

        impl RgbColor for $ty {
            fn r(&self) -> u8 {
                Self::r(*self)
            }

            fn g(&self) -> u8 {
                Self::g(*self)
            }

            fn b(&self) -> u8 {
                Self::b(*self)
            }

            const MAX_R: u8 = !u8::MAX.wrapping_shl($r);

            const MAX_G: u8 = !u8::MAX.wrapping_shl($g);

            const MAX_B: u8 = !u8::MAX.wrapping_shl($b);

            const BLACK: Self = Self::new(Self::MAX_A, 0, 0, 0);

            const RED: Self = Self::new(Self::MAX_A, Self::MAX_R, 0, 0);

            const GREEN: Self = Self::new(Self::MAX_A, 0, Self::MAX_G, 0);

            const BLUE: Self = Self::new(Self::MAX_A, 0, 0, Self::MAX_B);

            const YELLOW: Self = Self::new(Self::MAX_A, Self::MAX_R, Self::MAX_G, 0);

            const MAGENTA: Self = Self::new(Self::MAX_A, Self::MAX_R, 0, Self::MAX_B);

            const CYAN: Self = Self::new(Self::MAX_A, 0, Self::MAX_G, Self::MAX_B);

            const WHITE: Self =
                Self::new(Self::MAX_A, Self::MAX_R, Self::MAX_G, Self::MAX_B);
        }

        impl PixelColor for $ty {
            type Raw = $raw;
        }

        impl From<$storage> for $ty {
            fn from(value: $storage) -> Self {
                Self::from_storage(value)
            }
        }

        impl From<$ty> for $storage {
            fn from(argb: $ty) -> Self {
                argb.into_storage()
            }
        }

        impl From<$raw> for $ty {
            fn from(raw: $raw) -> Self {
                Self::from(raw.into_inner())
            }
        }

        impl From<$ty> for $raw {
            fn from(argb: $ty) -> Self {
                <$raw>::new(<$storage>::from(argb))
            }
        }
    };
}

macro_rules! impl_from_rgb {
    ($ty:ty, $from:ty) => {
        impl From<$from> for $ty {
            fn from(rgb: $from) -> Self {
                let r_shift = <$from>::MAX_R.leading_zeros();
                let g_shift = <$from>::MAX_G.leading_zeros();
                let b_shift = <$from>::MAX_B.leading_zeros();
                Self::new(
                    Self::MAX_A,
                    rgb.r() << r_shift,
                    rgb.g() << g_shift,
                    rgb.b() << b_shift,
                )
            }
        }
    };
}

macro_rules! impl_from_argb {
    ($ty:ty, $from:ty) => {
        impl From<$from> for $ty {
            fn from(argb: $from) -> Self {
                let a_shift = <$from>::MAX_A.leading_zeros();
                let r_shift = <$from>::MAX_R.leading_zeros();
                let g_shift = <$from>::MAX_G.leading_zeros();
                let b_shift = <$from>::MAX_B.leading_zeros();
                Self::new(
                    argb.a() << a_shift,
                    argb.r() << r_shift,
                    argb.g() << g_shift,
                    argb.b() << b_shift,
                )
            }
        }
    };
}

argb_color!(Argb8888, RawU32, u32, 8 8 8 8);
impl_from_rgb!(Argb8888, Rgb888);
impl_from_rgb!(Argb8888, Rgb666);
impl_from_rgb!(Argb8888, Rgb555);
impl_from_rgb!(Argb8888, Rgb565);
impl_from_rgb!(Argb8888, Bgr888);
impl_from_rgb!(Argb8888, Bgr666);
impl_from_rgb!(Argb8888, Bgr555);
impl_from_rgb!(Argb8888, Bgr565);
impl_from_argb!(Argb8888, Argb4444);
impl_from_argb!(Argb8888, Argb1555);

impl Display for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        LowerHex::fmt(self, f)
    }
}

impl LowerHex for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:08x}", self.into_storage())
    }
}

impl UpperHex for Argb8888 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:08X}", self.into_storage())
    }
}

argb_color!(Argb1555, RawU16, u16, 1 5 5 5);
impl_from_rgb!(Argb1555, Rgb555);
impl_from_rgb!(Argb1555, Bgr555);

argb_color!(Argb4444, RawU16, u16, 4 4 4 4);

macro_rules! al_color {
    ($ty:ident, $raw:ty, $storage:ty, $a:literal $l:literal) => {
        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(PartialOrd, Ord)]
        #[derive(Hash)]
        #[derive(bytemuck::Zeroable, bytemuck::Pod)]
        #[repr(transparent)]
        pub struct $ty($storage);

        impl $ty {
            const A_SHIFT: $storage = Self::L_SHIFT + $l;
            const L_SHIFT: $storage = 0;

            const A_MASK: $storage = (Self::MAX_A as $storage) << Self::A_SHIFT;
            const L_MASK: $storage = (Self::MAX_L as $storage) << Self::L_SHIFT;
            const AL_MASK: $storage = Self::A_MASK | Self::L_MASK;

            const MAX_L: u8 = !(u8::MAX.wrapping_shl($l));

            pub const fn new(a: u8, l: u8) -> Self {
                Self(
                    ((a as $storage) << Self::A_SHIFT) & Self::A_MASK
                        | ((l as $storage) << Self::L_SHIFT) & Self::L_MASK,
                )
            }

            pub const fn from_al([a, l]: [u8; 2]) -> Self {
                Self::new(a, l)
            }

            pub const fn from_storage(value: $storage) -> Self {
                Self(value & Self::AL_MASK)
            }

            pub const fn into_storage(self) -> $storage {
                self.0
            }

            pub const fn blend_al(self, other: Self) -> Self {
                let a = blend_alpha(self.a(), other.a(), Self::MAX_A);
                Self::new(
                    a,
                    blend_component(
                        self.a(),
                        self.l(),
                        other.a(),
                        other.l(),
                        a,
                        Self::MAX_A,
                    ),
                )
            }

            pub const fn a(self) -> u8 {
                let [a, _] = self.al();
                a
            }

            pub const fn l(self) -> u8 {
                let [_, l] = self.al();
                l
            }

            pub const fn al(self) -> [u8; 2] {
                self.into_storage().to_be_bytes()
            }
        }

        impl GrayColor for $ty {
            fn luma(&self) -> u8 {
                Self::l(*self)
            }

            const BLACK: Self = Self::new(Self::MAX_A, 0);
            const WHITE: Self = Self::new(Self::MAX_A, Self::MAX_L);
        }

        impl AlphaColor for $ty {
            const MAX_A: u8 = !(u8::MAX.wrapping_shl($a));

            fn a(&self) -> u8 {
                Self::a(*self)
            }
        }

        impl PixelColor for $ty {
            type Raw = $raw;
        }

        impl From<$storage> for $ty {
            fn from(value: $storage) -> Self {
                Self::from_storage(value)
            }
        }

        impl From<$ty> for $storage {
            fn from(al: $ty) -> Self {
                al.into_storage()
            }
        }

        impl From<$raw> for $ty {
            fn from(raw: $raw) -> Self {
                Self::from(raw.into_inner())
            }
        }

        impl From<$ty> for $raw {
            fn from(argb: $ty) -> Self {
                <$raw>::new(<$storage>::from(argb))
            }
        }

        impl From<$ty> for Argb8888 {
            fn from(value: $ty) -> Self {
                let [a, l] = value.al();
                Self::new(a, l, l, l)
            }
        }
    };
}

al_color!(Al88, RawU16, u16, 8 8);
impl From<Al44> for Al88 {
    fn from(value: Al44) -> Self {
        let a_shift = Al44::MAX_A.leading_zeros();
        let l_shift = Al44::MAX_L.leading_zeros();
        let [a, l] = value.al();
        Self::new(a << a_shift, l << l_shift)
    }
}

impl From<Gray8> for Al88 {
    fn from(value: Gray8) -> Self {
        Self::new(Self::MAX_A, value.luma())
    }
}

impl From<Gray4> for Al88 {
    fn from(value: Gray4) -> Self {
        let l_shift = Al44::MAX_L.leading_zeros();
        Self::new(Self::MAX_A, value.luma() << l_shift)
    }
}

impl Display for Al88 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        LowerHex::fmt(self, f)
    }
}

impl LowerHex for Al88 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:04x}", self.into_storage())
    }
}

impl UpperHex for Al88 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "#{:04X}", self.into_storage())
    }
}

al_color!(Al44, RawU16, u16, 4 4);

impl From<Gray4> for Al44 {
    fn from(value: Gray4) -> Self {
        Self::new(Self::MAX_A, value.luma())
    }
}

const fn blend_alpha(alpha_a: u8, alpha_b: u8, max: u8) -> u8 {
    alpha_a + ((alpha_b as u32 * (max - alpha_a) as u32) / max as u32) as u8
}

macro_rules! a_color {
    ($ty:ident, $raw:ty, $storage:ty, $a:literal) => {
        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(PartialOrd, Ord)]
        #[derive(Hash)]
        #[derive(bytemuck::Zeroable, bytemuck::Pod)]
        #[repr(transparent)]
        pub struct $ty($storage);

        impl $ty {
            const A_MASK: $storage = (Self::MAX_A as $storage);

            pub const fn new(a: u8) -> Self {
                Self((a as $storage) & Self::A_MASK)
            }

            pub const fn from_storage(value: $storage) -> Self {
                Self(value)
            }

            pub const fn into_storage(self) -> $storage {
                self.0
            }

            pub const fn blend_a(self, other: Self) -> Self {
                let a = blend_alpha(self.a(), other.a(), Self::MAX_A);
                Self::new(a)
            }

            pub const fn a(self) -> u8 {
                self.0 as u8
            }
        }

        impl AlphaColor for $ty {
            const MAX_A: u8 = !(u8::MAX.wrapping_shl($a));

            fn a(&self) -> u8 {
                Self::a(*self)
            }
        }

        impl PixelColor for $ty {
            type Raw = $raw;
        }

        impl From<$storage> for $ty {
            fn from(value: $storage) -> Self {
                Self::from_storage(value)
            }
        }

        impl From<$ty> for $storage {
            fn from(al: $ty) -> Self {
                al.into_storage()
            }
        }

        impl From<$raw> for $ty {
            fn from(raw: $raw) -> Self {
                Self::from(raw.into_inner())
            }
        }

        impl From<$ty> for $raw {
            fn from(argb: $ty) -> Self {
                <$raw>::new(<$storage>::from(argb))
            }
        }
    };
}

a_color!(A8, RawU8, u8, 8);
a_color!(A4, RawU4, u8, 4);

impl From<A8> for Gray8 {
    fn from(value: A8) -> Self {
        Self::new(value.a())
    }
}

impl From<Gray8> for A8 {
    fn from(value: Gray8) -> Self {
        Self::new(value.luma())
    }
}

impl From<A4> for Gray4 {
    fn from(value: A4) -> Self {
        Self::new(value.a())
    }
}

impl From<Gray4> for A4 {
    fn from(value: Gray4) -> Self {
        Self::new(value.luma())
    }
}

const fn blend_component(
    alpha_a: u8,
    comp_a: u8,
    alpha_b: u8,
    comp_b: u8,
    final_alpha: u8,
    max_alpha: u8,
) -> u8 {
    let comp_a = comp_a as u32;
    let comp_b = comp_b as u32;
    let a = comp_a * alpha_a as u32;
    let b = comp_b * alpha_b as u32 * (max_alpha - alpha_a) as u32 / max_alpha as u32;

    ((a + b) / final_alpha as u32) as u8
}
