#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(core_intrinsics)]
#![feature(layout_for_ptr)]
#![allow(internal_features)]
#![allow(unused)]
use core::array;
use core::fmt::Display;
use core::fmt::Write as FmtWrite;
#[allow(unused)]
use core::intrinsics::breakpoint;
use core::mem::MaybeUninit;
use core::str::FromStr;

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::join::join3;
use embassy_futures::yield_now;
use embassy_net::tcp;
use embassy_net::tcp::TcpSocket;
use embassy_net::IpEndpoint;
use embassy_net::Ipv4Address;
use embassy_sandbox::cli;
use embassy_sandbox::log;
use embassy_sandbox::util::ByteSliceExt;
use embassy_stm32::bind_interrupts;
use embassy_stm32::eth::PacketQueue;
use embassy_stm32::gpio;
use embassy_stm32::time::Hertz;
use embassy_stm32::Peripheral;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::Delay;
use embassy_time::Duration;
use embassy_time::Timer;
use embedded_hal_async::delay::DelayNs;
use embedded_io_async::Write as AsyncWrite;
use getargs::Options;
use heapless::format;
use heapless::String;
use heapless::Vec;
#[allow(unused_imports)]
use panic_halt as _;
use rand_core::RngCore;
use scuffed_write::async_writeln;
use static_cell::ConstStaticCell;
use static_cell::StaticCell;

const HOSTNAME: &str = "STM32F7-DISCO";
// first octet: locally administered (administratively assigned) unicast address;
// see https://en.wikipedia.org/wiki/MAC_address#IEEE_802c_local_MAC_address_usage
const MAC_ADDR: [u8; 6] = [0x02, 0xC7, 0x52, 0x67, 0x83, 0xEF];

bind_interrupts!(struct Irqs {
    ETH => embassy_stm32::eth::InterruptHandler;
    RNG => embassy_stm32::rng::InterruptHandler<embassy_stm32::peripherals::RNG>;
});

type Device = embassy_stm32::eth::Ethernet<
    'static,
    embassy_stm32::peripherals::ETH,
    embassy_stm32::eth::GenericPhy,
>;

#[embassy_executor::task]
async fn net_task(runner: embassy_net::Runner<'static, Device>) -> ! {
    let mut runner = runner;
    runner.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    _main(spawner).await
}

static DHCP_UP: Signal<ThreadModeRawMutex, ()> = Signal::new();

async fn _main(spawner: Spawner) -> ! {
    let (config, ahb_freq) = config();
    let p = embassy_stm32::init(config);
    let mut button =
        embassy_stm32::exti::ExtiInput::new(p.PA0, p.EXTI0, gpio::Pull::Down);

    // 128 Kib
    const SDRAM_SIZE: usize = (128 / 8) << 10;
    let memory: &'static mut [MaybeUninit<u32>] =
        unsafe { sdram_init::<SDRAM_SIZE>(create_sdram!(p)) };

    let (head, tail) = memory.split_at_mut(4);
    let values: &[u32] = &[0x12345678, 0x87654321, 0x89ABCDEF, 0xFEDCBA98];
    for (src, dst) in values.iter().zip(head.iter_mut()) {
        dst.write(*src);
    }
    let head =
        unsafe { core::mem::transmute::<&mut [MaybeUninit<u32>], &mut [u32]>(head) };

    assert_eq!(head, values);

    let ld1 = gpio::Output::new(p.PJ13, gpio::Level::High, gpio::Speed::Low);
    let ld2 = gpio::Output::new(p.PJ5, gpio::Level::High, gpio::Speed::Low);

    let mut rng = embassy_stm32::rng::Rng::new(p.RNG, Irqs);
    let seeds = core::array::from_fn(|_| rng.next_u64());

    let blink = blink(ld1, ld2);

    let net = async {
        let stack = net_stack_setup(
            spawner, HOSTNAME, MAC_ADDR, seeds, p.ETH, p.PA1, p.PA2, p.PC1, p.PA7, p.PC4,
            p.PC5, p.PG13, p.PG14, p.PG11,
        )
        .await;

        static LOG_CHANNEL: log::Channel<ThreadModeRawMutex, 1024> = log::Channel::new();
        static LOG_UP: Signal<ThreadModeRawMutex, bool> = Signal::new();

        let log_endpoint = (Ipv4Address::from([192, 168, 2, 161]), 1234);
        let log = log::log_task(log_endpoint, &DHCP_UP, &LOG_CHANNEL, &LOG_UP, stack);
        let echo = echo(1234, &LOG_CHANNEL, stack);
        let cli = cli::cli_task(4321, &LOG_CHANNEL, &DHCP_UP, stack);
        join3(log, echo, cli).await
    };

    join(blink, net).await.0
}

