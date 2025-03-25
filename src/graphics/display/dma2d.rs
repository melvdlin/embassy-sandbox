use core::future::poll_fn;
use core::mem;
use core::mem::MaybeUninit;
use core::task::Poll;

use embassy_stm32::interrupt::typelevel as interrupt;
use embassy_stm32::interrupt::typelevel::Interrupt;
use embassy_stm32::pac;
use embassy_stm32::pac::dma2d::regs;
use embassy_stm32::pac::dma2d::vals;
use embassy_stm32::peripherals;
use embassy_stm32::rcc;
use embassy_sync::waitqueue::AtomicWaker;
use embedded_graphics::prelude::RgbColor;

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

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[derive(Default)]
#[repr(u8)]
pub enum InputFormat {
    #[default]
    Argb8888 = 0,
    Rgb888 = 1,
    Rgb565 = 2,
    Argb1555 = 3,
    Argb4444 = 4,
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
pub enum OutputFormat {
    #[default]
    Argb8888 = 0,
    Rgb888 = 1,
    Rgb565 = 2,
    Argb1555 = 3,
    Argb4444 = 4,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub struct InputConfig {
    format: InputFormat,
    source: *const (),
    /// Offset after each line in pixels.
    /// Must be aligned to the [`len_alignment`](`InputFormat::len_alignment`)
    /// of [`format`](`Self::format`).
    offset: u16,
    alpha: Option<AlphaConfig>,
    /// RGB used in A4 and A8 format
    color: Option<[u8; 3]>,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
pub struct OutputConfig {
    format: OutputFormat,
    /// In pixels. Must be nonzero.
    width: u16,
    // In lines. Must be nonzero.
    height: u16,
    /// Offset after each line in pixels.
    /// Must be aligned to the [`len_alignment`](`InputFormat::len_alignment`)
    /// of [`format`](`Self::format`).
    offset: u16,
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

    pub async fn transfer_memory(
        &mut self,
        dst: &mut [MaybeUninit<u8>],
        out_cfg: &OutputConfig,
        foreground: &InputConfig,
        background: Option<&InputConfig>,
    ) {
        self.output_cfg(dst, out_cfg);
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
        self.run().await;
    }

    pub async fn fill(
        &mut self,
        dst: &mut [MaybeUninit<u8>],
        out_cfg: &OutputConfig,
        color: Argb8888,
    ) {
        self.output_cfg(dst, out_cfg);
        let color = regs::Ocolr(match out_cfg.format {
            | OutputFormat::Argb8888 | OutputFormat::Rgb888 => color.into_u32().to_le(),
            | OutputFormat::Rgb565 => {
                let red = color.r() >> 3;
                let green = color.g() >> 2;
                let blue = color.b() >> 3;
                let mut color = red as u32;
                color <<= 6;
                color |= green as u32;
                color <<= 5;
                color |= blue as u32;
                color
            }
            | OutputFormat::Argb1555 => {
                let alpha = color.a() >> 7;
                let red = color.r() >> 3;
                let green = color.g() >> 3;
                let blue = color.b() >> 3;
                let mut color = alpha as u32;
                color <<= 1;
                color |= red as u32;
                color <<= 5;
                color |= green as u32;
                color <<= 5;
                color |= blue as u32;
                color
            }
            | OutputFormat::Argb4444 => {
                let alpha = color.a();
                let red = color.r() >> 4;
                let green = color.g();
                let blue = color.b() >> 4;

                u32::from_le_bytes([0, 0, alpha | red, green | blue])
            }
        });
        DMA2D.ocolr().write_value(color);
        DMA2D.cr().modify(|w| w.set_mode(vals::Mode::REGISTER_TO_MEMORY));
        self.run().await;
    }

    fn input_cfg(
        &mut self,
        in_cfg: &InputConfig,
        out_cfg: &OutputConfig,
        background: bool,
    ) {
        assert!(in_cfg.source.is_aligned_to(in_cfg.format.alignment()));
        assert!(in_cfg.offset.is_multiple_of(in_cfg.format.len_alignment() as u16));
        assert!(out_cfg.width.is_multiple_of(in_cfg.format.len_alignment() as u16));

        if background {
            DMA2D.bgpfccr().write(|w| {
                w.set_cm(in_cfg.format.into());
                if let Some(AlphaConfig { alpha, mode }) = in_cfg.alpha {
                    w.set_am(mode.into());
                    w.set_alpha(alpha);
                }
            });

            DMA2D.bgmar().write(|w| {
                w.set_ma(
                    in_cfg.source.addr().try_into().expect("pointer must fit into u32"),
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
                w.set_cm(in_cfg.format.into());
                if let Some(AlphaConfig { alpha, mode }) = in_cfg.alpha {
                    w.set_am(mode.into());
                    w.set_alpha(alpha);
                }
            });

            DMA2D.fgmar().write(|w| {
                w.set_ma(
                    in_cfg.source.addr().try_into().expect("pointer must fit into u32"),
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

    fn output_cfg(&mut self, dst: &mut [MaybeUninit<u8>], cfg: &OutputConfig) {
        assert!(dst.as_ptr().is_aligned_to(cfg.format.alignment()));
        assert!(cfg.height != 0);
        assert!(cfg.width != 0);
        assert!(cfg.width <= u16::MAX >> 2);
        let width = cfg.width as usize;
        let height = cfg.height as usize;
        let offset = cfg.offset as usize;
        let total_size = (width + offset * (height - 1) + width) * cfg.format.size();
        assert!(total_size <= dst.len());

        DMA2D.opfccr().write(|w| {
            w.set_cm(vals::OpfccrCm::from(cfg.format));
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
                    Interrupts::enable_vector();

                    DMA2D.cr().modify(|w| w.set_start(vals::CrStart::START));

                    Poll::Pending
                })
            } else {
                Poll::Ready(())
            }
        })
        .await;
        Interrupts::disable_vector();

        let flags = Interrupts::read();
        flags.clear();

        assert!(!flags.contains(Interrupts::CONFIG_ERROR | Interrupts::TX_ERROR));
        assert_eq!(flags, Interrupts::TX_COMPLETE);
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
            w.0 &= Self::all().bits() << 8;
            w.0 |= self.bits() << 8;
        });
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
        WAKER.wake();
    }
}

impl InputFormat {
    pub const fn alignment(self) -> usize {
        match self {
            | InputFormat::Argb8888 => 4,
            | InputFormat::Rgb888 => 4,
            | InputFormat::Rgb565 => 2,
            | InputFormat::Argb1555 => 2,
            | InputFormat::Argb4444 => 2,
            | InputFormat::L8 => 1,
            | InputFormat::AL44 => 1,
            | InputFormat::AL88 => 2,
            | InputFormat::L4 => 1,
            | InputFormat::A8 => 1,
            | InputFormat::A4 => 1,
        }
    }

    pub const fn len_alignment(self) -> usize {
        match self {
            | InputFormat::Argb8888 => 1,
            | InputFormat::Rgb888 => 1,
            | InputFormat::Rgb565 => 1,
            | InputFormat::Argb1555 => 1,
            | InputFormat::Argb4444 => 1,
            | InputFormat::L8 => 1,
            | InputFormat::AL44 => 1,
            | InputFormat::AL88 => 1,
            | InputFormat::L4 => 2,
            | InputFormat::A8 => 1,
            | InputFormat::A4 => 2,
        }
    }
}

impl OutputFormat {
    pub const fn alignment(self) -> usize {
        match self {
            | OutputFormat::Argb8888 => 4,
            | OutputFormat::Rgb888 => 4,
            | OutputFormat::Rgb565 => 2,
            | OutputFormat::Argb1555 => 2,
            | OutputFormat::Argb4444 => 2,
        }
    }

    pub const fn size(self) -> usize {
        match self {
            | OutputFormat::Argb8888 => 4,
            | OutputFormat::Rgb888 => 3,
            | OutputFormat::Rgb565 => 2,
            | OutputFormat::Argb1555 => 2,
            | OutputFormat::Argb4444 => 2,
        }
    }
}

impl From<OutputFormat> for vals::OpfccrCm {
    fn from(format: OutputFormat) -> Self {
        match format {
            | OutputFormat::Argb8888 => Self::ARGB8888,
            | OutputFormat::Rgb888 => Self::RGB888,
            | OutputFormat::Rgb565 => Self::RGB565,
            | OutputFormat::Argb1555 => Self::ARGB1555,
            | OutputFormat::Argb4444 => Self::ARGB4444,
        }
    }
}

impl From<InputFormat> for vals::FgpfccrCm {
    fn from(format: InputFormat) -> Self {
        match format {
            | InputFormat::Argb8888 => Self::ARGB8888,
            | InputFormat::Rgb888 => Self::RGB888,
            | InputFormat::Rgb565 => Self::RGB565,
            | InputFormat::Argb1555 => Self::ARGB1555,
            | InputFormat::Argb4444 => Self::ARGB4444,
            | InputFormat::L8 => Self::L8,
            | InputFormat::AL44 => Self::AL44,
            | InputFormat::AL88 => Self::AL88,
            | InputFormat::L4 => Self::L4,
            | InputFormat::A8 => Self::A8,
            | InputFormat::A4 => Self::A4,
        }
    }
}

impl From<InputFormat> for vals::BgpfccrCm {
    fn from(format: InputFormat) -> Self {
        match format {
            | InputFormat::Argb8888 => Self::ARGB8888,
            | InputFormat::Rgb888 => Self::RGB888,
            | InputFormat::Rgb565 => Self::RGB565,
            | InputFormat::Argb1555 => Self::ARGB1555,
            | InputFormat::Argb4444 => Self::ARGB4444,
            | InputFormat::L8 => Self::L8,
            | InputFormat::AL44 => Self::AL44,
            | InputFormat::AL88 => Self::AL88,
            | InputFormat::L4 => Self::L4,
            | InputFormat::A8 => Self::A8,
            | InputFormat::A4 => Self::A4,
        }
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

impl InputConfig {
    pub const fn argb(source: *const (), offset: u16) -> Self {
        Self {
            format: InputFormat::Argb8888,
            source,
            offset,
            alpha: None,
            color: None,
        }
    }

    pub const fn rgb(source: *const (), offset: u16) -> Self {
        Self {
            format: InputFormat::Argb8888,
            source,
            offset,
            alpha: None,
            color: None,
        }
    }
}

impl OutputConfig {
    pub const fn argb(width: u16, height: u16, offset: u16) -> Self {
        Self {
            format: OutputFormat::Argb8888,
            width,
            height,
            offset,
        }
    }

    pub const fn rgb(width: u16, height: u16, offset: u16) -> Self {
        Self {
            format: OutputFormat::Rgb888,
            width,
            height,
            offset,
        }
    }
}
