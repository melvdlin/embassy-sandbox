use embassy_stm32::ltdc::RgbColor;
use embassy_stm32::pac;
use embassy_stm32::pac::ltdc::vals::Depol;
use embassy_stm32::pac::ltdc::vals::Hspol;
use embassy_stm32::pac::ltdc::vals::Pcpol;
use embassy_stm32::pac::ltdc::vals::Vspol;
use embassy_stm32::peripherals;

pub type Peripheral = peripherals::LTDC;
type PacLtdc = pac::ltdc::Ltdc;

const LTDC: PacLtdc = pac::LTDC;

pub struct Ltdc {
    _peripheral: Peripheral,
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
}