type Sdram = stm32_fmc::Sdram<
    embassy_stm32::fmc::Fmc<'static, embassy_stm32::peripherals::FMC>,
    stm32_fmc::devices::is42s32400f_6::Is42s32400f6,
>;

/// Safety: SIZE must be at most the SDRAM size in bytes
unsafe fn sdram_init<const SIZE: usize>(sdram: Sdram) -> &'static mut [MaybeUninit<u32>] {
    static SDRAM: StaticCell<Sdram> = StaticCell::new();
    let sdram = SDRAM.init(sdram);

    let ptr = sdram.init(&mut Delay);
    let ptr = ptr.cast::<MaybeUninit<u32>>();
    // Safety: pointee u32: Sized
    let size = unsafe { core::mem::size_of_val_raw(ptr) };
    let len = SIZE / size;
    // Safety:
    // - it is assumed that `embassy_stm32::fmc::Fmc::sdram_a13bits_d32bits_4banks_bank1`
    //   returns a read/write valid pointer
    // - the source ptr does not escape this scope
    assert!(SIZE <= isize::MAX as usize);
    assert!(ptr.wrapping_add(len) >= ptr);
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}

macro_rules! create_sdram {
    ($peripherals:ident) => {
        embassy_stm32::fmc::Fmc::sdram_a13bits_d32bits_4banks_bank1(
            $peripherals.FMC,
            $peripherals.PF0,
            $peripherals.PF1,
            $peripherals.PF2,
            $peripherals.PF3,
            $peripherals.PF4,
            $peripherals.PF5,
            $peripherals.PF12,
            $peripherals.PF13,
            $peripherals.PF14,
            $peripherals.PF15,
            $peripherals.PG0,
            $peripherals.PG1,
            $peripherals.PG2,
            $peripherals.PG4,
            $peripherals.PG5,
            $peripherals.PD14,
            $peripherals.PD15,
            $peripherals.PD0,
            $peripherals.PD1,
            $peripherals.PE7,
            $peripherals.PE8,
            $peripherals.PE9,
            $peripherals.PE10,
            $peripherals.PE11,
            $peripherals.PE12,
            $peripherals.PE13,
            $peripherals.PE14,
            $peripherals.PE15,
            $peripherals.PD8,
            $peripherals.PD9,
            $peripherals.PD10,
            $peripherals.PH8,
            $peripherals.PH9,
            $peripherals.PH10,
            $peripherals.PH11,
            $peripherals.PH12,
            $peripherals.PH13,
            $peripherals.PH14,
            $peripherals.PH15,
            $peripherals.PI0,
            $peripherals.PI1,
            $peripherals.PI2,
            $peripherals.PI3,
            $peripherals.PI6,
            $peripherals.PI7,
            $peripherals.PI9,
            $peripherals.PI10,
            $peripherals.PE0,
            $peripherals.PE1,
            $peripherals.PI4,
            $peripherals.PI5,
            $peripherals.PH2,
            $peripherals.PG8,
            $peripherals.PG15,
            $peripherals.PH3,
            $peripherals.PF11,
            $peripherals.PH5,
            stm32_fmc::devices::is42s32400f_6::Is42s32400f6 {},
        )
    };
}
pub(crate) use create_sdram;

async fn blink(ld1: gpio::Output<'_>, ld2: gpio::Output<'_>) -> ! {
    let mut ld1 = ld1;
    let mut ld2 = ld2;
    loop {
        ld1.set_high();
        if DHCP_UP.signaled() {
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
    log: &log::Channel<M, N>,
    stack: embassy_net::Stack<'_>,
) -> !
where
    M: RawMutex,
{
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
            writeln!(fmt, "{}", buf.len());
            if server.write_all(fmt.as_bytes()).await.is_err() {
                break;
            }
            fmt.clear();
        }
        server.close();
        let _ = server.flush().await;
    }
}

