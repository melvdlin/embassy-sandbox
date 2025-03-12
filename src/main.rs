#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(maybe_uninit_fill)]
#![feature(iter_array_chunks)]
#![feature(array_chunks)]
#![feature(breakpoint)]

#[allow(unused_imports)]
use core::arch::breakpoint;
use core::mem::MaybeUninit;
use core::num::NonZeroU16;
use core::sync::atomic::AtomicUsize;

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::join::join3;
use embassy_futures::yield_now;
use embassy_net::Ipv4Address;
use embassy_sandbox::*;
use embassy_stm32::bind_interrupts;
#[allow(unused_imports)]
use embassy_stm32::dsihost::DsiHost;
use embassy_stm32::gpio;
use embassy_stm32::ltdc;
use embassy_stm32::peripherals;
use embassy_stm32::rcc;
use embassy_stm32::time::Hertz;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::once_lock::OnceLock;
use embassy_sync::watch;
use embassy_sync::watch::Watch;
use embassy_time::Duration;
use embassy_time::Timer;
use embedded_graphics::pixelcolor::Rgb888;
#[allow(unused_imports)]
use panic_halt as _;
use rand_core::RngCore;

const HOSTNAME: &str = "STM32F7-DISCO";
// first octet: locally administered (administratively assigned) unicast address;
// see https://en.wikipedia.org/wiki/MAC_address#IEEE_802c_local_MAC_address_usage
const MAC_ADDR: [u8; 6] = [0x02, 0xC7, 0x52, 0x67, 0x83, 0xEF];

bind_interrupts!(struct Irqs {
    ETH => net::EthIrHandler;
    RNG => net::RngIrHandler;
    DSI => DSIInterruptHandler;
});

pub struct DSIInterruptHandler {}

