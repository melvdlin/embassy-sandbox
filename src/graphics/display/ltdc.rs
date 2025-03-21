use embassy_stm32::ltdc::RgbColor;
use embassy_stm32::pac;
use embassy_stm32::pac::ltdc::vals;
use embassy_stm32::pac::ltdc::vals::Depol;
use embassy_stm32::pac::ltdc::vals::Hspol;
use embassy_stm32::pac::ltdc::vals::Pcpol;
use embassy_stm32::pac::ltdc::vals::Vspol;
use embassy_stm32::pac::spdifrx::regs;
use embassy_stm32::peripherals;

use crate::graphics::color::Argb8888;

pub type Peripheral = peripherals::LTDC;
type PacLtdc = pac::ltdc::Ltdc;

const LTDC: PacLtdc = pac::LTDC;

pub struct Ltdc {
    _peripheral: Peripheral,
}

pub struct LayerConfig {
    pub x_offset: u16,
    pub y_offset: u16,
    pub width: u16,
    pub height: u16,
    pub pixel_format: embassy_stm32::ltdc::PixelFormat,
    pub alpha: u8,
    pub default_color: Argb8888,
}

impl Ltdc {
    pub async fn init(
        ltdc: Peripheral,
        background: RgbColor,
        cfg: &embassy_stm32::ltdc::LtdcConfiguration,
    ) -> Self {
        embassy_stm32::rcc::enable_and_reset::<peripherals::LTDC>();

        // configure HS, VS, DE and PC polarity
        LTDC.gcr().modify(|w| {
            w.set_hspol(match cfg.h_sync_polarity {
                | embassy_stm32::ltdc::PolarityActive::ActiveLow => Hspol::ACTIVE_HIGH,
                | embassy_stm32::ltdc::PolarityActive::ActiveHigh => Hspol::ACTIVE_LOW,
            });
            w.set_vspol(match cfg.v_sync_polarity {
                | embassy_stm32::ltdc::PolarityActive::ActiveLow => Vspol::ACTIVE_HIGH,
                | embassy_stm32::ltdc::PolarityActive::ActiveHigh => Vspol::ACTIVE_LOW,
            });
            w.set_depol(match cfg.data_enable_polarity {
                | embassy_stm32::ltdc::PolarityActive::ActiveLow => Depol::ACTIVE_HIGH,
                | embassy_stm32::ltdc::PolarityActive::ActiveHigh => Depol::ACTIVE_LOW,
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

        // TODO: enable and handle error IRs

        // enable LTDC
        LTDC.gcr().modify(|w| w.set_ltdcen(true));

        Ltdc { _peripheral: ltdc }
    }

    pub async fn config_layer(
        &mut self,
        layer: embassy_stm32::ltdc::LtdcLayer,
        framebuffer: *const (),
        cfg: &LayerConfig,
    ) {
        let h_win_start = cfg.x_offset + LTDC.bpcr().read().ahbp() + 1;
        let h_win_stop = h_win_start + cfg.width;
        let v_win_start = cfg.y_offset + LTDC.bpcr().read().avbp() + 1;
        let v_win_stop = v_win_start + cfg.height;

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
            let Argb8888 {
                alpha,
                red,
                green,
                blue,
            } = cfg.default_color;
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
        layer.cfbar().write(|w| w.set_cfbadd(framebuffer as u32));

        // frame buffer line length and pitch (offset between start of subsequent lines)
        let pixel_size = cfg.pixel_format.bytes_per_pixel() as u16;
        layer.cfblr().write(|w| {
            w.set_cfbll(cfg.width * pixel_size + 3);
            w.set_cfbp(cfg.width * pixel_size);
        });

        // frame buffer line count
        layer.cfblnr().write(|w| {
            w.set_cfblnbr(cfg.height);
        });

        layer.cr().write(|w| w.set_len(true));

        LTDC.srcr().modify(|w| w.set_imr(vals::Imr::RELOAD));
    }
}
