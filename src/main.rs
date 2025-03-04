#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(maybe_uninit_fill)]
#![feature(iter_array_chunks)]
#![feature(array_chunks)]

use core::mem::MaybeUninit;

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

        static LOG_CHANNEL: log::Channel<ThreadModeRawMutex, 1024> = log::Channel::new();
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

    {
        rcc::enable_and_reset::<peripherals::DSIHOST>();
        let dsihost = embassy_stm32::pac::DSIHOST;

        // === global config ===

        // enable voltage regulator
        dsihost.wrpcr().modify(|w| w.set_regen(true));
        while !dsihost.wisr().read().rrs() {
            yield_now().await;
        }

        // PLL setup
        let f_vco = Hertz::mhz(1_000);
        let hs_clk = Hertz::mhz(500);
        let pll_idf = 1_u32;
        let pll_ndiv = f_vco * pll_idf / hse_freq / 2;
        let pll_odf = hs_clk / (f_vco / 2_u32);

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

        // lane_byte_clk
        //  = fvco / (2 * odf * 8)
        //  = 1 GHz / (2 * 1 * 8)
        //  = 1/16 GHz
        //  = 62.5 MHz

        // TX escape clock = lane_byte_clk / TXECKDIV < 20 MHz
        //  <=> TXECKDIV > lane_byte_clk / 20 MHz
        //  <=> TXECKDIV > (62.5 / 20) MHz
        //  <=> TXECKDIV > 3.125
        dsihost.ccr().modify(|w| w.set_txeckdiv(4));

        // configure number of active data lanes
        // 0 = lane 0
        // 1 = lanes 0 and 1 (default)
        // dsihost.pconfr().modify(|w| w.set_nl(1));

        // enable D-PHY digital section and clock lane module
        dsihost.pctlr().modify(|w| {
            w.set_den(true);
            w.set_cke(true);
        });

        // set clock lane control to auto and enable HS clock lane
        dsihost.clcr().modify(|w| {
            w.set_acr(true);
            w.set_dpcc(true);
        });

        // set unit interval in multiples of .25 ns
        let unit_interval = Hertz::mhz(1_000) * 4_u32 / hs_clk;
        debug_assert!(unit_interval <= 0b11_1111);
        dsihost.wpcr0().modify(|w| w.set_uix4(unit_interval as u8));

        dsihost.pcr().modify(|w| {
            // enable EoT packet transmission
            w.set_ettxe(true);
            // enable EoT packet reception
            w.set_etrxe(true);
            // enable automatic bus turnaround
            w.set_btae(true);
            // check ECC of received packets
            w.set_eccrxe(true);
            // check CRC checksum of received packets
            w.set_crcrxe(true);
        });

        // set DSI wrapper to 24 bit color
        dsihost.wcfgr().modify(|w| w.set_colmux(0b101));
        // set DSI host to use 24 bit color
        dsihost.lcolcr().modify(|w| w.set_colc(0b101));

        // configure HSYNC, VSYNC and DATA ENABLE polarity
        // to match LTDC polarity config
        // default: all active high (0)
        // dsihost.lpcr().modify(|w| {
        //     w.set_hsp(false);
        //     w.set_vsp(false);
        //     w.set_dep(false);
        // });

        // === adapted command mode config ===

        // set DSI host to adapted command mode
        dsihost.mcr().modify(|w| w.set_cmdm(true));
        // set DSI wrapper to adapted command mode
        dsihost.wcfgr().modify(|w| w.set_dsim(true));

        // set stop wait time
        // (minimum wait time before requesting a HS transmission after stop state)
        // minimum is 10 (lanebyteclk cycles?)
        dsihost.pconfr().modify(|w| w.set_sw_time(10));

        // set size in pixels of long write commands
        // DSI host pixel FIFO is 960 * 32-bit words
        // 24-bit color depth
        //  => max command size = 960 * 32/24 = 1280 px
        dsihost.lccr().modify(|w| w.set_cmdsize(960 * 32 / 24));

        // set LTDC halt polarity in accordance with LPCR:
        // LPCR VSYNC active high <=> LTDC halt on rising edge
        // default: halt on falling edge (0)
        // dsihost.wcfgr().modify(|w| w.set_vspol(true));

        // configure tearing effect
        // TE effect over link
        //  => acknowledge request must be enabled
        //  && bus turnaround in PCR must be enabled
        dsihost.wcfgr().modify(|w| {
            // tearing effect over pin
            // w.set_tesrc(true);
            // polarity depends on display TE pin polarity
            // 0: rising (default)
            // 0: falling
            // w.set_tepol(true);
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

        // configure refresh mode
        // auto refresh:   WCR_LTDCEN is set automatically on TE event
        // manual refresh: WCR_LTDCEN must be set by software on TE event (default)
        // dsihost.wcfgr().modify(|w| w.set_ar(true));

        // command transmission mode
        // commands may be transmitted in either high-speed or low-power
        // default: high-speed (0)
        //
        // some displays require init commands to be sent in LP mode
        dsihost.cmcr().modify(|w| {
            // generic short write zero params
            w.set_gsw0tx(true);
            // generic short write one param
            w.set_gsw1tx(true);
            // generic short write two params
            w.set_gsw2tx(true);
            // generic short read zero params
            w.set_gsr0tx(true);
            // generic short read one param
            w.set_gsr1tx(true);
            // generic short read two params
            w.set_gsr2tx(true);
            // generic long write
            w.set_glwtx(true);
            // DCS short write zero params
            w.set_dsw0tx(true);
            // DCS short write one param
            w.set_dsw1tx(true);
            // DCS short read zero params
            w.set_dsr0tx(true);
            // DCS long write
            w.set_dlwtx(true);
            // maximum read packet size
            // w.set_mrdps(false);
        });

        // === OTM8009a cfg ===

        const PARAM_SHIFT: u8 = 0x00;

        // enable command 2 (Manufacturer Command Set); enable param shift
        // address: 0xFF
        // params:  0x08, 0x09: MCS
        //          0x01:       EXTC (enable param shift)
        dsi::generic_write(dsihost, 0, [0xFF, 0x80, 0x09, 0x01]).await;
        // shift base address by 0x80
        dsi::generic_write(dsihost, 0, [PARAM_SHIFT, 0x80]).await;
        // enable access to orise command 2
        dsi::generic_write(dsihost, 0, [0xFF, 0x80, 0x09]).await;

        dsi::generic_write(dsihost, 0, [PARAM_SHIFT, 0x80]).await;
        // set ource output levels during porch and non-display area to GND
        dsi::generic_write(dsihost, 0, [0xC4, 0b11 << 4]).await;

        // LTDC settings
        // pixel clock:
        //  - must be fast enough to ensure that GRAM refresh time
        //    is shorter than display internal refresh rate
        //  - must be slow enough to avoid LTDC FIFO underrun
        // video timing:
        //  - vertical/horizontal blanking periods may be set to minumum (1)
        //  - HACT and VACT must be set in accordance with line length and count
        // ...

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
            h_sync_polarity: ltdc::PolarityActive::ActiveLow,
            v_sync_polarity: ltdc::PolarityActive::ActiveLow,
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

        // request an acknowledge after every sent command
        // default: false
        // dsihost.cmcr().modify(|w| w.set_are(false))

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
    }

    join(blink, net).await.0
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
        let _ = server.flush().await;
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
            // PLL in == 16 MHz / 8 == 2 MHz
            prediv: PllPreDiv::DIV8,
            // PLL out == 2 MHz * 64 == 128 MHz
            mul: PllMul(64),
            // SYSCLK == PLL out / divp == 128 MHz / 2 == 64 MHz
            divp: Some(PllPDiv::DIV2),
            divq: None,
            divr: None,
        });
        rcc.pllsai = Some(Pll {
            // PLL in == 16 MHz / 8 == 2 MHz
            prediv: PllPreDiv::DIV8,
            // PLL out == 2 MHz * 64 == 128 MHz
            mul: PllMul(64),
            divp: None,
            divq: None,
            // LTDC clock == PLLSAIR == PLL out / divr == 128 MHz / 4 == 32 MHz
            divr: Some(PllRDiv::DIV4),
        });
        rcc.pll_src = PllSource::HSI;
        rcc.sys = Sysclk::PLL1_P;
        // APB1 clock must not be faster than 54 MHz
        rcc.apb1_pre = APBPrescaler::DIV2;
        // AHB clock == SYSCLK = 64MHz
        rcc.ahb_pre = AHBPrescaler::DIV1;
        rcc
    };
    (config, Hertz::mhz(64), hse_freq)
}

