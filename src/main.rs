#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(async_closure)]
#![feature(core_intrinsics)]
#![feature(layout_for_ptr)]
#![allow(internal_features)]
#![allow(unused)]
use core::array;
use core::fmt::Write as FmtWrite;
#[allow(unused)]
use core::intrinsics::breakpoint;
use core::mem::MaybeUninit;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::yield_now;
use embassy_stm32::eth::PacketQueue;
use embassy_stm32::time::Hertz;
use embassy_stm32::{bind_interrupts, gpio, Peripheral};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Delay, Duration, Timer};
use embedded_io_async::Write as AsyncWrite;
use heapless::String;
#[allow(unused_imports)]
use panic_halt as _;
use rand_core::RngCore;
use static_cell::{ConstStaticCell, StaticCell};
use stm32_fmc::Sdram;

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
    embassy_stm32::eth::generic_smi::GenericSMI,
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

    /* SDRAM
    let memory: &'static mut [MaybeUninit<u32>] = {
        static SDRAM: StaticCell<
            Sdram<
                embassy_stm32::fmc::Fmc<'static, embassy_stm32::peripherals::FMC>,
                stm32_fmc::devices::is42s32400f_6::Is42s32400f6,
            >,
        > = StaticCell::new();
        const SDRAM_SIZE: usize = (128 / 8) << 10;
        let sdram =
            SDRAM.init(embassy_stm32::fmc::Fmc::sdram_a13bits_d32bits_4banks_bank1(
                p.FMC,
                p.PF0,
                p.PF1,
                p.PF2,
                p.PF3,
                p.PF4,
                p.PF5,
                p.PF12,
                p.PF13,
                p.PF14,
                p.PF15,
                p.PG0,
                p.PG1,
                p.PG2,
                p.PG4,
                p.PG5,
                p.PD14,
                p.PD15,
                p.PD0,
                p.PD1,
                p.PE7,
                p.PE8,
                p.PE9,
                p.PE10,
                p.PE11,
                p.PE12,
                p.PE13,
                p.PE14,
                p.PE15,
                p.PD8,
                p.PD9,
                p.PD10,
                p.PH8,
                p.PH9,
                p.PH10,
                p.PH11,
                p.PH12,
                p.PH13,
                p.PH14,
                p.PH15,
                p.PI0,
                p.PI1,
                p.PI2,
                p.PI3,
                p.PI6,
                p.PI7,
                p.PI9,
                p.PI10,
                p.PE0,
                p.PE1,
                p.PI4,
                p.PI5,
                p.PH2,
                p.PG8,
                p.PG15,
                p.PH3,
                p.PF11,
                p.PH5,
                stm32_fmc::devices::is42s32400f_6::Is42s32400f6 {},
            ));
        let ptr = sdram.init(&mut Delay);
        let ptr = ptr.cast::<MaybeUninit<u32>>();
        // Safety: pointee u32: Sized
        let size = unsafe { core::mem::size_of_val_raw(ptr) };
        let len = SDRAM_SIZE / size;
        // Safety:
        // - I sure hope `embassy_stm32::fmc::Fmc::sdram_a13bits_d32bits_4banks_bank1` returns a read/write valid pointer
        // - the source ptr does not escape this scope
        const _: () = assert!(SDRAM_SIZE <= isize::MAX as usize);
        assert!((ptr as usize).checked_add(SDRAM_SIZE).is_some());
        unsafe { core::slice::from_raw_parts_mut(ptr, len) }
    };

    let (head, tail) = memory.split_at_mut(4);
    let values: &[u32] = &[0x12345678, 0x87654321, 0x89ABCDEF, 0xFEDCBA98];
    for (src, dst) in values.iter().zip(head.iter_mut()) {
        dst.write(*src);
    }
    let head =
        unsafe { core::mem::transmute::<&mut [MaybeUninit<u32>], &mut [u32]>(head) };

    assert_eq!(head, values);
    */

    loop {
        button.wait_for_falling_edge().await;
    }

    /*
    let ld1 = gpio::Output::new(p.PJ13, gpio::Level::High, gpio::Speed::Low);
    let ld2 = gpio::Output::new(p.PJ5, gpio::Level::High, gpio::Speed::Low);

    let mut rng = embassy_stm32::rng::Rng::new(p.RNG, Irqs);
    let seeds = core::array::from_fn(|_| rng.next_u64());

    let blink = blink(ld1, ld2);
    let echo = echo(
        spawner, HOSTNAME, MAC_ADDR, seeds, p.ETH, p.PA1, p.PA2, p.PC1, p.PA7, p.PC4,
        p.PC5, p.PG13, p.PG14, p.PG11,
    );

    join(blink, echo).await.0
    */
}

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

#[allow(clippy::upper_case_acronyms)]
type ETH = embassy_stm32::peripherals::ETH;
#[allow(clippy::too_many_arguments)]
async fn echo(
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
) -> ! {
    use embassy_net::*;
    let net_cfg =
        // Config::dhcpv4(dhcp_config(hostname).unwrap() /*Default::default()*/);
    Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address([192, 168, 2, 43]), 24),
        gateway: None,
        dns_servers: Default::default(),
    });

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
        embassy_stm32::eth::generic_smi::GenericSMI::new(0),
        mac_addr,
    );

    let mut server_rx_buf = [0; 4096];
    let mut server_tx_buf = [0; 4096];

    let (stack, runner) = embassy_net::new(ethernet, net_cfg, resources, seeds[0]);

    spawner.must_spawn(net_task(runner));
    stack.wait_config_up().await;

    let config = loop {
        if let Some(config) = stack.config_v4() {
            break config;
        }
        yield_now().await;
    };
    let addr = config.address.address();
    let _addr = addr;
    DHCP_UP.signal(());

    let mut server = tcp::TcpSocket::new(stack, &mut server_rx_buf, &mut server_tx_buf);
    server.set_timeout(Some(Duration::from_secs(120)));
    let config_v4 = stack.config_v4();
    let _config_v4 = config_v4;

    let mut server = async move || loop {
        if let Err(e) = server.accept(1234).await {
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

            for n in buf {
                fmt.write_fmt(format_args!("{:02x}", n))
                    .expect("fmt buffer should fit entire formatted input");
            }
            fmt.write_str("\r\n")
                .expect("fmt buffer should fit formatted input plus crlf");

            if server.write_all(fmt.as_bytes()).await.is_err() {
                break;
            }
            fmt.clear();
        }
        server.close();
        let _ = server.flush().await;
    };

    server().await
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
