#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(maybe_uninit_fill)]
#![feature(iter_array_chunks)]
#![feature(array_chunks)]
#![feature(breakpoint)]

#[allow(unused_imports)]
use core::arch::breakpoint;
use core::num::NonZeroU16;
use core::panic::PanicInfo;
use core::sync::atomic;
use core::sync::atomic::Ordering;

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::join::join3;
use embassy_net::Ipv4Address;
use embassy_sandbox::graphics::accelerated::Backing;
use embassy_sandbox::graphics::color::Argb8888;
use embassy_sandbox::graphics::display;
use embassy_sandbox::graphics::display::LayerConfig;
use embassy_sandbox::graphics::gui::Accelerated;
use embassy_sandbox::graphics::gui::Drawable;
use embassy_sandbox::graphics::gui::ext::AcceleratedExt;
use embassy_sandbox::graphics::gui::text::font;
use embassy_sandbox::graphics::gui::text::textbox;
use embassy_sandbox::graphics::gui::text::textbox::TextBox;
use embassy_sandbox::util::typelevel;
use embassy_sandbox::*;
use embassy_stm32::bind_interrupts;
#[allow(unused_imports)]
use embassy_stm32::dsihost::DsiHost;
use embassy_stm32::gpio;
use embassy_stm32::time::Hertz;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::watch;
use embassy_sync::watch::Watch;
use embassy_time::Duration;
use embassy_time::Timer;
use embedded_graphics::geometry::AnchorPoint;
use embedded_graphics::prelude::Dimensions;
use embedded_graphics::prelude::Point;
use embedded_graphics::primitives::Rectangle;
use rand_core::RngCore;

#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        breakpoint();
        atomic::compiler_fence(Ordering::SeqCst);
    }
}

const HOSTNAME: &str = "STM32F7-DISCO";
// first octet: locally administered (administratively assigned) unicast address;
// see https://en.wikipedia.org/wiki/MAC_address#IEEE_802c_local_MAC_address_usage
const MAC_ADDR: [u8; 6] = [0x02, 0xC7, 0x52, 0x67, 0x83, 0xEF];