// D0  = PC9
// D1  = PC10
// D2  = PE2
// D3  = PD13
// sck = PB2
// nss = DMA1

#[allow(unused)]
mod otm8009a {
    pub const WIDHT: u16 = 800;
    pub const HEIGHT: u16 = 480;

    pub const ID: u8 = 0x40;

    pub const HSYNC: u16 = 2;
    pub const HBP: u16 = 34;
    pub const HFP: u16 = 34;
    pub const VSYNC: u16 = 1;
    pub const VBP: u16 = 15;
    pub const VFP: u16 = 16;

    pub const FREQUENCY_DIVIDER: u16 = 2;

    mod cmd {
        use super::Format;
        use super::Orientation;

        #[repr(u8)]
        #[allow(clippy::upper_case_acronyms)]
        pub enum Dcs {
            NOP = 0x00,
            SWRESET = 0x01,
            RDDMADCTR = 0x0b, // read memory display access ctrl
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

            TEEON = 0x35,  // Tearing Effect Line On; 1 param: 'TELOM'
            IDMOFF = 0x38, // Idle mode Off
            IDMON = 0x39,  // Idle mode On

            COLMOD = 0x3A, // Interface Pixel format

            RAMWRC = 0x3C, // Memory write continue
            RAMRDC = 0x3E, // Memory read continue

