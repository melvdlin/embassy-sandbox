use core::future::poll_fn;
use core::mem;
use core::sync::atomic::Ordering;
use core::sync::atomic::fence;
use core::task::Poll;

use embassy_stm32::interrupt::typelevel as interrupt;
use embassy_stm32::interrupt::typelevel::Interrupt;
use embassy_stm32::pac;
use embassy_stm32::pac::dma2d::regs;
use embassy_stm32::pac::dma2d::vals;
use embassy_stm32::peripherals;
use embassy_stm32::rcc;
use embassy_sync::waitqueue::AtomicWaker;
use format::typelevel::Format;
use format::typelevel::Grayscale;
use format::typelevel::Rgb;

use crate::graphics::color::Argb8888;

pub type Peripheral = peripherals::DMA2D;
type PacDma2d = pac::dma2d::Dma2d;

const DMA2D: PacDma2d = pac::DMA2D;

static WAKER: AtomicWaker = AtomicWaker::new();
pub struct Dma2d {
    _peripheral: Peripheral,
}

bitflags::bitflags! {
    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[derive(Default)]
    struct Interrupts: u32 {
        const TX_ERROR = 1 << 0;
        const TX_COMPLETE = 1 << 1;
        const TX_WATERMARK = 1 << 2;
        const CLUT_ACCESS_ERROR = 1 << 3;
        const CLUT_TX_COMPLETE = 1 << 4;
        const CONFIG_ERROR = 1 << 5;
    }
}

pub mod format {
    use embassy_stm32::pac::dma2d::vals;

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[repr(u8)]
    pub enum Format {
        Rgb(Rgb),
        L8 = 5,
        AL44 = 6,
        AL88 = 7,
        L4 = 8,
        A8 = 9,
        A4 = 10,
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[derive(Default)]
    #[repr(u8)]
    pub enum Rgb {
        #[default]
        Argb8888 = 0,
        Rgb888 = 1,
        Rgb565 = 2,
        Argb1555 = 3,
        Argb4444 = 4,
    }

    impl Default for Format {
        fn default() -> Self {
            Self::Rgb(Rgb::default())
        }
    }

    impl From<Rgb> for Format {
        fn from(rgb: Rgb) -> Self {
            Format::Rgb(rgb)
        }
    }

    impl Format {
        pub const fn alignment(self) -> usize {
            match self {
                | Format::Rgb(rgb) => rgb.alignment(),
                | Format::L8 => 1,
                | Format::AL44 => 1,
                | Format::AL88 => 2,
                | Format::L4 => 1,
                | Format::A8 => 1,
                | Format::A4 => 1,
            }
        }

        pub const fn len_alignment(self) -> usize {
            match self {
                | Format::Rgb(rgb) => rgb.len_alignment(),
                | Format::L8 => 1,
                | Format::AL44 => 1,
                | Format::AL88 => 1,
                | Format::L4 => 2,
                | Format::A8 => 1,
                | Format::A4 => 2,
            }
        }
    }

    impl Rgb {
        pub const fn alignment(self) -> usize {
            match self {
                | Rgb::Argb8888 => 4,
                | Rgb::Rgb888 => 4,
                | Rgb::Rgb565 => 2,
                | Rgb::Argb1555 => 2,
                | Rgb::Argb4444 => 2,
            }
        }

        pub const fn len_alignment(self) -> usize {
            1
        }

        pub const fn size(self) -> usize {
            match self {
                | Rgb::Argb8888 => 4,
                | Rgb::Rgb888 => 3,
                | Rgb::Rgb565 => 2,
                | Rgb::Argb1555 => 2,
                | Rgb::Argb4444 => 2,
            }
        }
    }

    impl From<Rgb> for vals::OpfccrCm {
        fn from(format: Rgb) -> Self {
            match format {
                | Rgb::Argb8888 => Self::ARGB8888,
                | Rgb::Rgb888 => Self::RGB888,
                | Rgb::Rgb565 => Self::RGB565,
                | Rgb::Argb1555 => Self::ARGB1555,
                | Rgb::Argb4444 => Self::ARGB4444,
            }
        }
    }

