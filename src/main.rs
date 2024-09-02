#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_stm32::{bind_interrupts, gpio};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use heapless::String;
#[allow(unused_imports)]
use panic_halt as _;
use rand_core::RngCore;
use static_cell::ConstStaticCell;

const HOSTNAME: &str = "STM32F7-DISCO";
// first octet: locally administered (administratively assigned) unicast address;
// see https://en.wikipedia.org/wiki/MAC_address#IEEE_802c_local_MAC_address_usage
const MAC_ADDR: [u8; 6] = [0x02, 0xC7, 0x52, 0x67, 0x83, 0xEF];

bind_interrupts!(struct Irqs {
    ETH => embassy_stm32::eth::InterruptHandler;
    RNG => embassy_stm32::rng::InterruptHandler<embassy_stm32::peripherals::RNG>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    _main(spawner).await
}

async fn _main(_spawner: Spawner) -> ! {
    let p = embassy_stm32::init(config());

    let ld1 = gpio::Output::new(p.PJ13, gpio::Level::High, gpio::Speed::Low);
    let ld2 = gpio::Output::new(p.PJ5, gpio::Level::High, gpio::Speed::Low);

    let mut rng = embassy_stm32::rng::Rng::new(p.RNG, Irqs);
    let seed = rng.next_u64();

    let blink = blink(ld1, ld2);
    let echo = echo(
        HOSTNAME, MAC_ADDR, seed, p.ETH, p.PA1, p.PA2, p.PC1, p.PA7, p.PC4, p.PC5,
        p.PG13, p.PG14, p.PG11,
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

#[allow(clippy::too_many_arguments)]
async fn echo<T: embassy_stm32::eth::Instance>(
    hostname: impl AsRef<str>,
    mac_addr: [u8; 6],
    seed: u64,
    eth: impl embassy_stm32::Peripheral<P = T>,
    ref_clk: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::RefClkPin<T>>,
    mdio: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::MDIOPin<T>>,
    mdc: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::MDCPin<T>>,
    crs: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::CRSPin<T>>,
    rx_d0: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::RXD0Pin<T>>,
    rx_d1: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::RXD1Pin<T>>,
    tx_d0: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::TXD0Pin<T>>,
    tx_d1: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::TXD1Pin<T>>,
    tx_en: impl embassy_stm32::Peripheral<P = impl embassy_stm32::eth::TXEnPin<T>>,
) -> ! {
    use embassy_net::*;
    let net_cfg = Config::dhcpv4(dhcp_config(hostname).unwrap());
    // Config::ipv4_static(StaticConfigV4 {
    //     address: Ipv4Cidr::new(Ipv4Address([192, 168, 2, 194]), 0),
    //     gateway: None,
    //     dns_servers: Default::default(),
    // });
    let mut packet_queue = embassy_stm32::eth::PacketQueue::<4, 4>::new();

    static RESOURCES: ConstStaticCell<StackResources<4>> =
        ConstStaticCell::new(StackResources::new());
    let resources = RESOURCES.take();

    let ethernet = embassy_stm32::eth::Ethernet::new(
        &mut packet_queue,
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

    let mut rx_buf = [0; 512];
    let mut tx_buf = [0; 512];

    let stack = Stack::new(ethernet, net_cfg, resources, seed);
    let mut sock = tcp::TcpSocket::new(&stack, &mut rx_buf, &mut tx_buf);
    let config_v4 = stack.config_v4();
    let config_v4 = config_v4;

    loop {
        sock.local_endpoint();
        if sock.accept(12345).await.is_err() {
            Timer::after_secs(1).await;
            continue;
        }

        let mut buf = [0; 512];
        loop {
            let len = match sock.read(&mut buf).await {
                | Err(_) | Ok(0) => break,
                | Ok(len) => len,
            };

            if sock.write_all(&buf[..len]).await.is_err() {
                break;
            }
        }
    }
}

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
    Ok(config)
}

// D0  = PC9
// D1  = PC10
// D2  = PE2
// D3  = PD13
// sck = PB2
// nss = DMA1
