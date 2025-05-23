use core::fmt::Debug;
use core::future::poll_fn;
use core::hash::Hash;
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
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::RgbColor;
use format::typelevel::Rgb;
use gui_widgets::color::AlphaColor;
use gui_widgets::color::Argb8888;
use gui_widgets::color::Storage;

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

pub use format::typelevel::Format;

use crate::util::drop_guard::DropGuard;

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
        use embedded_graphics::pixelcolor::Gray4;
        use embedded_graphics::pixelcolor::Gray8;
        use embedded_graphics::pixelcolor::Rgb888;
        use embedded_graphics::prelude::PixelColor;
        use embedded_graphics::prelude::RawData;
        use embedded_graphics::prelude::RgbColor;
        use gui_widgets::color::A4;
        use gui_widgets::color::A8;
        use gui_widgets::color::Al44;
        use gui_widgets::color::Al88;
        use gui_widgets::color::Argb1555;
        use gui_widgets::color::Argb4444;
        use gui_widgets::color::Argb8888;

        pub trait Rgb: Format + RgbColor {
            const FORMAT: super::Rgb;
        }

        const fn clamp_min(n: usize, min: usize) -> usize {
            if n < min { min } else { n }
        }
        pub trait Format: PixelColor {
            const FORMAT: super::Format;
            const SIZE: usize = clamp_min(<Self::Raw as RawData>::BITS_PER_PIXEL / 8, 1);
            const LEN_ALIGN: usize =
                clamp_min(8 / <Self::Raw as RawData>::BITS_PER_PIXEL, 1);
            const ALIGN: usize = core::mem::align_of::<Self::Raw>();
        }

        macro_rules! rgb_color {
            ($ty:ident) => {
                impl Format for $ty {
                    const FORMAT: super::Format =
                        super::Format::Rgb(<Self as Rgb>::FORMAT);
                }

                impl Rgb for $ty {
                    const FORMAT: super::Rgb = super::Rgb::$ty;
                }
            };
        }

        macro_rules! color {
            ($ty:ty, $format:ident) => {
                impl Format for $ty {
                    const FORMAT: super::Format = super::Format::$format;
                }
            };
        }

        // TODO: move formats out of DMA module
        rgb_color!(Argb8888);
        rgb_color!(Rgb888);
        rgb_color!(Argb1555);
        rgb_color!(Argb4444);

        color!(Gray8, L8);
        color!(Gray4, L4);
        color!(Al88, AL88);
        color!(Al44, AL44);
        color!(A8, A8);
        color!(A4, A4);
    }
}

pub struct InputConfig<'a, Format = Argb8888>
where
    Format: format::typelevel::Format,
{
    pub source: &'a [Storage<Format>],
    /// Offset after each line in pixels.
    /// Must be aligned to the [`LEN_ALIGN`](`format::typelevel::Format::LEN_ALIGN`)
    /// of [`format`](`Self::format`).
    pub offset: u16,
    pub alpha: Option<AlphaConfig>,
    /// RGB used in A4 and A8 format
    pub color: Option<[u8; 3]>,
}

impl<F> Debug for InputConfig<'_, F>
where
    F: format::typelevel::Format,
    Storage<F>: Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InputConfig")
            .field("source", &self.source)
            .field("offset", &self.offset)
            .field("alpha", &self.alpha)
            .field("color", &self.color)
            .finish()
    }
}

impl<F> Clone for InputConfig<'_, F>
where
    F: format::typelevel::Format,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<F> Copy for InputConfig<'_, F> where F: format::typelevel::Format {}

impl<F> PartialEq for InputConfig<'_, F>
where
    F: format::typelevel::Format,
    Storage<F>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.offset == other.offset
            && self.alpha == other.alpha
            && self.color == other.color
    }
}

