use core::convert::Infallible;
use core::mem::MaybeUninit;
use core::num::NonZeroU16;

use embassy_futures::yield_now;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Output;
use embassy_stm32::ltdc;
use embassy_stm32::peripherals;
use embassy_stm32::rcc;
use embassy_stm32::time::Hertz;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::Dimensions;
use embedded_graphics::prelude::DrawTarget;

mod dsi;
mod otm8009a;
pub use dsi::InterruptHandler as DSIInterruptHandler;
pub use otm8009a::ColorMap;
pub use otm8009a::Config;
pub use otm8009a::FrameRateHz;
pub use otm8009a::HEIGHT;
pub use otm8009a::Orientation;
pub use otm8009a::WIDTH;

pub async fn init(
    _dsi: embassy_stm32::peripherals::DSIHOST,
    ltdc: embassy_stm32::peripherals::LTDC,
    buf: &mut [MaybeUninit<u8>],
    config: otm8009a::Config,
    hse_freq: Hertz,
    reset_pin: &mut Output<'_>,
    _button: &mut ExtiInput<'_>,
) -> impl DrawTarget<Color = Rgb888, Error = Infallible> {
    let mut framebuf = super::framebuffer::Framebuffer::new(
        buf,
        config.rows.get() as usize,
        config.cols.get() as usize,
    );

    rcc::enable_and_reset::<peripherals::DSIHOST>();
    let dsi = embassy_stm32::pac::DSIHOST;

    // === global config ===

    // == clock and timing config ==

    // enable voltage regulator
    dsi.wrpcr().modify(|w| w.set_regen(true));
    while !dsi.wisr().read().rrs() {
        yield_now().await;
    }

    // PLL setup
    let f_vco = Hertz::mhz(1_000);
    let hs_clk = Hertz::mhz(500);
    let pll_idf = 1_u32;
    let pll_ndiv = f_vco / ((hse_freq * 2_u32) / pll_idf);
    let pll_odf = (f_vco / 2_u32) / hs_clk;

    debug_assert!((10..=125).contains(&pll_ndiv));
    debug_assert!((1..=7).contains(&pll_idf));
    debug_assert!([1, 2, 4, 8].contains(&pll_odf));

    dsi.wrpcr().modify(|w| {
        w.set_ndiv(pll_ndiv as u8);
        w.set_idf(pll_idf as u8 - 1);
        w.set_odf(pll_odf.ilog2() as u8);
        w.set_pllen(true);
    });

    while !dsi.wisr().read().pllls() {
        yield_now().await;
    }

    // enable D-PHY digital section and clock lane module
    dsi.pctlr().modify(|w| {
        w.set_den(true);
        w.set_cke(true);
    });

    // set clock lane control to auto and enable HS clock lane
    dsi.clcr().modify(|w| {
        w.set_acr(false);
        w.set_dpcc(true);
    });

    // configure number of active data lanes
    // 0 = lane 0
    // 1 = lanes 0 and 1 (default)
    dsi.pconfr().modify(|w| w.set_nl(1));

    // lane_byte_clk
    //  = fvco / (2 * odf * 8)
    //  = 1 GHz / (2 * 1 * 8)
    //  = 1/16 GHz
    //  = 62.5 MHz

    // TX escape clock = lane_byte_clk / TXECKDIV < 20 MHz
    //  <=> TXECKDIV > lane_byte_clk / 20 MHz
    //  <=> TXECKDIV > (62.5 / 20) MHz
    //  <=> TXECKDIV > 3.125
    // Timeout clock div = 1
    dsi.ccr().modify(|w| {
        w.set_txeckdiv(4);
    });

    // set unit interval in multiples of .25 ns
    let unit_interval = Hertz::mhz(1_000) * 4_u32 / hs_clk;
    debug_assert!(unit_interval <= 0b11_1111);
    dsi.wpcr0().modify(|w| w.set_uix4(unit_interval as u8));

    // set stop wait time
    // (minimum wait time before requesting a HS transmission after stop state)
    // minimum is 10 (lanebyteclk cycles?)
    dsi.pconfr().modify(|w| w.set_sw_time(10));

    dsi.cltcr().modify(|w| {
        // lanebyteclk cycles
        // copied from https://github.com/STMicroelectronics/STM32CubeF7/blob/18642033f376dead4f28f09f21b5ae75abb4020e/Projects/STM32F769I-Discovery/Examples/LCD_DSI/LCD_DSI_CmdMode_DoubleBuffer/Src/main.c#L299
        let time = 35;
        w.set_hs2lp_time(time);
        w.set_lp2hs_time(time);
    });

    dsi.dltcr().modify(|w| {
        // lanebyteclk cycles
        // copied from https://github.com/STMicroelectronics/STM32CubeF7/blob/18642033f376dead4f28f09f21b5ae75abb4020e/Projects/STM32F769I-Discovery/Examples/LCD_DSI/LCD_DSI_CmdMode_DoubleBuffer/Src/main.c#L301
        let time = 35;
        w.set_hs2lp_time(time);
        w.set_lp2hs_time(time);
    });

    // flow control config ==
    dsi.pcr().modify(|w| {
        // enable EoT packet transmission
        // w.set_ettxe(true);
        // enable EoT packet reception
        // w.set_etrxe(true);
        // enable automatic bus turnaround
        w.set_btae(true);
        // check ECC of received packets
        // w.set_eccrxe(true);
        // // check CRC checksum of received packets
        // w.set_crcrxe(true);
    });

    // === adapted command mode config ===

    // set DSI host to adapted command mode
    dsi.mcr().modify(|w| w.set_cmdm(true));
    // set DSI wrapper to adapted command mode
    dsi.wcfgr().modify(|w| w.set_dsim(true));

    // set DSI wrapper to 24 bit color
    dsi.wcfgr().modify(|w| w.set_colmux(0b101));
    // set DSI host to use 24 bit color
    dsi.lcolcr().modify(|w| w.set_colc(0b101));

    // set size in pixels of long write commands
    // DSI host pixel FIFO is 960 * 32-bit words
    // 24-bit color depth
    //  => max command size = 960 * 32/24 = 1280 px
    let pixel_fifo_size = 960 * 32 / 24;
    dsi.lccr().modify(|w| w.set_cmdsize(pixel_fifo_size));

    // configure HSYNC, VSYNC and DATA ENABLE polarity
    // to match LTDC polarity config
    // default: all active high (0)
    dsi.lpcr().modify(|w| {
        w.set_dep(false);
        w.set_vsp(false);
        w.set_hsp(false);
    });

    dsi.wcfgr().modify(|w| {
        // configure tearing effect
        // TE effect over link
        //  => acknowledge request must be enabled
        //  && bus turnaround in PCR must be enabled
        // tearing effect over pin
        // 0: in-link (default)
        // 1: external pin
        w.set_tesrc(false);

        // polarity depends on display TE pin polarity
        // 0: rising (default)
        // 1: falling
        w.set_tepol(false);

        // configure auto refresh
        // auto refresh:   WCR_LTDCEN is set automatically on TE event
        // manual refresh: WCR_LTDCEN must be set by software on TE event (default)
        w.set_ar(false);

        // configure VSync polarity
        // set LTDC halt polarity in accordance with LPCR:
        // LPCR VSYNC active high <=> LTDC halt on rising edge
        // 0: halt on falling edge (default)
        // 1: halt on rising edge
        w.set_vspol(false);
    });

    dsi.cmcr().modify(|w| {
        // enable TE ack request
        // default: false
        w.set_teare(true);
    });

    dsi.wier().write(|w| {
        // enable tearing effect interrupt
        w.set_teie(true);
        // enable end of refresh interrupt
        w.set_erie(true);
    });

    // command transmission mode
    // commands may be transmitted in either high-speed or low-power
    // default: high-speed (0)
    //
    // some displays require init commands to be sent in LP mode
    dsi.cmcr().modify(|w| {
        // generic short write zero params
        w.set_gsw0tx(false);
        // generic short write one param
        w.set_gsw1tx(false);
        // generic short write two params
        w.set_gsw2tx(false);
        // generic short read zero params
        w.set_gsr0tx(false);
        // generic short read one param
        w.set_gsr1tx(false);
        // generic short read two params
        w.set_gsr2tx(false);
        // generic long write
        w.set_glwtx(false);
        // DCS short write zero params
        w.set_dsw0tx(false);
        // DCS short write one param
        w.set_dsw1tx(false);
        // DCS short read zero params
        w.set_dsr0tx(false);
        // DCS long write
        w.set_dlwtx(false);
        // maximum read packet size
        w.set_mrdps(false);
    });

    // request an acknowledge after every sent command
    // default: false
    dsi.cmcr().modify(|w| w.set_are(false));

    dsi.ier1().write(|w| {
        // LTDC payload write error
        w.set_lpwreie(true);
        // generic command write error
        w.set_gcwreie(true);
        // generic payload write error
        w.set_gpwreie(true);
        // gemeroc payload tx error
        w.set_gptxeie(true);
        // generic payload read error
        w.set_gprdeie(true);
        // generic payload rx error
        w.set_gprxeie(true);
    });

    // dsi.cr().write(|w| w.set_en(true));
    // dsi.wcr().write(|w| w.set_dsien(true));

    let mut ltdc = ltdc::Ltdc::new(ltdc);
    ltdc.init(&ltdc::LtdcConfiguration {
        active_width: config.rows.get(),
        active_height: config.rows.get(),
        h_back_porch: 1,
        h_front_porch: 1,
        v_back_porch: 1,
        v_front_porch: 1,
        h_sync: 1,
        v_sync: 1,
        h_sync_polarity: ltdc::PolarityActive::ActiveHigh,
        v_sync_polarity: ltdc::PolarityActive::ActiveHigh,
        data_enable_polarity: ltdc::PolarityActive::ActiveLow,
        pixel_clock_polarity: ltdc::PolarityEdge::RisingEdge,
    });

    otm8009a::init(
        dsi,
        otm8009a::Config {
            framerate: otm8009a::FrameRateHz::_65,
            orientation: otm8009a::Orientation::Landscape,
            color_map: otm8009a::ColorMap::Rgb,
            rows: NonZeroU16::new(otm8009a::HEIGHT).expect("height must be nonzero"),
            cols: NonZeroU16::new(otm8009a::WIDTH).expect("width must be nonzero"),
        },
        pixel_fifo_size,
        reset_pin,
        _button,
    )
    .await;

    Ok(()) = framebuf.fill_solid(&framebuf.bounding_box(), Rgb888::new(0x57, 0x00, 0x7F));

    // ltdc.set_buffer(ltdc::LtdcLayer::Layer1, framebuf.as_ptr().as_ptr().cast())
    //     .await
    //     .unwrap();

    let ltdc_p = embassy_stm32::pac::LTDC;
    ltdc_p
        .layer(0)
        .cfbar()
        .modify(|w| w.set_cfbadd(framebuf.as_ptr().as_ptr().cast::<()>() as u32));

    ltdc.init_layer(
        &ltdc::LtdcLayerConfig {
            layer: ltdc::LtdcLayer::Layer1,
            pixel_format: ltdc::PixelFormat::RGB888,
            window_x0: 0,
            window_x1: config.rows.get(),
            window_y0: 0,
            window_y1: config.rows.get(),
        },
        None,
    );

    // ltdc.enable();
    dsi.wcr().modify(|w| w.set_ltdcen(true));

    // LTDC settings
    // pixel clock:
    //  - must be fast enough to ensure that GRAM refresh time
    //    is shorter than display internal refresh rate
    //  - must be slow enough to avoid LTDC FIFO underrun
    // video timing:
    //  - vertical/horizontal blanking periods may be set to minumum (1)
    //  - HACT and VACT must be set in accordance with line length and count
    // ...

    loop {
        yield_now().await
    }

    #[allow(unreachable_code)]
    framebuf
}
