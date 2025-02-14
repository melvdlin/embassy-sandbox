#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(layout_for_ptr)]

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
use embassy_stm32::peripherals;
use embassy_stm32::rcc;
use embassy_stm32::time::Hertz;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch;
use embassy_sync::watch::Watch;
use embassy_time::Duration;
use embassy_time::Timer;
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
});

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
    let memory: &'static mut [MaybeUninit<u32>] =
        unsafe { sdram::init::<SDRAM_SIZE>(sdram::create_sdram!(p)) };

    let (head, _tail) = memory.split_at_mut(4);
    let values: &[u32] = &[0x12345678, 0x87654321, 0x89ABCDEF, 0xFEDCBA98];
    for (src, dst) in values.iter().zip(head.iter_mut()) {
        dst.write(*src);
    }
    let head =
        unsafe { core::mem::transmute::<&mut [MaybeUninit<u32>], &mut [u32]>(head) };

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
            w.ettxe();
            // enable EoT packet reception
            w.etrxe();
            // enable automatic bus turnaround
            w.btae();
            // check ECC of received packets
            w.eccrxe();
            // check CRC checksum of received packets
            w.crcrxe();
        });

        // set DSI wrapper to 24 bit color
        dsihost.wcfgr().modify(|w| w.set_colmux(0b101));
        // set DSI host to use 24 bit color
        dsihost.lcolcr().modify(|w| w.set_colc(0b101));

        // configure HSYNC, VSYNC and DATA ENABLE polarity
        // to match LTDC polarity config
        // default: all active high
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
        // default: halt on falling edge
        dsihost.wcfgr().modify(|w| w.set_vspol(true));

        // configure tearing effect
        // TE effect over link
        //  => acknowledge request must be enabled
        //  && bus turnaround in PCR must be enabled
        dsihost.wcfgr().modify(|w| {
            // tearing effect over pin
            w.set_tesrc(true);
            // polarity depends on display TE pin polarity
            w.set_tepol(true);
        });
        dsihost.cmcr().modify(|w| {
            // disable TE ack request
            w.set_teare(false);
        });

        // configure refresh mode
        // auto refresh:   WCR_LTDCEN is set automatically on TE event
        // manual refresh: WCR_LTDCEN must be set by software on TE event (default)
        dsihost.wcfgr().modify(|w| w.set_ar(true));

        // LTDC settings
        // pixel clock:
        //  - must be fast enough to ensure that GRAM refresh time
        //    is shorter than display internal refresh rate
        //  - must be slow enough to avoid LTDC FIFO underrun
        // video timing:
        //  - vertical/horizontal blanking periods may be set to minumum (1)
        //  - HACT and VACT must be set in accordance with line length and count
        // ...

        // command transmission mode
        // commands may be transmitted in either high-speed or low-power
        // default: high-speed (0)
        //
        // some displays require init commands to be sent in LP mode
        // dsihost.cmcr().modify(|w| {
        //     // generic short write zero params
        //     w.set_gsw0tx(false);
        //     // generic short write one param
        //     w.set_gsw1tx(false);
        //     // generic short write two params
        //     w.set_gsw2tx(false);
        //     // generic short read zero params
        //     w.set_gsr0tx(false);
        //     // generic short read one param
        //     w.set_gsr1tx(false);
        //     // generic short read two params
        //     w.set_gsr2tx(false);
        //     // generic long write
        //     w.set_glwtx(false);
        //     // DCS short write zero params
        //     w.set_dsw0tx(false);
        //     // DCS short write one param
        //     w.set_dsw1tx(false);
        //     // DCS short read zero params
        //     w.set_dsr0tx(false);
        //     // DCS long write
        //     w.set_dlwtx(false);
        //     // maximum read packet size
        //     w.set_mrdps(false);
        // });

        // request an acknowledge after every sent command
        // default: false
        // dsihost.cmcr().modify(|w| w.set_are(false))
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