            WRTESCN = 0x44, // Write Tearing Effect Scan line
            RDSCNL = 0x45,  // Read  Tearing Effect Scan line

            // CABC Management, ie, Content Adaptive, Back light Control in IC OTM8009a
            WRDISBV = 0x51,  // Write Display Brightness
            WRCTRLD = 0x53,  // Write CTRL Display
            WRCABC = 0x55,   // Write Content Adaptive Brightness
            WRCABCMB = 0x5E, // Write CABC Minimum Brightness

            ID1 = 0xDA, // Read ID1
            ID2 = 0xDB, // Read ID2
            ID3 = 0xDC, // Read ID3
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

        #[repr(u8)]
        pub enum MadctrMode {
            Portrait = 0x00,
            Landscape = 0x01,
        }

        impl TryFrom<u8> for MadctrMode {
            type Error = ();

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                Ok(match value {
                    | 0x00 => MadctrMode::Landscape,
                    | 0x01 => MadctrMode::Portrait,
                    | _ => return Err(()),
                })
            }
        }

        impl From<Orientation> for MadctrMode {
            fn from(value: Orientation) -> Self {
                match value {
                    | Orientation::Portrait => MadctrMode::Portrait,
                    | Orientation::Landscape => MadctrMode::Landscape,
                }
            }
        }

        impl From<MadctrMode> for Orientation {
            fn from(value: MadctrMode) -> Self {
                match value {
                    | MadctrMode::Portrait => Orientation::Portrait,
                    | MadctrMode::Landscape => Orientation::Landscape,
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
    }

    #[repr(u32)]
    pub enum Format {
        RGB888 = 0,
        RGB565 = 2,
    }

    impl TryFrom<u32> for Format {
        type Error = ();

        fn try_from(value: u32) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0 => Self::RGB888,
                | 2 => Self::RGB565,
                | _ => return Err(()),
            })
        }
    }

    pub enum Orientation {
        Portrait,
        Landscape,
    }