bind_interrupts!(struct Irqs {
    ETH => net::EthIrHandler;
    RNG => net::RngIrHandler;
    DSI => display::DSIInterruptHandler;
    DMA2D => display::Dma2dInterruptHandler;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    _main(spawner).await
}

async fn _main(spawner: Spawner) -> ! {
    let (config, _ahb, hse, ltdc_clock) = config();
    let p = embassy_stm32::init(config);
    let mut _button =
        embassy_stm32::exti::ExtiInput::new(p.PA0, p.EXTI0, gpio::Pull::Down);
    let lcd_reset_pin = gpio::Output::new(p.PJ15, gpio::Level::High, gpio::Speed::High);

    // 128 Mib
    const SDRAM_SIZE: usize = (128 / 8) << 20;
    let memory: &'static mut [u32] =
        unsafe { sdram::init::<SDRAM_SIZE>(sdram::create_sdram!(p)) };
    let (head, _tail) = memory.split_at_mut(4);
    let values: &[u32] = &[0x12, 0x21, 0xEF, 0xFE];
    for (src, dst) in values.iter().zip(head.iter_mut()) {
        *dst = *src;
    }

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

    let mut dma2d = display::dma2d::Dma2d::init(p.DMA2D, Irqs);

    const PIXELS: usize = display::WIDTH as usize * display::HEIGHT as usize;
    let (layer0, tail) = memory.split_at_mut(PIXELS);
    let (layer1, _tail) = tail.split_at_mut(PIXELS);
    let layer0: &'static mut [Argb8888] = bytemuck::must_cast_slice_mut(layer0);
    let layer1: &'static mut [Argb8888] = bytemuck::must_cast_slice_mut(layer1);
    let display_config = display::Config {
        framerate: display::FrameRateHz::_65,
        orientation: display::Orientation::Landscape,
        color_map: display::ColorMap::Rgb,
        rows: NonZeroU16::new(display::HEIGHT).expect("height must be nonzero"),
        cols: NonZeroU16::new(display::WIDTH).expect("width must be nonzero"),
    };

    let mut buf = [layer0, layer1];
    let buf_ptr = buf.each_mut().map(|buf| *buf as *mut _ as *const _);
    let layer0_cfg = LayerConfig {
        framebuffer: buf_ptr[0],
        x_offset: 0,
        y_offset: 0,
        width: display_config.cols.get(),
        height: display_config.rows.get(),
        pixel_format: embassy_stm32::ltdc::PixelFormat::ARGB8888,
        alpha: 0xFF,
        default_color: Argb8888::from_u32(0x00000000),
    };
    let layer_cfg = [
        layer0_cfg,
        LayerConfig {
            framebuffer: buf_ptr[1],
            ..layer0_cfg
        },
    ];
    let (mut disp, typelevel::Some(layer_0), typelevel::Some(_layer_1)) =
        display::Display::init(
            p.DSIHOST,
            p.LTDC,
            &display_config,
            typelevel::Some(&layer_cfg[0]),
            typelevel::Some(&layer_cfg[1]),
            hse,
            ltdc_clock,
            lcd_reset_pin,
            p.PJ2,
            &mut _button,
        )
        .await;

    let rows = display::HEIGHT as usize;
    let cols = display::WIDTH as usize;
    let mut backing = buf.map(|buf| Backing::new(buf, cols as u16, rows as u16));
    let bounds = backing[0].bounding_box();
    let mut buf = backing[0].with_dma(&mut dma2d);

    disp.enable_layer(layer_0, true);

    buf.fill_rect(&bounds, Argb8888(0xFF7F0057)).await;

    if false {
        Timer::after_secs(1).await;
        disp.enable_layer(layer_0, false);
        Timer::after_secs(1).await;
        disp.enable_layer(layer_0, true);
        Timer::after_secs(1).await;
        disp.enable(false).await;
        Timer::after_secs(1).await;
        disp.enable(true).await;
        Timer::after_secs(1).await;
        disp.sleep(true).await;
        Timer::after_secs(1).await;
        disp.sleep(false).await;
        Timer::after_secs(1).await;
        disp.set_brightness(0x00).await;
        Timer::after_secs(1).await;
        disp.set_brightness(0xFF).await;
    }

    buf.fill_rect(
        &bounds.resized(bounds.size / 2, AnchorPoint::Center),
        Argb8888::from_u32(0xFF660033),
    )
    .await;

    Timer::after_secs(1).await;

    let mut s = heapless::String::<64>::new();
    s.push_str("Hello, world!").unwrap();
    let mut text = TextBox {
        content: s,
        color: Argb8888(0xff326ba6),
        layout: textbox::Layout {
            char_map: &font::FIRA_MONO_40,
            cols: 16,
            rows: 3,
        },
        layer: 1,
        line_break_aware: true,
    };
    let mut translated = buf.translated(Point {
        x: cols as i32 / 4,
        y: rows as i32 / 4,
    });

    text.draw(&mut translated, 1).await;

    text.content.clear();
    text.content.push_str("lorem ipsum dolor sit amet").unwrap();

    Timer::after_secs(1).await;
    translated
        .fill_rect(
            &Rectangle::new(Point::zero(), bounds.size / 2),
            Argb8888::from_u32(0xFF660033),
        )
        .await;
    text.draw(&mut translated, 1).await;

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
        _ = server.flush().await;
    }
}

// noinspection ALL
fn config() -> (embassy_stm32::Config, Hertz, Hertz, Hertz) {
    use embassy_stm32::rcc::*;
    let mut config = embassy_stm32::Config::default();
    let hse_freq = Hertz::mhz(25);
    let ltdc_clock_mul = 280; // original: 384
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
            // PLL out == 1 MHz * 384 == 384 MHz
            mul: PllMul(ltdc_clock_mul),
            divp: None,
            divq: None,
            // LTDC clock == PLLSAIR / 2
            //            == PLL out / divr / 2
            //            == 280 MHz / 7 / 2
            //            == 20 MHz
            divr: Some(PllRDiv::DIV7),
        });
        rcc.pll_src = PllSource::HSE;
        rcc.sys = Sysclk::PLL1_P;
        // APB1 clock must not be faster than 54 MHz
        rcc.apb1_pre = APBPrescaler::DIV4;
        // AHB clock == SYSCLK / 2 = 108MHz
        rcc.ahb_pre = AHBPrescaler::DIV2;
        rcc
    };
    (
        config,
        Hertz::mhz(216),
        hse_freq,
        Hertz(ltdc_clock_mul as u32 * 1_000_000 / 7 / 2),
    )
}

// D0  = PC9
// D1  = PC10
// D2  = PE2
// D3  = PD13
// sck = PB2
// nss = DMA1