impl<F> Eq for InputConfig<'_, F>
where
    F: format::typelevel::Format,
    Storage<F>: PartialEq,
{
}
impl<F> Hash for InputConfig<'_, F>
where
    F: format::typelevel::Format,
    Storage<F>: Hash,
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.offset.hash(state);
        self.alpha.hash(state);
        self.color.hash(state);
    }
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
                clut.add(offset).write_volatile(value.into_storage());
            }
        }
    }

    pub async fn transfer_merge<OF, FF, BF>(
        &mut self,
        dst: &mut [Storage<OF>],
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
        dst: &mut [Storage<OF>],
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
        dst: &mut [Storage<OF>],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<'_, FF>,
    ) where
        OF: Rgb,
        FF: Format,
    {
        self.transfer_merge::<OF, _, Argb8888>(dst, out_cfg, foreground, None).await;
    }

    pub fn transfer_memory_blocking<OF, FF>(
        &mut self,
        dst: &mut [Storage<OF>],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<'_, FF>,
    ) where
        OF: Rgb,
        FF: Format,
    {
        fence(Ordering::SeqCst);
        self.transfer_merge_blocking::<OF, _, Argb8888>(dst, out_cfg, foreground, None);
        fence(Ordering::SeqCst);
    }

    fn transfer_memory_cfg<OF, FF, BF>(
        &mut self,
        dst: &mut [Storage<OF>],
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

    pub async fn transfer_onto<OF, IF>(
        &mut self,
        dst: &mut [Storage<OF>],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<'_, IF>,
        background: Option<AlphaConfig>,
    ) where
        OF: Rgb,
        IF: Format,
    {
        self.transfer_onto_cfg::<OF, IF>(dst, out_cfg, foreground, background);

        fence(Ordering::SeqCst);
        self.run().await;
        fence(Ordering::SeqCst);
    }

    pub fn transfer_onto_blocking<OF, IF>(
        &mut self,
        dst: &mut [Storage<OF>],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<'_, IF>,
        background: Option<AlphaConfig>,
    ) where
        OF: Rgb,
        IF: Format,
    {
        self.transfer_onto_cfg::<OF, IF>(dst, out_cfg, foreground, background);

        fence(Ordering::SeqCst);
        self.run_blocking();
        fence(Ordering::SeqCst);
    }

    fn transfer_onto_cfg<OF, IF>(
        &mut self,
        dst: &mut [Storage<OF>],
        out_cfg: &OutputConfig,
        foreground: &InputConfig<IF>,
        background: Option<AlphaConfig>,
    ) where
        OF: Rgb,
        IF: Format,
    {
        self.output_cfg::<OF>(dst, out_cfg);
        self.input_cfg(foreground, out_cfg, false);
        let bg_cfg = InputConfig::<OF> {
            source: dst,
            offset: out_cfg.offset,
            alpha: background,
            color: None,
        };
        self.input_cfg(&bg_cfg, out_cfg, true);

        DMA2D.cr().modify(|w| w.set_mode(vals::Mode::MEMORY_TO_MEMORY_PFCBLENDING));
    }

    pub async fn fill<Format>(
        &mut self,
        dst: &mut [Storage<Format>],
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
        dst: &mut [Storage<Format>],
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
        dst: &mut [Storage<Format>],
        out_cfg: &OutputConfig,
        color: Argb8888,
    ) where
        Format: Rgb,
    {
        self.output_cfg::<Format>(dst, out_cfg);

        let white = <Format as RgbColor>::WHITE;
        let r_truncate = white.r().leading_zeros();
        let g_truncate = white.g().leading_zeros();
        let b_truncate = white.b().leading_zeros();
        let a_truncate = match <Format as Rgb>::FORMAT {
            | format::Rgb::Argb8888 => 0,
            | format::Rgb::Argb4444 => 4,
            | format::Rgb::Argb1555 => 7,
            | _ => 8,
        };
        let a = color.a().wrapping_shl(a_truncate) as u32;
        let r = color.r().wrapping_shl(r_truncate) as u32;
        let g = color.g().wrapping_shl(g_truncate) as u32;
        let b = color.b().wrapping_shl(b_truncate) as u32;

        let b_shift = 0;
        let g_shift = b_shift + (8 - b_truncate);
        let r_shift = g_shift + (8 - g_truncate);
        let a_shift = r_shift + (8 - r_truncate);
        let a_fill = a << a_shift;
        let r_fill = r << r_shift;
        let g_fill = g << g_shift;
        let b_fill = b << b_shift;
        let argb = a_fill | r_fill | g_fill | b_fill;

        DMA2D.ocolr().write_value(regs::Ocolr(argb));
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
        let offset = in_cfg.offset as usize;
        let width = out_cfg.width as usize;
        let height = out_cfg.height as usize;
        assert_eq!(in_cfg.source.len(), width * height + offset * (height - 1));
        assert!(offset.is_multiple_of(Format::LEN_ALIGN));
        assert!(width.is_multiple_of(Format::LEN_ALIGN));

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

    fn output_cfg<Format>(&mut self, dst: &mut [Storage<Format>], cfg: &OutputConfig)
    where
        Format: Rgb,
    {
        assert!(cfg.height != 0);
        assert!(cfg.width != 0);
        assert!(cfg.width <= u16::MAX >> 2);
        let width = cfg.width as usize;
        let height = cfg.height as usize;
        let offset = cfg.offset as usize;
        let total_size = (width + offset) * (height - 1) + width;
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
        let mut guard = None;
        poll_fn(|cx| {
            if !mem::replace(&mut polled, true) {
                cortex_m::interrupt::free(|_cs| {
                    WAKER.register(cx.waker());
                    Interrupts::clear_pending();
                    Interrupts::enable_vector();

                    guard = Some(DropGuard::new(|| {
                        DMA2D.cr().modify(|w| w.set_abort(vals::Abort::ABORT_REQUEST));
                    }));
                    DMA2D.cr().modify(|w| w.set_start(vals::CrStart::START));

                    Poll::Pending
                })
            } else {
                mem::forget(guard.take());
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
        Interrupts::clear_pending();
        Interrupts::enable_vector();
        DMA2D.cr().modify(|w| w.set_start(vals::CrStart::START));
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
    pub const fn copy(source: &'a [Storage<F>], offset: u16) -> Self {
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
    F: format::typelevel::Format + AlphaColor,
{
    pub const fn blend_color(self, color: Argb8888) -> Self {
        Self {
            alpha: Some(AlphaConfig {
                alpha: color.a(),
                mode: AlphaMode::Multiply,
            }),
            color: Some([color.r(), color.g(), color.b()]),
            ..self
        }
    }
}

impl<'a> InputConfig<'a, Argb8888> {
    pub const fn argb(source: &'a [Storage<Argb8888>], offset: u16) -> Self {
        Self {
            source,
            offset,
            alpha: None,
            color: None,
        }
    }
}

impl<'a> InputConfig<'a, Rgb888> {
    pub const fn rgb(source: &'a [Storage<Rgb888>], offset: u16) -> Self {
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