    impl From<Format> for vals::FgpfccrCm {
        fn from(format: Format) -> Self {
            match format {
                | Format::Rgb(rgb) => Self::from(rgb),
                | Format::L8 => Self::L8,
                | Format::AL44 => Self::AL44,
                | Format::AL88 => Self::AL88,
                | Format::L4 => Self::L4,
                | Format::A8 => Self::A8,
                | Format::A4 => Self::A4,
            }
        }
    }

    impl From<Rgb> for vals::FgpfccrCm {
        fn from(format: Rgb) -> Self {
            match format {
                | Rgb::Argb8888 => Self::ARGB8888,
                | Rgb::Rgb888 => Self::RGB888,
                | Rgb::Rgb565 => Self::RGB565,
                | Rgb::Argb1555 => Self::ARGB1555,
                | Rgb::Argb4444 => Self::ARGB4444,
            }
        }
    }

    impl From<Format> for vals::BgpfccrCm {
        fn from(format: Format) -> Self {
            match format {
                | Format::Rgb(rgb) => Self::from(rgb),
                | Format::L8 => Self::L8,
                | Format::AL44 => Self::AL44,
                | Format::AL88 => Self::AL88,
                | Format::L4 => Self::L4,
                | Format::A8 => Self::A8,
                | Format::A4 => Self::A4,
            }
        }
    }

    impl From<Rgb> for vals::BgpfccrCm {
        fn from(format: Rgb) -> Self {
            match format {
                | Rgb::Argb8888 => Self::ARGB8888,
                | Rgb::Rgb888 => Self::RGB888,
                | Rgb::Rgb565 => Self::RGB565,
                | Rgb::Argb1555 => Self::ARGB1555,
                | Rgb::Argb4444 => Self::ARGB4444,
            }
        }
    }

    pub mod typelevel {

        pub trait Rgb: Format {
            const FORMAT: super::Rgb;
        }

        pub trait Format {
            type Repr: bytemuck::Pod;
            const FORMAT: super::Format;

            const LEN_ALIGN: usize = Self::FORMAT.len_alignment();
        }

        pub trait Grayscale: Format {}
        pub trait Alpha: Format {}

        macro_rules! rgb_color {
            ($id:ident, $repr:ty) => {
                #[derive(Debug)]
                #[derive(Clone, Copy)]
                #[derive(PartialEq, Eq, PartialOrd, Ord)]
                #[derive(Hash)]
                #[derive(Default)]
                pub struct $id;

                impl Rgb for $id {
                    const FORMAT: super::Rgb = super::Rgb::$id;
                }

                impl Format for $id {
                    type Repr = $repr;
                    const FORMAT: super::Format =
                        super::Format::Rgb(<Self as Rgb>::FORMAT);
                }
            };
        }

        macro_rules! color {
            ($id:ident, $repr:ty) => {
                #[derive(Debug)]
                #[derive(Clone, Copy)]
                #[derive(PartialEq, Eq, PartialOrd, Ord)]
                #[derive(Hash)]
                #[derive(Default)]
                pub struct $id;
                impl Format for $id {
                    type Repr = $repr;
                    const FORMAT: super::Format = super::Format::$id;
                }
            };
        }

        rgb_color!(Argb8888, u32);
        rgb_color!(Rgb888, u32);
        rgb_color!(Argb1555, u16);
        rgb_color!(Argb4444, u16);

        color!(L8, u8);
        color!(AL44, u8);
        color!(AL88, u16);
        color!(L4, u8);
        color!(A8, u8);
        color!(A4, u8);

        impl Grayscale for L8 {}
        impl Grayscale for AL44 {}
        impl Grayscale for AL88 {}
        impl Grayscale for L4 {}
        impl Grayscale for A8 {}
        impl Grayscale for A4 {}