impl
    embassy_stm32::interrupt::typelevel::Handler<embassy_stm32::interrupt::typelevel::DSI>
    for DSIInterruptHandler
{
    unsafe fn on_interrupt() {
        let dsihost = embassy_stm32::pac::DSIHOST;
        let flags = dsihost.wisr().read();
        let tearing_effect = flags.teif();
        let end_of_refresh = flags.erif();
        _ = tearing_effect;
        _ = end_of_refresh;
        dsihost.wifcr().modify(|w| {
            w.set_cteif(true);
            w.set_cerif(true);
        });
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    _main(spawner).await
}

async fn _main(spawner: Spawner) -> ! {
    let (config, _ahb_freq, hse_freq) = config();
    let p = embassy_stm32::init(config);
    let mut _button =
        embassy_stm32::exti::ExtiInput::new(p.PA0, p.EXTI0, gpio::Pull::Down);
    let mut lcd_reset_pin =
        gpio::Output::new(p.PJ15, gpio::Level::High, gpio::Speed::High);

    // 128 Mib
    const SDRAM_SIZE: usize = (128 / 8) << 20;
    let memory: &'static mut [MaybeUninit<u8>] =
        unsafe { sdram::init::<SDRAM_SIZE>(sdram::create_sdram!(p)) };
    let (head, _tail) = memory.split_at_mut(4);
    let values: &[u8] = &[0x12, 0x21, 0xEF, 0xFE];
    for (src, dst) in values.iter().zip(head.iter_mut()) {
        dst.write(*src);
    }
    let head = unsafe { core::mem::transmute::<&mut [MaybeUninit<u8>], &mut [u8]>(head) };

    assert_eq!(head, values);

    let mut rng = embassy_stm32::rng::Rng::new(p.RNG, Irqs);
    let seeds = core::array::from_fn(|_| rng.next_u64());

    static DHCP_UP: watch::Watch<ThreadModeRawMutex, (), 3> = watch::Watch::new();
    static LOG_CHANNEL: log::Channel<ThreadModeRawMutex, 1024> = log::Channel::new();
    let dhcp_up_sender = DHCP_UP.dyn_sender();
    let net = async {
        let stack = net::stack_setup(
            spawner,
            &dhcp_up_sender,
            HOSTNAME,
            MAC_ADDR,
            seeds,
            Irqs,
            p.ETH,
            p.PA1,
            p.PA2,
            p.PC1,
            p.PA7,
            p.PC4,
            p.PC5,
            p.PG13,
            p.PG14,
            p.PG11,
        )
        .await;

        static LOG_UP: Watch<ThreadModeRawMutex, bool, 3> = Watch::new();
        let log_up_sender = LOG_UP.dyn_sender();

        let log_endpoint = (Ipv4Address::from([192, 168, 2, 161]), 1234);
        let log = log::log_task(
            log_endpoint,
            DHCP_UP.dyn_receiver().expect("not enough watch receivers available"),
            &LOG_CHANNEL,
            &log_up_sender,
            stack,
        );
        let echo = echo(1234, &LOG_CHANNEL, stack);
        let cli = cli::cli_task(
            4321,
            &LOG_CHANNEL,
            DHCP_UP.dyn_receiver().expect("not enough watch receivers available"),
            stack,
        );
        join3(log, echo, cli).await
    };

    let ld1 = gpio::Output::new(p.PJ13, gpio::Level::High, gpio::Speed::Low);
    let ld2 = gpio::Output::new(p.PJ5, gpio::Level::High, gpio::Speed::Low);
    let blink = blink(
        ld1,
        ld2,
        DHCP_UP.dyn_receiver().expect("not enough watch receivers available"),
    );

    // let mut dsi = DsiHost::new(p.DSIHOST, p.PJ2);
    // _ = dsi.write_cmd(0, 0, &[]);
    // dsi.disable_wrapper_dsi();
    // dsi.disable();

    let disp = async {
        rcc::enable_and_reset::<peripherals::DSIHOST>();
        let dsihost = embassy_stm32::pac::DSIHOST;

        // === global config ===

        // == clock and timing config ==

        // enable voltage regulator
        dsihost.wrpcr().modify(|w| w.set_regen(true));
        while !dsihost.wisr().read().rrs() {
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

        dsihost.wrpcr().modify(|w| {
            w.set_ndiv(pll_ndiv as u8);
            w.set_idf(pll_idf as u8 - 1);
            w.set_odf(pll_odf.ilog2() as u8);
            w.set_pllen(true);
        });

        while !dsihost.wisr().read().pllls() {
            yield_now().await;
        }

        // enable D-PHY digital section and clock lane module
        dsihost.pctlr().modify(|w| {
            w.set_den(true);
            w.set_cke(true);
        });

        // set clock lane control to auto and enable HS clock lane
        dsihost.clcr().modify(|w| {
            w.set_acr(false);
            w.set_dpcc(true);
        });

        // configure number of active data lanes
        // 0 = lane 0
        // 1 = lanes 0 and 1 (default)
        dsihost.pconfr().modify(|w| w.set_nl(1));

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
        dsihost.ccr().modify(|w| {
            w.set_txeckdiv(4);
        });

        // set unit interval in multiples of .25 ns
        let unit_interval = Hertz::mhz(1_000) * 4_u32 / hs_clk;
        debug_assert!(unit_interval <= 0b11_1111);
        dsihost.wpcr0().modify(|w| w.set_uix4(unit_interval as u8));

        // set stop wait time
        // (minimum wait time before requesting a HS transmission after stop state)
        // minimum is 10 (lanebyteclk cycles?)
        dsihost.pconfr().modify(|w| w.set_sw_time(10));

        // flow control config ==
        dsihost.pcr().modify(|w| {
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
        dsihost.mcr().modify(|w| w.set_cmdm(true));
        // set DSI wrapper to adapted command mode
        dsihost.wcfgr().modify(|w| w.set_dsim(true));

        // set DSI wrapper to 24 bit color
        dsihost.wcfgr().modify(|w| w.set_colmux(0b101));
        // set DSI host to use 24 bit color
        dsihost.lcolcr().modify(|w| w.set_colc(0b101));

        // set size in pixels of long write commands
        // DSI host pixel FIFO is 960 * 32-bit words
        // 24-bit color depth
        //  => max command size = 960 * 32/24 = 1280 px
        dsihost.lccr().modify(|w| w.set_cmdsize(960 * 32 / 24));

        // configure HSYNC, VSYNC and DATA ENABLE polarity
        // to match LTDC polarity config
        // default: all active high (0)
        dsihost.lpcr().modify(|w| {
            w.set_dep(false);
            w.set_vsp(false);
            w.set_hsp(false);
        });

        dsihost.wcfgr().modify(|w| {
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

        dsihost.cmcr().modify(|w| {
            // enable TE ack request
            // default: false
            w.set_teare(true);
        });

        dsihost.wier().write(|w| {
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
        dsihost.cmcr().modify(|w| {
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
        dsihost.cmcr().modify(|w| w.set_are(false));

        // dsihost.cr().modify(|w| w.set_en(true));
        otm8009a::init(
            dsihost,
            otm8009a::Config {
                framerate: otm8009a::FrameRateHz::_60,
                orientation: otm8009a::Orientation::Landscape,
                color_map: otm8009a::ColorMap::Rgb,
                rows: NonZeroU16::new(otm8009a::HEIGHT).expect("height must be nonzero"),
                cols: NonZeroU16::new(otm8009a::WIDTH).expect("width must be nonzero"),
            },
            &mut lcd_reset_pin,
            &mut _button,
        )
        .await;

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

        const ROWS: usize = 480;
        const COLS: usize = 800;
        const PIXELS: usize = COLS * ROWS;
        let mut ltdc = ltdc::Ltdc::new(p.LTDC);
        ltdc.init(&ltdc::LtdcConfiguration {
            active_width: COLS as u16,
            active_height: ROWS as u16,
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

        dsihost.cltcr().modify(|w| {
            // lanebyteclk cycles
            // copied from https://github.com/STMicroelectronics/STM32CubeF7/blob/18642033f376dead4f28f09f21b5ae75abb4020e/Projects/STM32F769I-Discovery/Examples/LCD_DSI/LCD_DSI_CmdMode_DoubleBuffer/Src/main.c#L299
            let time = 35;
            w.set_hs2lp_time(time);
            w.set_lp2hs_time(time);
        });

        dsihost.dltcr().modify(|w| {
            // lanebyteclk cycles
            // copied from https://github.com/STMicroelectronics/STM32CubeF7/blob/18642033f376dead4f28f09f21b5ae75abb4020e/Projects/STM32F769I-Discovery/Examples/LCD_DSI/LCD_DSI_CmdMode_DoubleBuffer/Src/main.c#L301
            let time = 35;
            w.set_hs2lp_time(time);
            w.set_lp2hs_time(time);
        });

        ltdc.init_layer(
            &ltdc::LtdcLayerConfig {
                layer: ltdc::LtdcLayer::Layer1,
                pixel_format: ltdc::PixelFormat::RGB888,
                window_x0: 0,
                window_x1: COLS as u16,
                window_y0: 0,
                window_y1: ROWS as u16,
            },
            None,
        );

        dsihost.cr().modify(|w| w.set_en(true));
        dsihost.wcr().modify(|w| w.set_dsien(true));
        ltdc.enable();

        let buf: &'static mut [MaybeUninit<u8>] = &mut memory[..PIXELS * 3];
        let mut framebuf = graphics::Framebuffer::<[u8; 3]>::new(buf, ROWS, COLS);
        use embedded_graphics::prelude::*;
        Ok(()) =
            framebuf.fill_solid(&framebuf.bounding_box(), Rgb888::new(0x57, 0x00, 0x7F));
        ltdc.set_buffer(ltdc::LtdcLayer::Layer1, framebuf.as_ptr().as_ptr().cast())
            .await
            .unwrap();
    };

    join3(blink, net, disp).await.0
}

async fn blink(
    ld1: gpio::Output<'_>,
    ld2: gpio::Output<'_>,
    dhcp_up: watch::DynReceiver<'_, ()>,
) -> ! {
    let mut ld1 = ld1;
    let mut ld2 = ld2;
    loop {
        ld1.set_high();
        if dhcp_up.contains_value() {
            ld2.set_high();
        }

        Timer::after_millis(500).await;
        ld1.set_low();

        Timer::after_millis(500).await;
        ld1.set_high();
        ld2.set_low();

        Timer::after_millis(500).await;
        ld1.set_low();

        Timer::after_millis(500).await;
    }
}

async fn echo<M, const N: usize>(
    port: u16,
    _log: &log::Channel<M, N>,
    stack: embassy_net::Stack<'_>,
) -> !
where
    M: RawMutex,
{
    use core::fmt::Write as FmtWrite;

    use embassy_net::tcp;
    use embedded_io_async::Write as AsyncWrite;
    use heapless::String;

    let mut rx_buf = [0; 4096];
    let mut tx_buf = [0; 4096];

    let mut server = tcp::TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    server.set_keep_alive(Some(Duration::from_secs(10)));
    server.set_timeout(Some(Duration::from_secs(20)));
    // let config_v4 = stack.config_v4();
    // let _config_v4 = config_v4;

    loop {
        if let Err(e) = server.accept(port).await {
            let _e = e;
            Timer::after_secs(1).await;
            continue;
        }

        let mut buf = [0; 512];
        let mut fmt = String::<1026>::new();
        loop {
            let len = match server.read(&mut buf).await {
                | Err(_) | Ok(0) => break,
                | Ok(len) => len,
            };
            let buf = &buf[..len];
            writeln!(fmt, "{}", buf.len())
                .expect("usize decimal repr should not exceed 1026 bytes");
            if server.write_all(fmt.as_bytes()).await.is_err() {
                break;
            }
            fmt.clear();
        }
        server.close();
        server.abort();
        _ = server.flush().await;
    }
}

// noinspection ALL
fn config() -> (embassy_stm32::Config, Hertz, Hertz) {
    use embassy_stm32::rcc::*;
    let mut config = embassy_stm32::Config::default();
    let hse_freq = Hertz::mhz(25);
    config.rcc = {
        // config is non-exhaustive
        let mut rcc = Config::default();
        // HSI == 16 MHz
        rcc.hsi = true;
        rcc.hse = Some(Hse {
            freq: hse_freq,
            mode: HseMode::Oscillator,
        });
        rcc.pll = Some(Pll {
            // PLL in == 25 MHz / 25 == 2 MHz
            prediv: PllPreDiv::DIV25,
            // PLL out == 1 MHz * 432 == 432 MHz
            mul: PllMul(432),
            // SYSCLK == PLL out / divp == 432 MHz / 2 == 216 MHz
            divp: Some(PllPDiv::DIV2),
            divq: None,
            divr: None,
        });
        rcc.pllsai = Some(Pll {
            // PLL in == 25 MHz / 25 == 1 MHz
            prediv: PllPreDiv::DIV25,
            // PLL out == 1 MHz * 192 == 192 MHz
            mul: PllMul(192),
            divp: None,
            divq: None,
            // LTDC clock == PLLSAIR / 2
            //            == PLL out / divr / 2
            //            == 192 MHz / 2 / 2
            //            == 48 MHz
            divr: Some(PllRDiv::DIV2),
        });
        rcc.pll_src = PllSource::HSE;
        rcc.sys = Sysclk::PLL1_P;
        // APB1 clock must not be faster than 54 MHz
        rcc.apb1_pre = APBPrescaler::DIV4;
        // AHB clock == SYSCLK / 2 = 108MHz
        rcc.ahb_pre = AHBPrescaler::DIV2;
        rcc
    };
    (config, Hertz::mhz(216), hse_freq)
}

// D0  = PC9
// D1  = PC10
// D2  = PE2
// D3  = PD13
// sck = PB2
// nss = DMA1
#[allow(dead_code)]
mod otm8009a {
    #[allow(unused_imports)]
    use core::arch::breakpoint;
    use core::array;
    use core::future;
    use core::num::NonZeroU16;
    use core::sync::atomic;

    use embassy_stm32::exti::ExtiInput;
    use embassy_stm32::gpio::Output;
    use embassy_stm32::pac::dsihost::Dsihost;
    use embassy_time::Timer;
    use embassy_time::WithTimeout;

    use crate::dsi;

    pub const WIDTH: u16 = 800;
    pub const HEIGHT: u16 = 480;

    pub const ID: u8 = 0x40;

    pub const HSYNC: u16 = 2;
    pub const HBP: u16 = 34;
    pub const HFP: u16 = 34;
    pub const VSYNC: u16 = 1;
    pub const VBP: u16 = 15;
    pub const VFP: u16 = 16;

    pub const FREQUENCY_DIVIDER: u16 = 2;

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[repr(u8)]
    pub enum FrameRateHz {
        _35 = 0,
        _40 = 1,
        _45 = 2,
        _50 = 3,
        _55 = 4,
        _60 = 5,
        _65 = 6,
        _70 = 7,
    }

    pub async fn init(
        dsi: embassy_stm32::pac::dsihost::Dsihost,
        config: Config,
        reset_pin: &mut Output<'_>,
        _button: &mut ExtiInput<'_>,
    ) {
        use dsi::*;
        let transactions = &dsi::TRANSACTIONS;
        let _transactions = transactions;

        async fn write_reg(dsi: Dsihost, addr: u16, data: &[u8]) {
            let [base, offset] = addr.to_be_bytes();
            dcs_write(dsi, 0, 0x00, [offset]).await;
            dcs_write(dsi, 0, base, data.iter().copied()).await;
        }

        async fn read_reg(dsi: Dsihost, addr: u16, dst: &mut [u8]) {
            let [base, offset] = addr.to_be_bytes();
            dcs_write(dsi, 0, 0x00, [offset]).await;
            dcs_read(dsi, 0, base, dst).await;
        }

        // reset active low
        reset_pin.set_low();
        Timer::after_millis(20).await;
        reset_pin.set_high();
        Timer::after_millis(10).await;

        dsi.cr().write(|w| w.set_en(true));
        dsi.wcr().write(|w| w.set_dsien(true));

        let mut id = [0; 3];
        let [id1, id2, id3] = &mut id;
        dcs_read(dsi, 0, 0xda, core::slice::from_mut(id1)).await;
        dcs_read(dsi, 0, 0xdb, core::slice::from_mut(id2)).await;
        dcs_read(dsi, 0, 0xdc, core::slice::from_mut(id3)).await;
        assert_eq!(id, [0x40, 0x00, 0x00]);

        // let mut id_two = [0; 3];
        // dsi::generic_read(dsihost, 0, &[0x04], &mut id_two).await;
        // assert_eq!(id_two, [0x40, 0x00, 0x00]);

        // button.wait_for_rising_edge().await;

        // enable command 2 (Manufacturer Command Set); enable param shift
        // address: 0xFF
        // params:  0x08, 0x09: MCS
        //          0x01:       EXTC (enable param shift)
        // enable MCS access
        // enable orise command 2 access
        write_reg(dsi, 0xff00, &[0x80, 0x09, 0x01]).await;
        write_reg(dsi, 0xff80, &[0x80, 0x09]).await;

        // set source output levels during porch and non-display area to GND
        write_reg(dsi, 0xc480, &[0b11 << 4]).await;
        Timer::after_millis(10).await;

        // register not documented
        write_reg(dsi, 0xc48a, &[0b100 << 4]).await;
        Timer::after_millis(10).await;

        // enable VCOM test mode (gvdd_en_test)
        // default: 0xa8
        write_reg(dsi, 0xc5b1, &[0xa9]).await;

        // set pump 4 and 5 VGH to 13V and -9V, respectively
        write_reg(dsi, 0xc591, &[0x34]).await;

        // enable column inversion
        write_reg(dsi, 0xc0b4, &[0x50]).await;

        // set VCOM to -1.2625 V
        write_reg(dsi, 0xd900, &[0x4e]).await;

        // set idle and normal framerate
        let framerate = config.framerate as u8;
        write_reg(dsi, 0xc181, &[framerate | framerate << 4]).await;

        // set RGB video mode VSync source to external;
        // HSync, Data Enable and clock to internal
        write_reg(dsi, 0xc1a1, &[0x08]).await;

        // set pump 4 and 5 to x6 => VGH = 6 * VDD and VGL = -6 * VDD
        write_reg(dsi, 0xc592, &[0x01]).await;

        // set pump 4 (VGH/VGL) clock freq to from line to 1/2 line
        write_reg(dsi, 0xc595, &[0x34]).await;

        // set GVDD/NGVDD from +- 5V to +- 4.625V
        write_reg(dsi, 0xd800, &[0x79, 0x79]).await;

        // set pump 1 clock freq to line (default)
        write_reg(dsi, 0xc594, &[0x33]).await;

        // set Source Driver Pull Low phase to 0x1b + 1 MCLK cycles
        write_reg(dsi, 0xc0a3, &[0x1b]).await;

        // enable flying Cap23, Cap24 and Cap32
        write_reg(dsi, 0xc582, &[0x83]).await;

        // set bias current of source OP to 1.2µA
        write_reg(dsi, 0xc481, &[0x83]).await;

        // set RGB video mode VSync, HSync and Data Enable sources to external;
        // clock to internal
        write_reg(dsi, 0xc1a1, &[0x0e]).await;

        // set panel type to normal
        write_reg(dsi, 0xb3a6, &[0x00, 0x01]).await;

        // GOA VST:     (reference point is end of back porch; unit = lines)
        // - tcon_goa_vst 1 shift: rising edge 5 cycles before reference point
        // - tcon_goa_vst 1 pulse width: 1 + 1 cyles
        // - tcon_goa_vst 1 tchop: delay rising edge by 0 cycles
        // - tcon_goa_vst 2 shift: rising edge 4 cycles before reference point
        // - tcon_goa_vst 2 pulse width: 1 + 1 cyles
        // - tcon_goa_vst 2 tchop: delay rising edge by 0 cycles
        write_reg(dsi, 0xce80, &[0x85, 0x01, 0x00, 0x84, 0x01, 0x00]).await;

        // GOA CLK A1:  (reference point is end of back porch)
        // - width: period = 2 * (1 + 1) units
        // - shift: rising edge 4 units before reference point
        // - switch: clock ends 825 (0x339) units after reference point
        // - extend: don't extend pulse
        // - tchop: delay rising edge by 0 units
        // - tglue: delay falling edge by 0 units
        // GOA CLK A2:
        // - period: 2 * (1 + 1) units
        // - shift: rising edge 3 units before reference point
        // - switch: clock ends 826 (0x33a) units after reference point
        // - extend: don't extend pulse
        // - tchop: delay rising edge by 0 units
        // - tglue: delay falling edge by 0 units
        write_reg(
            dsi,
            0xcea0,
            &[
                0x18, 0x04, 0x03, 0x39, 0x00, 0x00, 0x00, //
                0x18, 0x03, 0x03, 0x3A, 0x00, 0x00, 0x00,
            ],
        )
        .await;

        // GOA CLK A2:  (reference point is end of back porch; unit = lines)
        // - width: period = 2 * (1 + 1) units
        // - shift: rising edge 2 units before reference point
        // - switch: clock ends 827 (0x33b) units after reference point
        // - extend: don't extend pulse
        // - tchop: delay rising edge by 0 units
        // - tglue: delay falling edge by 0 units
        // GOA CLK A2:
        // - period: 2 * (1 + 1) units
        // - shift: rising edge 1 units before reference point
        // - switch: clock ends 828 (0x33c) units after reference point
        // - extend: don't extend pulse
        // - tchop: delay rising edge by 0 units
        // - tglue: delay falling edge by 0 units
        write_reg(
            dsi,
            0xceb0,
            &[
                0x18, 0x02, 0x03, 0x3B, 0x00, 0x00, 0x00, //
                0x18, 0x01, 0x03, 0x3C, 0x00, 0x00, 0x00,
            ],
        )
        .await;

        // GOA ECLK:    (unit = frames)
        // - normal  mode width: period = 2 * (1 + 1) units
        // - partial mode width: period = 2 * (1 + 1) units
        // - normal  mode tchop: rising edge delay = 32 (0x20) units
        // - partial mode tchop: rising edge delay = 32 (0x20) units
        // - eclk 1-4 follow: no effect because width > 0
        // - output level = tcon_goa_dir2
        // - set tcon_goa_clkx to toggle continuously until frame boundary + 1 line
        // - duty cycle = 50%
        // - 0 VSS lines before VGH
        // - pre-charge to GND period = 0
        write_reg(
            dsi,
            0xcfc0,
            &[0x01, 0x01, 0x20, 0x20, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00],
        )
        .await;

        // register not documented
        write_reg(dsi, 0xcfd0, &[0x00]).await;

        // GOA PAD output level during sleep = VGL
        write_reg(dsi, 0xcb80, &[0x00; 10]).await;
        // GOA PAD L output level = VGL
        write_reg(dsi, 0xcb90, &[0x00; 15]).await;
        // write_reg(dsihost, 0xcba0, &[0x00; 15]).await;
        write_reg(dsi, 0xcba0, &[0x00; 15]).await;
        write_reg(dsi, 0xcbb0, &[0x00; 10]).await;
        // write_reg(dsihost, 0xcbb0, &[0x00; 10]).await;
        // GOA PAD H 2..=6 to internal tcon_goa in normal mode
        write_reg(
            dsi,
            0xcbc0,
            &[
                0x00, 0x04, 0x04, 0x04, 0x04, //
                0x04, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x00, 0x00, 0x00, 0x00,
            ],
        )
        .await;
        // GOA PAD H 22..=26 to internal tcon_goa in normal mode
        write_reg(
            dsi,
            0xcbd0,
            &[
                0x00, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x04, 0x04, 0x04, 0x04, //
                0x04, 0x00, 0x00, 0x00, 0x00,
            ],
        )
        .await;
        // GOA PAD H ..=40 output level = VGL
        write_reg(dsi, 0xcbe0, &[0x00; 10]).await;
        // GOA PAD LVD output level = VGH
        write_reg(dsi, 0xcbf0, &[0xFF; 10]).await;

        // map GOA output pads to internal signals:
        // normal scan:
        // GOUT1:       none
        // GOUT2:       dir2
        // GOUT3:       clka1
        // GOUT4:       clka3
        // GOUT5:       vst1
        // GOUT6:       dir1
        // GOUT7..=21:  none
        // GOUT22:      dir2
        // GOUT23:      clka2
        // GOUT24:      clka4
        // GOUT25:      vst2
        // GOUT26:      dir1
        // GOUT27..=40: none
        write_reg(
            dsi,
            0xcc80,
            &[
                0x00, 0x26, 0x09, 0x0B, 0x01, //
                0x25, 0x00, 0x00, 0x00, 0x00,
            ],
        )
        .await;
        write_reg(
            dsi,
            0xcc90,
            &[
                0x00, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x26, 0x0A, 0x0C, 0x02,
            ],
        )
        .await;
        write_reg(
            dsi,
            0xcca0,
            &[
                0x25, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x00, 0x00, 0x00, 0x00,
            ],
        )
        .await;
        // reverse scan:
        // GOUT1:       none
        // GOUT2:       dir1
        // GOUT3:       clka4
        // GOUT4:       clka2
        // GOUT5:       vst2
        // GOUT6:       dir2
        // GOUT7..=21:  none
        // GOUT22:      dir1
        // GOUT23:      clka3
        // GOUT24:      clka1
        // GOUT25:      vst1
        // GOUT26:      dir2
        // GOUT27..=40: none
        write_reg(
            dsi,
            0xccb0,
            &[
                0x00, 0x25, 0x0C, 0x0A, 0x02, //
                0x26, 0x00, 0x00, 0x00, 0x00,
            ],
        )
        .await;
        write_reg(
            dsi,
            0xccc0,
            &[
                0x00, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x25, 0x0B, 0x09, 0x01,
            ],
        )
        .await;
        write_reg(
            dsi,
            0xccd0,
            &[
                0x26, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x00, 0x00, 0x00, 0x00, //
                0x00, 0x00, 0x00, 0x00, 0x00,
            ],
        )
        .await;

        // set pump 1 min/max DM
        write_reg(dsi, 0xc581, &[0x66]).await;

        // register not documented
        write_reg(dsi, 0xf5b6, &[0x06]).await;

        // set PWM freq to 19.531kHz
        write_reg(dsi, 0xc6b1, &[0x06]).await;

        // Gamma correction 2.2+ table
        write_reg(
            dsi,
            0xe100,
            &[
                0x00, 0x09, 0x0F, 0x0E, 0x07, 0x10, 0x0B, 0x0A, //
                0x04, 0x07, 0x0B, 0x08, 0x0F, 0x10, 0x0A, 0x01,
            ],
        )
        .await;
        // Gamma correction 2.2- table
        write_reg(
            dsi,
            0xe200,
            &[
                0x00, 0x09, 0x0F, 0x0E, 0x07, 0x10, 0x0B, 0x0A, //
                0x04, 0x07, 0x0B, 0x08, 0x0F, 0x10, 0x0A, 0x01,
            ],
        )
        .await;

        let mut gamma = [0x00; 16];
        read_reg(dsi, 0xe100, &mut gamma).await;
        gamma.fill(0);
        read_reg(dsi, 0xe200, &mut gamma).await;

        // exit CMD2 mode
        write_reg(dsi, 0xff00, &[0xff, 0xff, 0xff]).await;

        // standard DCS initialisation
        dcs_write(dsi, 0, cmd::Dcs::SLPOUT, None).await;
        Timer::after_millis(5).await;
        dcs_write(dsi, 0, cmd::Dcs::COLMOD, [cmd::Colmod::Rgb888 as u8]).await;
        dcs_write(dsi, 0, cmd::Dcs::RDDMADCTR, [cmd::Colmod::Rgb888 as u8]).await;

        // configure orientation and screen area
        let madctr =
            cmd::Madctr::from(config.orientation) | cmd::Madctr::from(config.color_map);
        dcs_write(dsi, 0, cmd::Dcs::MADCTR, [madctr.bits()]).await;
        let [col_hi, col_lo] = (config.cols.get() - 1).to_be_bytes();
        let [row_hi, row_lo] = (config.rows.get() - 1).to_be_bytes();
        dcs_write(dsi, 0, cmd::Dcs::CASET, [0, 0, col_hi, col_lo]).await;
        dcs_write(dsi, 0, cmd::Dcs::PASET, [0, 0, row_hi, row_lo]).await;

        // set display brightness
        dsi::dcs_write(dsi, 0, cmd::Dcs::WRDISBV, [0xFF]).await;

        // display backlight control config
        let wctrld = cmd::Ctrld::BRIGHTNESS_CONTROL_ON
            | cmd::Ctrld::DIMMING_ON
            | cmd::Ctrld::BACKLIGHT_ON;
        dcs_write(dsi, 0, cmd::Dcs::WRCTRLD, [wctrld.bits()]).await;

        // content adaptive brightness control config
        dcs_write(dsi, 0, cmd::Dcs::WRCABC, [Cabc::StillPicture as u8]).await;

        // set CABC minimum brightness
        dcs_write(dsi, 0, cmd::Dcs::WRCABCMB, [0xFF]).await;

        // turn display on
        dcs_write(dsi, 0, cmd::Dcs::DISPON, None).await;

        dcs_write(dsi, 0, cmd::Dcs::NOP, None).await;

        // send GRAM memory write to initiate frame write
        // via other DSI commands sent by LTDC

        dcs_write(dsi, 0, cmd::Dcs::RAMWR, None).await;
    }

    mod cmd {
        use super::ColorMap;
        use super::Format;
        use super::Orientation;

        #[repr(u8)]
        #[allow(clippy::upper_case_acronyms)]
        pub enum Dcs {
            NOP = 0x00,
            SWRESET = 0x01,
            RDDMADCTR = 0x0b, // read memory data access ctrl
            RDDCOLMOD = 0x0c, // read display pixel format
            SLPIN = 0x10,     // sleep in
            SLPOUT = 0x11,    // sleep out
            PTLON = 0x12,     // partialmode on

            DISPOFF = 0x28, // display on
            DISPON = 0x29,  // display off

            CASET = 0x2A, // Column address set
            PASET = 0x2B, // Page address set

            RAMWR = 0x2C, // Memory (GRAM) write
            RAMRD = 0x2E, // Memory (GRAM) read

            PLTAR = 0x30, // Partial area

            TEOFF = 0x34, // Tearing Effect Line Off
            TEEON = 0x35, // Tearing Effect Line On; 1 param: 'TELOM'

            MADCTR = 0x36, // memory access data ctrl; 1 param

            IDMOFF = 0x38, // Idle mode Off
            IDMON = 0x39,  // Idle mode On

            COLMOD = 0x3A, // Interface Pixel format

            RAMWRC = 0x3C, // Memory write continue
            RAMRDC = 0x3E, // Memory read continue

            WRTESCN = 0x44, // Write Tearing Effect Scan line
            RDSCNL = 0x45,  // Read  Tearing Effect Scan line

            // CABC Management, ie, Content Adaptive, Back light Control in IC OTM8009a
            WRDISBV = 0x51,  // Write Display Brightness; 1 param
            WRCTRLD = 0x53,  // Write CTRL Display; 1 param
            WRCABC = 0x55,   // Write Content Adaptive Brightness; 1 param
            WRCABCMB = 0x5E, // Write CABC Minimum Brightness; 1 param

            ID1 = 0xDA, // Read ID1
            ID2 = 0xDB, // Read ID2
            ID3 = 0xDC, // Read ID3
        }

        impl From<Dcs> for u8 {
            fn from(cmd: Dcs) -> Self {
                cmd as u8
            }
        }

        impl TryFrom<u8> for Dcs {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                Ok(match value {
                    | 0x00 => Dcs::NOP,
                    | 0x01 => Dcs::SWRESET,
                    | 0x0b => Dcs::RDDMADCTR,
                    | 0x0c => Dcs::RDDCOLMOD,
                    | 0x10 => Dcs::SLPIN,
                    | 0x11 => Dcs::SLPOUT,
                    | 0x12 => Dcs::PTLON,
                    | 0x28 => Dcs::DISPOFF,
                    | 0x29 => Dcs::DISPON,
                    | 0x2A => Dcs::CASET,
                    | 0x2B => Dcs::PASET,
                    | 0x2C => Dcs::RAMWR,
                    | 0x2E => Dcs::RAMRD,
                    | 0x30 => Dcs::PLTAR,
                    | 0x34 => Dcs::TEOFF,
                    | 0x35 => Dcs::TEEON,
                    | 0x36 => Dcs::MADCTR,
                    | 0x38 => Dcs::IDMOFF,
                    | 0x39 => Dcs::IDMON,
                    | 0x3A => Dcs::COLMOD,
                    | 0x3C => Dcs::RAMWRC,
                    | 0x3E => Dcs::RAMRDC,
                    | 0x44 => Dcs::WRTESCN,
                    | 0x45 => Dcs::RDSCNL,
                    | 0x51 => Dcs::WRDISBV,
                    | 0x53 => Dcs::WRCTRLD,
                    | 0x55 => Dcs::WRCABC,
                    | 0x5E => Dcs::WRCABCMB,
                    | 0xDA => Dcs::ID1,
                    | 0xDB => Dcs::ID2,
                    | 0xDC => Dcs::ID3,
                    | _ => return Err(()),
                })
            }
        }

        /// Tearing Effect Line Output Mode
        #[repr(u8)]
        pub enum TeeonTelom {
            VBlankOnly = 0x00,
            Both = 0x01,
        }

        impl TryFrom<u8> for TeeonTelom {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                Ok(match value {
                    | 0x00 => TeeonTelom::VBlankOnly,
                    | 0x01 => TeeonTelom::Both,
                    | _ => return Err(()),
                })
            }
        }

        bitflags::bitflags! {
            #[derive(Debug)]
            #[derive(Clone, Copy)]
            #[derive(PartialEq, Eq)]
            #[derive(Default)]
            #[derive(Hash)]
            pub struct Madctr: u8 {
                const RGB = 0 << 3;
                const BGR = 1 << 3;

                const VERT_REFRESH_TTB = 0 << 4;
                const VERT_REFRESH_BTT = 1 << 4;

                const ROW_COL_SWAP = 1 << 5;

                const COL_ADDR_LTR = 0 << 6;
                const COL_ADDR_RTL = 1 << 6;

                const ROW_ADDR_TTB = 0 << 7;
                const ROW_ADDR_BTT = 1 << 7;

                const PORTRAIT = Madctr::empty().bits();
                const LANDSCAPE = Madctr::ROW_COL_SWAP.bits() | Madctr::COL_ADDR_RTL.bits();
            }

        }

        impl From<Orientation> for Madctr {
            fn from(value: Orientation) -> Self {
                match value {
                    | Orientation::Portrait => Madctr::PORTRAIT,
                    | Orientation::Landscape => Madctr::LANDSCAPE,
                }
            }
        }

        impl From<ColorMap> for Madctr {
            fn from(value: ColorMap) -> Self {
                match value {
                    | ColorMap::Rgb => Madctr::RGB,
                    | ColorMap::Bgr => Madctr::BGR,
                }
            }
        }

        #[repr(u8)]
        pub enum Colmod {
            Rgb565 = 0x55,
            Rgb888 = 0x77,
        }

        impl TryFrom<u8> for Colmod {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                Ok(match value {
                    | 0x55 => Colmod::Rgb565,
                    | 0x77 => Colmod::Rgb565,
                    | _ => return Err(()),
                })
            }
        }

        impl From<Format> for Colmod {
            fn from(value: Format) -> Self {
                match value {
                    | Format::RGB888 => Colmod::Rgb888,
                    | Format::RGB565 => Colmod::Rgb565,
                }
            }
        }

        impl From<Colmod> for Format {
            fn from(value: Colmod) -> Self {
                match value {
                    | Colmod::Rgb565 => Format::RGB565,
                    | Colmod::Rgb888 => Format::RGB888,
                }
            }
        }

        bitflags::bitflags! {
            #[derive(Debug)]
            #[derive(Clone, Copy)]
            #[derive(PartialEq, Eq)]
            #[derive(Default)]
            #[derive(Hash)]
            pub struct Ctrld: u8 {
                const BACKLIGHT_OFF = 0 << 2;
                const BACKLIGHT_ON = 1 << 2;

                const DIMMING_OFF = 0 << 3;
                const DIMMING_ON = 1 << 3;

                const BRIGHTNESS_CONTROL_OFF = 0 << 5;
                const BRIGHTNESS_CONTROL_ON = 1 << 5;
            }

        }
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Default)]
    #[derive(Hash)]
    #[repr(u8)]
    pub enum Cabc {
        #[default]
        Off = 0b00,
        UserInterface = 0b01,
        StillPicture = 0b10,
        MovingImage = 0b11,
    }

    impl From<Cabc> for u8 {
        fn from(value: Cabc) -> Self {
            value as u8
        }
    }

    impl TryFrom<u8> for Cabc {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0b00 => Cabc::Off,
                | 0b01 => Cabc::UserInterface,
                | 0b10 => Cabc::StillPicture,
                | 0b11 => Cabc::MovingImage,
                | _ => return Err(()),
            })
        }
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]

    pub struct Config {
        pub framerate: FrameRateHz,
        pub orientation: Orientation,
        pub color_map: ColorMap,
        pub rows: NonZeroU16,
        pub cols: NonZeroU16,
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[repr(u8)]
    pub enum Format {
        RGB888 = 0,
        RGB565 = 2,
    }

    impl TryFrom<u8> for Format {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0 => Self::RGB888,
                | 2 => Self::RGB565,
                | _ => return Err(()),
            })
        }
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Default)]
    #[derive(Hash)]

    pub enum Orientation {
        #[default]
        Portrait,
        Landscape,
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Default)]
    #[derive(Hash)]
    pub enum ColorMap {
        #[default]
        Rgb,
        Bgr,
    }
}

#[allow(unused)]
mod dsi {
    use core::array;
    use core::array::from_fn;
    use core::future;
    use core::iter;
    use core::sync::atomic;
    use core::sync::atomic::AtomicUsize;

    use embassy_futures::yield_now;
    use embassy_stm32::pac::dsihost::Dsihost;
    use embassy_stm32::pac::dsihost::regs::Ghcr;
    use embassy_stm32::pac::rtc::regs::Tr;
    use embassy_sync::blocking_mutex;
    use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
    use embassy_sync::mutex::Mutex;
    use embassy_time::Duration;
    use scuffed_write::async_writeln;

    #[used]
    pub static GPDR_WORDS_WRITTEN: AtomicUsize = AtomicUsize::new(0);

    /// MUST NOT BE HELD ACROSS AWAIT POINTS
    #[used]
    pub static TRANSACTIONS: Mutex<
        ThreadModeRawMutex,
        heapless::Deque<Transaction, 1024>,
    > = Mutex::new(heapless::Deque::new());

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[repr(C)]
    pub struct Transaction {
        pub ty: TransactionType,
        pub data: u32,
    }

    impl Transaction {
        pub const fn new(ty: TransactionType, data: u32) -> Self {
            Self { ty, data }
        }

        pub const fn header(data: u32) -> Self {
            Self::new(TransactionType::HeaderWrite, data)
        }

        pub const fn write(data: u32) -> Self {
            Self::new(TransactionType::DataWrite, data)
        }

        pub const fn read(data: u32) -> Self {
            Self::new(TransactionType::DataRead, data)
        }
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[repr(u32)]
    pub enum TransactionType {
        HeaderWrite = 0x11111111,
        DataWrite = 0x22222222,
        DataRead = 0x33333333,
    }

    pub mod packet {
        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(Hash)]
        pub enum Type {
            Short(Short),
            Long(Long),
        }

        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(Hash)]
        #[repr(u8)]
        pub enum Short {
            GenericWrite0P = 0x03,
            GenericWrite1P = 0x13,
            GenericWrite2P = 0x23,
            GenericRead0P = 0x04,
            GenericRead1P = 0x14,
            GenericRead2P = 0x24,
            DCSWrite0P = 0x05,
            DCSWrite1P = 0x15,
            DCSRead0P = 0x06,
            SetMaxReturnPacketSize = 0x37,
        }

        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(Hash)]
        #[repr(u8)]
        pub enum Long {
            Null = 0x09,
            Blanking = 0x19,
            GenericWrite = 0x29,
            DCSWrite = 0x39,
            YCbCr20LooselyPacked = 0x0c,
            YCbCr24Packed = 0x1c,
            YCbCr16Packed = 0x2c,
            RGB30Packed = 0x0d,
            RGB36Packed = 0x1d,
            YCbCr12Packed = 0x3d,
            RGB16Packed = 0x0e,
            RGB18Packed = 0x1e,
            RGB18LooselyPacked = 0x2e,
            RGB24Packed = 0x3e,
        }

        impl TryFrom<u8> for Type {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                Short::try_from(value)
                    .map(Type::Short)
                    .or_else(|()| Long::try_from(value).map(Type::Long))
            }
        }

        impl From<Type> for u8 {
            fn from(value: Type) -> Self {
                match value {
                    | Type::Short(short) => u8::from(short),
                    | Type::Long(long) => u8::from(long),
                }
            }
        }

        impl From<Short> for Type {
            fn from(short: Short) -> Self {
                Type::Short(short)
            }
        }

        impl From<Long> for Type {
            fn from(long: Long) -> Self {
                Type::Long(long)
            }
        }

        impl From<Short> for u8 {
            fn from(short: Short) -> Self {
                short as Self
            }
        }

        impl TryFrom<u8> for Short {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                Ok(match value {
                    | 0x03 => Short::GenericWrite0P,
                    | 0x13 => Short::GenericWrite1P,
                    | 0x23 => Short::GenericWrite2P,
                    | 0x04 => Short::GenericRead0P,
                    | 0x14 => Short::GenericRead1P,
                    | 0x24 => Short::GenericRead2P,
                    | 0x05 => Short::DCSWrite0P,
                    | 0x15 => Short::DCSWrite1P,
                    | 0x06 => Short::DCSRead0P,
                    | 0x37 => Short::SetMaxReturnPacketSize,
                    | _ => return Err(()),
                })
            }
        }

        impl From<Long> for u8 {
            fn from(long: Long) -> Self {
                long as Self
            }
        }

        impl TryFrom<u8> for Long {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                Ok(match value {
                    | 0x09 => Long::Null,
                    | 0x19 => Long::Blanking,
                    | 0x29 => Long::GenericWrite,
                    | 0x39 => Long::DCSWrite,
                    | 0x0c => Long::YCbCr20LooselyPacked,
                    | 0x1c => Long::YCbCr24Packed,
                    | 0x2c => Long::YCbCr16Packed,
                    | 0x0d => Long::RGB30Packed,
                    | 0x1d => Long::RGB36Packed,
                    | 0x3d => Long::YCbCr12Packed,
                    | 0x0e => Long::RGB16Packed,
                    | 0x1e => Long::RGB18Packed,
                    | 0x2e => Long::RGB18LooselyPacked,
                    | 0x3e => Long::RGB24Packed,
                    | _ => return Err(()),
                })
            }
        }
    }

    pub async fn generic_write<I>(dsi: Dsihost, channel: u8, tx: I)
    where
        I: IntoIterator<Item = u8>,
        I::IntoIter: ExactSizeIterator,
    {
        let tx = tx.into_iter();
        let ty = match tx.len() {
            | 0 => packet::Type::Short(packet::Short::GenericWrite0P),
            | 1 => packet::Type::Short(packet::Short::GenericWrite1P),
            | 2 => packet::Type::Short(packet::Short::GenericWrite2P),
            | 3.. => packet::Type::Long(packet::Long::GenericWrite),
        };
        write(dsi, channel, ty, tx).await
    }

    pub async fn dcs_write<I>(dsi: Dsihost, channel: u8, cmd: impl Into<u8>, tx: I)
    where
        I: IntoIterator<Item = u8>,
        I::IntoIter: ExactSizeIterator,
    {
        let tx = tx.into_iter();
        let ty = match tx.len() {
            | 0 => packet::Type::Short(packet::Short::DCSWrite0P),
            | 1 => packet::Type::Short(packet::Short::DCSWrite1P),
            | 2.. => packet::Type::Long(packet::Long::DCSWrite),
        };
        write(dsi, channel, ty, iter::once(cmd.into()).chain(tx)).await
    }

    async fn write<I>(dsi: Dsihost, channel: u8, ty: packet::Type, tx: I)
    where
        I: IntoIterator<Item = u8>,
    {
        match ty {
            | packet::Type::Long(ty) => long_write(dsi, channel, ty, tx).await,
            | packet::Type::Short(ty) => {
                let mut tx = tx.into_iter();
                short_transfer(dsi, channel, ty, tx.next(), tx.next()).await
            }
        }
    }

    async fn long_write(
        dsi: Dsihost,
        channel: u8,
        ty: packet::Long,
        tx: impl IntoIterator<Item = u8>,
    ) {
        let mut len: u16 = 0;

        let mut bytes = tx.into_iter().inspect(|_| len += 1).array_chunks::<4>();

        wait_command_fifo_empty(dsi).await;

        for chunk in &mut bytes {
            wait_command_fifo_not_full(dsi).await;
            write_word(dsi, u32::from_le_bytes(chunk));
            wait_command_fifo_empty(dsi).await;
        }

        if let Some(mut remainder) = bytes.into_remainder() {
            wait_command_fifo_not_full(dsi).await;
            write_word(
                dsi,
                u32::from_le_bytes(array::from_fn(|_| remainder.next().unwrap_or(0))),
            );

            wait_command_fifo_empty(dsi).await;
        }

        let [lsb, msb] = len.to_le_bytes();
        config_header(dsi, ty, channel, lsb, msb);

        wait_command_fifo_empty(dsi).await;
        wait_payload_write_fifo_empty(dsi).await;
    }

    async fn short_transfer(
        dsi: Dsihost,
        channel: u8,
        ty: packet::Short,
        p0: Option<u8>,
        p1: Option<u8>,
    ) {
        wait_command_fifo_empty(dsi).await;

        config_header(dsi, ty, channel, p0.unwrap_or(0), p1.unwrap_or(0));

        wait_command_fifo_empty(dsi).await;
        wait_payload_write_fifo_empty(dsi).await;
    }

    pub async fn generic_read(dsi: Dsihost, channel: u8, args: &[u8], dst: &mut [u8]) {
        assert!(args.len() <= 2);
        let ty = match args.len() {
            | 0 => packet::Short::GenericRead0P,
            | 1 => packet::Short::GenericRead1P,
            | 2 => packet::Short::GenericRead2P,
            | _ => unreachable!(),
        };

        read(
            dsi,
            channel,
            ty,
            #[allow(clippy::get_first)]
            args.get(0).copied(),
            args.get(1).copied(),
            dst,
        )
        .await
    }

    pub async fn dcs_read(dsi: Dsihost, channel: u8, cmd: u8, dst: &mut [u8]) {
        read(dsi, channel, packet::Short::DCSRead0P, Some(cmd), None, dst).await
    }

    async fn read(
        dsi: Dsihost,
        channel: u8,
        ty: packet::Short,
        p0: Option<u8>,
        p1: Option<u8>,
        dst: &mut [u8],
    ) {
        let len = u16::try_from(dst.len()).expect("read len out of bounds for u16");

        wait_command_fifo_empty(dsi).await;

        if len > 2 {
            set_max_return(dsi, channel, len);
        }

        config_header(dsi, ty, channel, p0.unwrap_or(0), p1.unwrap_or(0));

        wait_read_not_busy(dsi).await;

        let mut bytes = dst.array_chunks_mut::<4>();
        for chunk in &mut bytes {
            wait_payload_read_fifo_not_empty(dsi).await;
            *chunk = read_word(dsi).to_le_bytes();
        }

        let remainder = bytes.into_remainder();
        if !remainder.is_empty() {
            wait_payload_read_fifo_not_empty(dsi).await;
            let word = read_word(dsi).to_le_bytes();
            remainder.copy_from_slice(&word[..remainder.len()]);
        }
    }

    #[inline]
    fn set_max_return(dsi: Dsihost, channel: u8, size: u16) {
        let [lsb, msb] = size.to_le_bytes();
        config_header(
            dsi,
            packet::Short::SetMaxReturnPacketSize,
            channel,
            lsb,
            msb,
        )
    }

    fn config_header(
        dsi: Dsihost,
        dt: impl Into<packet::Type>,
        channel: u8,
        wclsb: u8,
        wcmsb: u8,
    ) {
        let mut ghcr = Ghcr::default();
        ghcr.set_dt(dt.into().into());
        ghcr.set_vcid(channel);
        ghcr.set_wclsb(wclsb);
        ghcr.set_wcmsb(wcmsb);

        dsi.ghcr().write_value(ghcr);

        #[cfg(debug_assertions)]
        report_transaction(Transaction::header(ghcr.0));
    }

    fn write_word(dsi: Dsihost, word: u32) {
        dsi.gpdr().write_value(embassy_stm32::pac::dsihost::regs::Gpdr(word));

        #[cfg(debug_assertions)]
        {
            GPDR_WORDS_WRITTEN.fetch_add(1, atomic::Ordering::Relaxed);
            report_transaction(Transaction::write(word));
        }
    }

    fn read_word(dsi: Dsihost) -> u32 {
        let word = dsi.gpdr().read().0;

        #[cfg(debug_assertions)]
        report_transaction(Transaction::read(word));

        word
    }

    fn report_transaction(transaction: Transaction) {
        let mut t = TRANSACTIONS.try_lock().expect("deadlock");
        if t.is_full() {
            t.pop_front();
        }
        t.push_back(transaction).expect("transaction fifo has 0 capacity");
    }

    async fn wait_command_fifo_empty(dsi: Dsihost) {
        while !dsi.gpsr().read().cmdfe() {
            yield_now().await
        }
    }

    async fn wait_command_fifo_not_full(dsi: Dsihost) {
        while dsi.gpsr().read().cmdff() {
            yield_now().await
        }
    }

    async fn wait_read_not_busy(dsi: Dsihost) {
        while dsi.gpsr().read().rcb() {
            yield_now().await
        }
    }

    async fn wait_payload_read_fifo_not_empty(dsi: Dsihost) {
        while dsi.gpsr().read().prdfe() {
            yield_now().await
        }
    }

    async fn wait_payload_write_fifo_empty(dsi: Dsihost) {
        while !dsi.gpsr().read().pwrfe() {
            yield_now().await
        }
    }
}
