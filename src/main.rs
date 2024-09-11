#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(async_closure)]
#![feature(core_intrinsics)]

use core::intrinsics::breakpoint;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::yield_now;
use embassy_net::Stack;
use embassy_stm32::eth::PacketQueue;
use embassy_stm32::{bind_interrupts, gpio, Peripheral};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use heapless::String;
#[allow(unused_imports)]
use panic_halt as _;
use rand_core::RngCore;
use smoltcp::socket::dhcpv4::RetryConfig;
use static_cell::{ConstStaticCell, StaticCell};

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
async fn net_task(stack: &'static Stack<Device>) -> ! {
    stack.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    _main(spawner).await
}

async fn _main(spawner: Spawner) -> ! {
    let p = embassy_stm32::init(config());

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
}

async fn blink(
    ld1: gpio::Output<'_, impl gpio::Pin>,
    ld2: gpio::Output<'_, impl gpio::Pin>,
) -> ! {
    let mut ld1 = ld1;
    let mut ld2 = ld2;
    loop {
        ld1.set_high();
        ld2.set_high();

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

type ETH = embassy_stm32::peripherals::ETH;
#[allow(clippy::too_many_arguments)]
async fn echo(
    spawner: Spawner,
    hostname: impl AsRef<str>,
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
        Config::dhcpv4(dhcp_config(hostname).unwrap() /*Default::default()*/);
    // Config::ipv4_static(StaticConfigV4 {
    //     address: Ipv4Cidr::new(Ipv4Address([192, 168, 2, 194]), 0),
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
        embassy_stm32::eth::generic_smi::GenericSMI::new(0),
        mac_addr,
    );

    let mut server_rx_buf = [0; 4096];
    let mut server_tx_buf = [0; 4096];
    //
    // let mut client_rx_buf = [0; 512];
    // let mut client_tx_buf = [0; 512];

    static STACK: StaticCell<Stack<Device>> = StaticCell::new();
    let stack = STACK.init(Stack::new(ethernet, net_cfg, resources, seeds[0]));

    spawner.must_spawn(net_task(stack));
    stack.wait_config_up().await;
    unsafe {
        breakpoint();
    }
    let config = (async || loop {
        if let Some(config) = stack.config_v4() {
            return config;
        }
        yield_now().await;
    })()
    .await;
    let addr = config.address.address();
    let addr = addr;
    unsafe {
        breakpoint();
    }

    let mut server = tcp::TcpSocket::new(&stack, &mut server_rx_buf, &mut server_tx_buf);
    server.set_timeout(Some(Duration::from_secs(20)));
    // let mut client = tcp::TcpSocket::new(&stack, &mut client_rx_buf, &mut client_tx_buf);
    let config_v4 = stack.config_v4();
    let config_v4 = config_v4;

    let mut server = async move || loop {
        if let Err(e) = server.accept(1234).await {
            let e = e;
            Timer::after_secs(1).await;
            continue;
        }

        let mut buf = [0; 512];
        loop {
            let len = match server.read(&mut buf).await {
                | Err(_) | Ok(0) => break,
                | Ok(len) => len,
            };

            if server.write_all(&buf[..len]).await.is_err() {
                break;
            }
        }
        let _ = server.flush().await;
    };
    //
    // let mut client = async move || loop {
    //     if let Err(err) = client
    //         .connect((Ipv4Address::new(127, 0, 0, 1), 1234))
    //         .await
    //     {
    //         unsafe {
    //             core::intrinsics::breakpoint();
    //         }
    //         Timer::after_secs(1).await;
    //         continue;
    //     }
    //     unsafe {
    //         core::intrinsics::breakpoint();
    //     }
    //     let mut buf = [0; 512];
    //     if client.write_all("1234".as_bytes()).await.is_err()
    //         || client.flush().await.is_err()
    //     {
    //         continue;
    //     }
    //
    //     loop {
    //         let len = match client.read(&mut buf).await {
    //             | Err(_) | Ok(0) => break,
    //             | Ok(len) => len,
    //         };
    //         let buf = &mut buf[..len];
    //         let buf = buf;
    //     }
    // };

    server().await
}

// noinspection ALL
fn config() -> embassy_stm32::Config {
    use embassy_stm32::rcc::*;
    let mut config = embassy_stm32::Config::default();
    config.rcc = {
        let mut rcc = Config::default();
        rcc.hsi = true;
        rcc.pll = Some(Pll {
            prediv: PllPreDiv::DIV8,
            mul: PllMul(64),
            divp: Some(PllPDiv::DIV2),
            divq: None,
            divr: None,
        });
        rcc.pll_src = PllSource::HSI;
        rcc.sys = Sysclk::PLL1_P;
        rcc.apb1_pre = APBPrescaler::DIV2;
        rcc
    };
    config
}

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