    impl From<bool> for Orientation {
        fn from(value: bool) -> Self {
            match value {
                | false => Self::Portrait,
                | true => Self::Landscape,
            }
        }
    }

    impl From<Orientation> for bool {
        fn from(value: Orientation) -> Self {
            match value {
                | Orientation::Portrait => false,
                | Orientation::Landscape => true,
            }
        }
    }
}

mod dsi {
    use core::iter;

    use embassy_futures::yield_now;
    use embassy_stm32::pac::dsihost::Dsihost;

    mod packet {
        pub enum Type {
            Short(Short),
            Long(Long),
        }

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
        transfer(dsi, channel, ty, tx).await
    }

    pub async fn dcs_write<I>(dsi: Dsihost, channel: u8, cmd: u8, tx: I)
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
        transfer(dsi, channel, ty, iter::once(cmd).chain(tx)).await
    }

    async fn transfer<I>(dsi: Dsihost, channel: u8, ty: packet::Type, tx: I)
    where
        I: IntoIterator<Item = u8>,
    {
        match ty {
            | packet::Type::Long(ty) => long_transfer(dsi, channel, ty, tx).await,
            | packet::Type::Short(ty) => {
                let mut tx = tx.into_iter();
                short_transfer(dsi, channel, ty, tx.next(), tx.next()).await
            }
        }
    }

    async fn long_transfer(
        dsi: Dsihost,
        channel: u8,
        ty: packet::Long,
        tx: impl IntoIterator<Item = u8>,
    ) {
        let mut len: u16 = 0;

        let mut bytes = tx.into_iter().inspect(|_| len += 1).array_chunks::<4>();

        wait_command_fifo_empty(dsi).await;

        for chunk in &mut bytes {
            dsi.gpdr().write_value(embassy_stm32::pac::dsihost::regs::Gpdr(
                u32::from_le_bytes(chunk),
            ));
            wait_command_fifo_empty(dsi).await;
        }

        if let Some(remainder) = bytes.into_remainder() {
            dsi.gpdr().write_value(embassy_stm32::pac::dsihost::regs::Gpdr(
                remainder.fold(0, |acc, byte| (acc << 8) | byte as u32),
            ));
            wait_command_fifo_empty(dsi).await;
        }

        dsi.ghcr().write(|w| {
            w.set_vcid(channel);
            w.set_dt(ty.into());
            let [lsb, msb] = len.to_le_bytes();
            w.set_wclsb(lsb);
            w.set_wcmsb(msb);
        });

        wait_command_fifo_empty(dsi).await;
    }

    async fn short_transfer(
        dsi: Dsihost,
        channel: u8,
        ty: packet::Short,
        p0: Option<u8>,
        p1: Option<u8>,
    ) {
        wait_command_fifo_empty(dsi).await;

        dsi.ghcr().write(|w| {
            w.set_vcid(channel);
            w.set_dt(ty.into());
            w.set_wclsb(p0.unwrap_or(0));
            w.set_wcmsb(p1.unwrap_or(0));
        });

        wait_command_fifo_empty(dsi).await;
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
        let [lsb, msb] = len.to_le_bytes();
        short_transfer(
            dsi,
            channel,
            packet::Short::SetMaxReturnPacketSize,
            Some(lsb),
            Some(msb),
        )
        .await;

        dsi.ghcr().write(|w| {
            w.set_vcid(channel);
            w.set_dt(ty.into());
            w.set_wclsb(p0.unwrap_or(0));
            w.set_wcmsb(p1.unwrap_or(0));
        });

        wait_read_not_busy(dsi).await;

        let mut bytes = dst.array_chunks_mut::<4>();
        for chunk in &mut bytes {
            wait_payload_read_fifo_not_empty(dsi).await;
            *chunk = dsi.gpdr().read().0.to_le_bytes();
        }

        let remainder = bytes.into_remainder();
        if !remainder.is_empty() {
            let gdpr = dsi.gpdr().read().0.to_le_bytes();
            remainder.copy_from_slice(&gdpr[..remainder.len()]);
        }

        wait_command_fifo_empty(dsi).await;
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
}
