use core::future::poll_fn;
use core::mem;
use core::sync::atomic::Ordering;
use core::task::Poll;

use embassy_stm32::interrupt::typelevel as interrupt;
use embassy_stm32::interrupt::typelevel::Interrupt;
pub use embassy_stm32::ltdc::LtdcLayer as Layer;
use embassy_stm32::ltdc::RgbColor;
use embassy_stm32::pac;
use embassy_stm32::pac::ltdc::regs;
use embassy_stm32::pac::ltdc::vals;
use embassy_stm32::pac::ltdc::vals::Depol;
use embassy_stm32::pac::ltdc::vals::Hspol;
use embassy_stm32::pac::ltdc::vals::Pcpol;
use embassy_stm32::pac::ltdc::vals::Vspol;
use embassy_stm32::peripherals;
use embassy_sync::waitqueue::AtomicWaker;

use crate::graphics::color::Argb8888;

pub type Peripheral = peripherals::LTDC;
type PacLtdc = pac::ltdc::Ltdc;

const LTDC: PacLtdc = pac::LTDC;
static VSYNC: AtomicWaker = AtomicWaker::new();

pub struct Ltdc {
    _peripheral: Peripheral,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct LayerConfig {
    pub framebuffer: *const (),
    pub x_offset: u16,
    pub y_offset: u16,
    pub width: u16,
    pub height: u16,
    pub pixel_format: embassy_stm32::ltdc::PixelFormat,
    pub alpha: u8,
    pub default_color: Argb8888,
}

bitflags::bitflags! {
    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[derive(Default)]
    struct Interrupts: u32 {
        const LINE = 1 << 0;
        const FIFO_UNDERRUN = 1 << 1;
        const TX_ERROR = 1 << 2;
        const REGISTER_RELOAD = 1 << 3;
    }
}

impl Ltdc {
    pub fn init(
        ltdc: Peripheral,
        _irq: impl interrupt::Binding<interrupt::LTDC, InterruptHandler>
        + interrupt::Binding<interrupt::LTDC_ER, ErrorInterruptHandler>,
        background: RgbColor,
        cfg: &embassy_stm32::ltdc::LtdcConfiguration,
    ) -> Self {
        embassy_stm32::rcc::enable_and_reset::<peripherals::LTDC>();

        // configure HS, VS, DE and PC polarity
        LTDC.gcr().modify(|w| {
            w.set_hspol(match cfg.h_sync_polarity {
                | embassy_stm32::ltdc::PolarityActive::ActiveLow => Hspol::ACTIVE_LOW,
                | embassy_stm32::ltdc::PolarityActive::ActiveHigh => Hspol::ACTIVE_HIGH,
            });
            w.set_vspol(match cfg.v_sync_polarity {
                | embassy_stm32::ltdc::PolarityActive::ActiveLow => Vspol::ACTIVE_LOW,
                | embassy_stm32::ltdc::PolarityActive::ActiveHigh => Vspol::ACTIVE_HIGH,
            });
            w.set_depol(match cfg.data_enable_polarity {
                | embassy_stm32::ltdc::PolarityActive::ActiveLow => Depol::ACTIVE_LOW,
                | embassy_stm32::ltdc::PolarityActive::ActiveHigh => Depol::ACTIVE_HIGH,
            });
            w.set_pcpol(match cfg.pixel_clock_polarity {
                | embassy_stm32::ltdc::PolarityEdge::FallingEdge => Pcpol::FALLING_EDGE,
                | embassy_stm32::ltdc::PolarityEdge::RisingEdge => Pcpol::RISING_EDGE,
            });
        });

        // configure sync size
        let v_sync = cfg.v_sync;
        let h_sync = cfg.h_sync;
        LTDC.sscr().modify(|w| {
            w.set_vsh(v_sync - 1);
            w.set_hsw(h_sync - 1);
        });

        // configure accumulated back porch
        let acc_vbp = v_sync + cfg.v_back_porch;
        let acc_hbp = h_sync + cfg.h_back_porch;
        LTDC.bpcr().modify(|w| {
            w.set_avbp(acc_vbp - 1);
            w.set_ahbp(acc_hbp - 1);
        });

        // configure accumulated active width / height
        let acc_active_height = acc_vbp + cfg.active_height;
        let acc_active_width = acc_hbp + cfg.active_width;
        LTDC.awcr().modify(|w| {
            w.set_aah(acc_active_height - 1);
            w.set_aaw(acc_active_width - 1);
        });

        // configure total width / height
        let total_height = acc_active_height + cfg.v_front_porch;
        let total_width = acc_active_width + cfg.h_front_porch;
        LTDC.twcr().modify(|w| {
            w.set_totalh(total_height - 1);
            w.set_totalw(total_width - 1);
        });

        // configure background color
        LTDC.bccr().modify(|w| {
            w.set_bcred(background.red);
            w.set_bcgreen(background.green);
            w.set_bcblue(background.blue);
        });

        Interrupts::REGISTER_RELOAD.enable();

        // TODO: enable and handle error IRs

        // enable LTDC
        let mut ltdc = Ltdc { _peripheral: ltdc };

        ltdc.enable(true);

        ltdc
    }