        impl Alpha for Argb8888 {}
        impl Alpha for Argb1555 {}
        impl Alpha for Argb4444 {}
        impl Alpha for AL44 {}
        impl Alpha for AL88 {}
        impl Alpha for A8 {}
        impl Alpha for A4 {}

        pub trait Color {
            type Format: Format;
        }

        impl Color for crate::graphics::color::Argb8888 {
            type Format = Argb8888;
        }

        impl Color for crate::graphics::color::Al88 {
            type Format = AL88;
        }

        impl Color for embedded_graphics::pixelcolor::Gray8 {
            type Format = L8;
        }

        impl Color for embedded_graphics::pixelcolor::Gray4 {
            type Format = L4;
        }
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub struct InputConfig<'a, Format = format::typelevel::Argb8888>
where
    Format: format::typelevel::Format,
{
    pub source: &'a [Format::Repr],
    /// Offset after each line in pixels.
    /// Must be aligned to the [`len_alignment`](`format::Format::len_alignment`)
    /// of [`format`](`Self::format`).
    pub offset: u16,
    pub alpha: Option<AlphaConfig>,
    /// RGB used in A4 and A8 format
    pub color: Option<[u8; 3]>,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub struct OutputConfig {
    /// In pixels. Must be nonzero.
    pub width: u16,
    // In lines. Must be nonzero.
    pub height: u16,
    /// Offset after each line in pixels.
    /// Must be aligned to the [`len_alignment`](`format::Rgb::len_alignment`)
    /// of [`format`](`Self::format`).
    pub offset: u16,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub struct AlphaConfig {
    pub alpha: u8,
    pub mode: AlphaMode,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[repr(u8)]
pub enum AlphaMode {
    NoModification = 0,
    Replace = 1,
    Multiply = 2,
}

impl Dma2d {
    pub fn init(
        _peripheral: Peripheral,
        _irq: impl interrupt::Binding<interrupt::DMA2D, InterruptHandler>,
    ) -> Self {
        rcc::enable_and_reset::<Peripheral>();

        (Interrupts::TX_ERROR | Interrupts::TX_COMPLETE | Interrupts::CONFIG_ERROR)
            .enable();

        Self { _peripheral }
    }

    pub fn write_foreground_clut<I>(&mut self, clut: I)
    where
        I: IntoIterator<Item = Argb8888>,
    {
        self.write_clut(false, clut)
    }

    pub fn write_background_clut<I>(&mut self, clut: I)
    where
        I: IntoIterator<Item = Argb8888>,
    {
        self.write_clut(true, clut)
    }

    fn write_clut<I>(&mut self, background: bool, values: I)
    where
        I: IntoIterator<Item = Argb8888>,
    {
        let clut = if background {
            DMA2D.bgclut().as_ptr().cast::<u32>()
        } else {
            DMA2D.fgclut().as_ptr().cast::<u32>()
        };
        for (offset, value) in values.into_iter().take(0x100).enumerate() {
            // Safety: clut .. clut + 0x100 is outside of the AM
            unsafe {
                clut.add(offset).write_volatile(value.into_u32());
            }
        }
    }

    pub async fn transfer_merge<OF, FF, BF>(
        &mut self,
        dst: &mut [OF::Repr],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<'_, FF>,
        background: Option<&InputConfig<'_, BF>>,
    ) where
        OF: Rgb,
        FF: Format,
        BF: Format,
    {
        self.transfer_memory_cfg::<OF, _, _>(dst, out_cfg, foreground, background);

        fence(Ordering::SeqCst);
        self.run().await;
        fence(Ordering::SeqCst);
    }

    pub fn transfer_merge_blocking<OF, FF, BF>(
        &mut self,
        dst: &mut [OF::Repr],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<FF>,
        background: Option<&InputConfig<BF>>,
    ) where
        OF: Rgb,
        FF: Format,
        BF: Format,
    {
        self.transfer_memory_cfg::<OF, _, _>(dst, out_cfg, foreground, background);

        fence(Ordering::SeqCst);
        self.run_blocking();
        fence(Ordering::SeqCst);
    }

    pub async fn transfer_memory<OF, FF>(
        &mut self,
        dst: &mut [OF::Repr],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<'_, FF>,
    ) where
        OF: Rgb,
        FF: Format,
    {
        self.transfer_merge::<OF, _, format::typelevel::Argb8888>(
            dst, out_cfg, foreground, None,
        )
        .await;
    }

    pub async fn transfer_memory_blocking<OF, FF>(
        &mut self,
        dst: &mut [OF::Repr],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<'_, FF>,
    ) where
        OF: Rgb,
        FF: Format,
    {
        fence(Ordering::SeqCst);
        self.transfer_merge_blocking::<OF, _, format::typelevel::Argb8888>(
            dst, out_cfg, foreground, None,
        );
        fence(Ordering::SeqCst);
    }

    fn transfer_memory_cfg<OF, FF, BF>(
        &mut self,
        dst: &mut [OF::Repr],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<FF>,
        background: Option<&InputConfig<BF>>,
    ) where
        OF: Rgb,
        FF: Format,
        BF: Format,
    {
        self.output_cfg::<OF>(dst, out_cfg);
        self.input_cfg(foreground, out_cfg, false);
        if let Some(background) = background {
            self.input_cfg(background, out_cfg, true);
        }

        DMA2D.cr().modify(|w| {
            w.set_mode(if background.is_none() {
                vals::Mode::MEMORY_TO_MEMORY_PFC
            } else {
                vals::Mode::MEMORY_TO_MEMORY_PFCBLENDING
            })
        });
    }

    pub async fn fill<Format>(
        &mut self,
        dst: &mut [Format::Repr],
        out_cfg: &OutputConfig,
        color: Argb8888,
    ) where
        Format: Rgb,
    {
        self.fill_cfg::<Format>(dst, out_cfg, color);
        self.run().await
    }

    pub fn fill_blocking<Format>(
        &mut self,
        dst: &mut [Format::Repr],
        out_cfg: &OutputConfig,
        color: Argb8888,
    ) where
        Format: Rgb,
    {
        self.fill_cfg::<Format>(dst, out_cfg, color);
        self.run_blocking()
    }

    fn fill_cfg<Format>(
        &mut self,
        dst: &mut [Format::Repr],
        out_cfg: &OutputConfig,
        color: Argb8888,
    ) where
        Format: Rgb,
    {
        self.output_cfg::<Format>(dst, out_cfg);
        let color = regs::Ocolr(match <Format as Rgb>::FORMAT {
            | format::Rgb::Argb8888 | format::Rgb::Rgb888 => color.into_u32().to_le(),
            | format::Rgb::Rgb565 => {
                let red = color.red() >> 3;
                let green = color.green() >> 2;
                let blue = color.blue() >> 3;
                let mut color = red as u32;
                color <<= 6;
                color |= green as u32;
                color <<= 5;
                color |= blue as u32;
                color
            }
            | format::Rgb::Argb1555 => {
                let alpha = color.alpha() >> 7;
                let red = color.red() >> 3;
                let green = color.green() >> 3;
                let blue = color.blue() >> 3;
                let mut color = alpha as u32;
                color <<= 1;
                color |= red as u32;
                color <<= 5;
                color |= green as u32;
                color <<= 5;
                color |= blue as u32;
                color
            }
            | format::Rgb::Argb4444 => {
                let alpha = color.alpha();
                let red = color.red() >> 4;
                let green = color.green();
                let blue = color.blue() >> 4;

                u32::from_le_bytes([0, 0, alpha | red, green | blue])
            }
        });
        DMA2D.ocolr().write_value(color);
        DMA2D.cr().modify(|w| w.set_mode(vals::Mode::REGISTER_TO_MEMORY));
    }

    fn input_cfg<Format>(
        &mut self,
        in_cfg: &InputConfig<Format>,
        out_cfg: &OutputConfig,
        background: bool,
    ) where
        Format: format::typelevel::Format,
    {
        assert_eq!(
            in_cfg.source.len(),
            out_cfg.width as usize * out_cfg.height as usize
        );
        assert!(in_cfg.offset.is_multiple_of(Format::FORMAT.len_alignment() as u16));
        assert!(out_cfg.width.is_multiple_of(Format::FORMAT.len_alignment() as u16));

        if background {
            DMA2D.bgpfccr().write(|w| {
                w.set_cm(Format::FORMAT.into());
                if let Some(AlphaConfig { alpha, mode }) = in_cfg.alpha {
                    w.set_am(mode.into());
                    w.set_alpha(alpha);
                }
            });

            DMA2D.bgmar().write(|w| {
                w.set_ma(
                    in_cfg
                        .source
                        .as_ptr()
                        .addr()
                        .try_into()
                        .expect("pointer must fit into u32"),
                )
            });

            DMA2D.bgor().write(|w| {
                w.set_lo(in_cfg.offset);
            });

            if let Some([red, green, blue]) = in_cfg.color {
                DMA2D.bgcolr().write(|w| {
                    w.set_red(red);
                    w.set_green(green);
                    w.set_blue(blue);
                })
            }
        } else {
            DMA2D.fgpfccr().write(|w| {
                w.set_cm(Format::FORMAT.into());
                if let Some(AlphaConfig { alpha, mode }) = in_cfg.alpha {
                    w.set_am(mode.into());
                    w.set_alpha(alpha);
                }
            });

            DMA2D.fgmar().write(|w| {
                w.set_ma(
                    in_cfg
                        .source
                        .as_ptr()
                        .addr()
                        .try_into()
                        .expect("pointer must fit into u32"),
                )
            });

            DMA2D.fgor().write(|w| {
                w.set_lo(in_cfg.offset);
            });

            if let Some([red, green, blue]) = in_cfg.color {
                DMA2D.fgcolr().write(|w| {
                    w.set_red(red);
                    w.set_green(green);
                    w.set_blue(blue);
                })
            }
        }
    }

    fn output_cfg<Format>(&mut self, dst: &mut [Format::Repr], cfg: &OutputConfig)
    where
        Format: Rgb,
    {
        assert!(cfg.height != 0);
        assert!(cfg.width != 0);
        assert!(cfg.width <= u16::MAX >> 2);
        let width = cfg.width as usize;
        let height = cfg.height as usize;
        let offset = cfg.offset as usize;
        let total_size = width + offset * (height - 1) + width;
        assert!(total_size <= dst.len());

        DMA2D.opfccr().write(|w| {
            w.set_cm(vals::OpfccrCm::from(<Format as Rgb>::FORMAT));
        });

        DMA2D.omar().write(|w| {
            w.set_ma(
                dst.as_mut_ptr().addr().try_into().expect("pointer must fit into u32"),
            )
        });

        DMA2D.oor().write(|w| {
            w.set_lo(cfg.offset);
        });

        DMA2D.nlr().write(|w| {
            w.set_nl(cfg.height);
            w.set_pl(cfg.width);
        });
    }

    async fn run(&mut self) {
        let mut polled = false;
        poll_fn(|cx| {
            if !mem::replace(&mut polled, true) {
                cortex_m::interrupt::free(|_cs| {
                    WAKER.register(cx.waker());
                    Interrupts::clear_pending();
                    Interrupts::enable_vector();

                    DMA2D.cr().modify(|w| w.set_start(vals::CrStart::START));

                    Poll::Pending
                })
            } else {
                Poll::Ready(())
            }
        })
        .await;

        let flags = Interrupts::read();
        flags.clear();

        assert!(!flags.intersects(Interrupts::CONFIG_ERROR | Interrupts::TX_ERROR));
        assert_eq!(flags, Interrupts::TX_COMPLETE);
    }

    fn run_blocking(&mut self) {
        loop {
            let flags = Interrupts::read();
            flags.clear();
            assert!(!flags.intersects(Interrupts::CONFIG_ERROR | Interrupts::TX_ERROR));

            if flags.contains(Interrupts::TX_COMPLETE) {
                break;
            }

            cortex_m::asm::wfe();
        }
    }
}

impl Interrupts {
    #[inline]
    pub fn read() -> Self {
        let flags = DMA2D.isr().read();
        Self::from_bits_truncate(flags.0)
    }

    #[inline]
    pub fn clear(self) {
        DMA2D.ifcr().write_value(regs::Ifcr(self.bits()));
    }

    #[inline]
    pub fn enable(self) {
        DMA2D.cr().modify(|w| {
            w.0 &= !Self::all().bits() << 8;
            w.0 |= self.bits() << 8;
        });
    }

    #[inline]
    pub fn clear_pending() {
        interrupt::DMA2D::unpend();
    }

    #[inline]
    pub fn enable_vector() {
        // Safety: critical section is priority based, not mask based
        unsafe {
            interrupt::DMA2D::enable();
        }
    }

    #[inline]
    pub fn disable_vector() {
        interrupt::DMA2D::disable();
    }
}

pub struct InterruptHandler {}

impl interrupt::Handler<interrupt::DMA2D> for InterruptHandler {
    unsafe fn on_interrupt() {
        Interrupts::disable_vector();
        WAKER.wake();
    }
}

impl From<AlphaMode> for vals::FgpfccrAm {
    fn from(mode: AlphaMode) -> Self {
        match mode {
            | AlphaMode::NoModification => Self::NO_MODIFY,
            | AlphaMode::Replace => Self::REPLACE,
            | AlphaMode::Multiply => Self::MULTIPLY,
        }
    }
}

impl From<AlphaMode> for vals::BgpfccrAm {
    fn from(mode: AlphaMode) -> Self {
        match mode {
            | AlphaMode::NoModification => Self::NO_MODIFY,
            | AlphaMode::Replace => Self::REPLACE,
            | AlphaMode::Multiply => Self::MULTIPLY,
        }
    }
}

impl<'a, F> InputConfig<'a, F>
where
    F: Format,
{
    pub const fn copy(source: &'a [F::Repr], offset: u16) -> Self {
        Self {
            source,
            offset,
            alpha: None,
            color: None,
        }
    }

    pub const fn blend_alpha(self, alpha: u8) -> Self {
        Self {
            alpha: Some(AlphaConfig {
                alpha,
                mode: AlphaMode::Multiply,
            }),
            ..self
        }
    }

    pub const fn set_alpha(self, alpha: u8) -> Self {
        Self {
            alpha: Some(AlphaConfig {
                alpha,
                mode: AlphaMode::Replace,
            }),
            ..self
        }
    }
}

impl<F> InputConfig<'_, F>
where
    F: Grayscale,
{
    pub const fn blend_color(self, color: Argb8888) -> Self {
        Self {
            alpha: Some(AlphaConfig {
                alpha: color.alpha(),
                mode: AlphaMode::Multiply,
            }),
            color: Some([color.red(), color.green(), color.blue()]),
            ..self
        }
    }
}

impl<'a> InputConfig<'a, format::typelevel::Argb8888> {
    pub const fn argb(
        source: &'a [<format::typelevel::Argb8888 as format::typelevel::Format>::Repr],
        offset: u16,
    ) -> Self {
        Self {
            source,
            offset,
            alpha: None,
            color: None,
        }
    }
}

impl<'a> InputConfig<'a, format::typelevel::Rgb888> {
    pub const fn rgb(
        source: &'a [<format::typelevel::Rgb888 as format::typelevel::Format>::Repr],
        offset: u16,
    ) -> Self {
        Self {
            source,
            offset,
            alpha: None,
            color: None,
        }
    }
}

impl OutputConfig {
    pub const fn new(width: u16, height: u16, offset: u16) -> Self {
        Self {
            width,
            height,
            offset,
        }
    }
}