#[allow(clippy::upper_case_acronyms)]
type ETH = embassy_stm32::peripherals::ETH;
#[allow(clippy::too_many_arguments)]
async fn net_stack_setup(
    spawner: Spawner,
    #[allow(unused)] hostname: impl AsRef<str>,
    mac_addr: [u8; 6],
    seeds: [u64; 2],
    eth: ETH,
    ref_clk: impl Peripheral<P = impl embassy_stm32::eth::RefClkPin<ETH>> + 'static,
    mdio: impl Peripheral<P = impl embassy_stm32::eth::MDIOPin<ETH>> + 'static,
    mdc: impl Peripheral<P = impl embassy_stm32::eth::MDCPin<ETH>> + 'static,
    crs: impl Peripheral<P = impl embassy_stm32::eth::CRSPin<ETH>> + 'static,
    rx_d0: impl Peripheral<P = impl embassy_stm32::eth::RXD0Pin<ETH>> + 'static,
    rx_d1: impl Peripheral<P = impl embassy_stm32::eth::RXD1Pin<ETH>> + 'static,
    tx_d0: impl Peripheral<P = impl embassy_stm32::eth::TXD0Pin<ETH>> + 'static,
    tx_d1: impl Peripheral<P = impl embassy_stm32::eth::TXD1Pin<ETH>> + 'static,
    tx_en: impl Peripheral<P = impl embassy_stm32::eth::TXEnPin<ETH>> + 'static,
) -> embassy_net::Stack<'static> {
    use embassy_net::*;
    let net_cfg =
        Config::dhcpv4(dhcp_config(hostname).unwrap() /*Default::default()*/);
    // Config::ipv4_static(StaticConfigV4 {
    //     address: Ipv4Cidr::new(Ipv4Address([192, 168, 2, 43]), 24),
    //     gateway: None,
    //     dns_servers: Default::default(),
    // });

    static PACKET_QUEUE: ConstStaticCell<PacketQueue<8, 8>> =
        ConstStaticCell::new(PacketQueue::new());
    let packet_queue = PACKET_QUEUE.take();

    static RESOURCES: ConstStaticCell<StackResources<8>> =
        ConstStaticCell::new(StackResources::new());
    let resources = RESOURCES.take();

    let ethernet = embassy_stm32::eth::Ethernet::new(
        packet_queue,
        eth,
        Irqs,
        ref_clk,
        mdio,
        mdc,
        crs,
        rx_d0,
        rx_d1,
        tx_d0,
        tx_d1,
        tx_en,
        embassy_stm32::eth::GenericPhy::new(0),
        mac_addr,
    );

    let (stack, runner) = embassy_net::new(ethernet, net_cfg, resources, seeds[0]);

    spawner.must_spawn(net_task(runner));
    stack.wait_config_up().await;
    let config = loop {
        if let Some(config) = stack.config_v4() {
            break config;
        }
        yield_now().await;
    };
    DHCP_UP.signal(());

    stack
}

// noinspection ALL
fn config() -> (embassy_stm32::Config, Hertz) {
    use embassy_stm32::rcc::*;
    let mut config = embassy_stm32::Config::default();
    config.rcc = {
        let mut rcc = Config::default();
        // HSI == 16 MHz
        rcc.hsi = true;
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
    (config, Hertz(64_000_000))
}

#[allow(unused)]
fn dhcp_config(hostname: impl AsRef<str>) -> Result<embassy_net::DhcpConfig, ()> {
    let mut config = embassy_net::DhcpConfig::default();
    config.hostname = Some(String::from_str(hostname.as_ref())?);
    config.retry_config.discover_timeout = smoltcp::time::Duration::from_secs(16);
    config.retry_config.initial_request_timeout = smoltcp::time::Duration::from_secs(16);

    Ok(config)
}

// D0  = PC9
// D1  = PC10
// D2  = PE2
// D3  = PD13
// sck = PB2
// nss = DMA1