    pub fn config_layer(&mut self, layer: Layer, cfg: &LayerConfig) {
        let h_win_start = cfg.x_offset + LTDC.bpcr().read().ahbp() + 1;
        let h_win_stop = h_win_start + cfg.width - 1;
        let v_win_start = cfg.y_offset + LTDC.bpcr().read().avbp() + 1;
        let v_win_stop = v_win_start + cfg.height - 1;

        {
            let layer = LTDC.layer(layer as usize);

            // horizontal and vertical window start and stop
            layer.whpcr().write(|w| {
                w.set_whstpos(h_win_start);
                w.set_whsppos(h_win_stop);
            });
            layer.wvpcr().write(|w| {
                w.set_wvstpos(v_win_start);
                w.set_wvsppos(v_win_stop);
            });

            // pixel format
            layer.pfcr().write(|w| w.set_pf(vals::Pf::from_bits(cfg.pixel_format as u8)));

            // default color
            layer.dccr().write(|w| {
                let [alpha, red, green, blue] = cfg.default_color.argb();
                w.set_dcalpha(alpha);
                w.set_dcred(red);
                w.set_dcgreen(green);
                w.set_dcblue(blue);
            });

            // alpha multiplier
            layer.cacr().write(|w| w.set_consta(cfg.alpha));

            // blending factors (color alpha x alpha multiplier)
            layer.bfcr().write(|w| {
                w.set_bf1(vals::Bf1::PIXEL);
                w.set_bf2(vals::Bf2::PIXEL);
            });

            // framebuffer start address
            layer.cfbar().write(|w| w.set_cfbadd(cfg.framebuffer as u32));

            // frame buffer line length and pitch (offset between start of subsequent lines)
            let pixel_size = cfg.pixel_format.bytes_per_pixel() as u16;
            layer.cfblr().write(|w| {
                w.set_cfbll(cfg.width * pixel_size + 3);
                w.set_cfbp((cfg.width) * pixel_size);
            });

            // frame buffer line count
            layer.cfblnr().write(|w| {
                w.set_cfblnbr(cfg.height);
            });
        }
    }

    pub fn enable(&mut self, enable: bool) {
        LTDC.gcr().modify(|w| w.set_ltdcen(enable));
    }

    pub fn enable_layer(&mut self, layer: Layer, enable: bool) {
        LTDC.layer(layer as usize).cr().modify(|w| w.set_len(enable));
        LTDC.srcr().write(|w| w.set_imr(vals::Imr::RELOAD));
    }

    pub fn set_framebuffer(&mut self, buffer: *const (), layer: Layer) {
        LTDC.layer(layer as usize).cfbar().write(|w| w.set_cfbadd(buffer as u32));
    }

    pub async fn reload(&mut self) {
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        let mut polled = false;
        poll_fn(|cx| {
            if !mem::replace(&mut polled, true) {
                cortex_m::interrupt::free(|_cs| {
                    VSYNC.register(cx.waker());
                    Interrupts::clear_pending();
                    Interrupts::enable_vector();
                    LTDC.srcr().write(|w| w.set_vbr(vals::Vbr::RELOAD));
                });

                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
    }
}

#[allow(dead_code)]
impl Interrupts {
    #[inline]
    pub fn read() -> Self {
        let flags = LTDC.isr().read();
        Self::from_bits_truncate(flags.0)
    }

    #[inline]
    pub fn clear(self) {
        LTDC.icr().write_value(regs::Icr(self.bits()));
    }

    #[inline]
    pub fn enable(self) {
        LTDC.ier().write_value(regs::Ier(self.bits()));
    }

    #[inline]
    pub fn clear_pending() {
        interrupt::LTDC::unpend();
    }

    #[inline]
    pub fn clear_pending_error() {
        interrupt::LTDC::unpend();
    }

    #[inline]
    pub fn enable_vector() {
        // Safety: critical section is priority based, not mask based
        unsafe {
            interrupt::LTDC::enable();
        }
    }

    #[inline]
    pub fn disable_vector() {
        interrupt::LTDC::disable();
    }

    #[inline]
    pub fn enable_error_vector() {
        // Safety: critical section is priority based, not mask based
        unsafe {
            interrupt::LTDC_ER::enable();
        }
    }

    #[inline]
    pub fn disable_error_vector() {
        interrupt::LTDC_ER::disable();
    }
}

pub struct InterruptHandler {}
pub struct ErrorInterruptHandler {}

impl interrupt::Handler<interrupt::LTDC> for InterruptHandler {
    unsafe fn on_interrupt() {
        Interrupts::disable_vector();
        VSYNC.wake();
    }
}

impl interrupt::Handler<interrupt::LTDC_ER> for ErrorInterruptHandler {
    unsafe fn on_interrupt() {
        Interrupts::disable_error_vector();
    }
}
