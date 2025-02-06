use core::str::FromStr;

use embassy_executor::Spawner;
use embassy_futures::yield_now;
use embassy_stm32::eth::PacketQueue;
use embassy_stm32::eth::{self};
use embassy_stm32::interrupt;
use embassy_stm32::Peripheral;
use embassy_sync::watch;
use heapless::String;
use static_cell::ConstStaticCell;

pub type Ethernet = embassy_stm32::eth::Ethernet<
    'static,
    embassy_stm32::peripherals::ETH,
    embassy_stm32::eth::GenericPhy,
>;

#[embassy_executor::task]
pub async fn runner_task(runner: embassy_net::Runner<'static, Ethernet>) -> ! {
    let mut runner = runner;
    runner.run().await
}

pub type EthIrHandler = embassy_stm32::eth::InterruptHandler;
pub type RngIrHandler =
    embassy_stm32::rng::InterruptHandler<embassy_stm32::peripherals::RNG>;

#[allow(clippy::upper_case_acronyms)]
type ETH = embassy_stm32::peripherals::ETH;
#[allow(clippy::too_many_arguments)]
pub async fn stack_setup(
    spawner: Spawner,
    dhcp_up: &watch::DynSender<'_, ()>,
    #[allow(unused)] hostname: impl AsRef<str>,
    mac_addr: [u8; 6],
    seeds: [u64; 2],
    irq: impl interrupt::typelevel::Binding<interrupt::typelevel::ETH, eth::InterruptHandler>
        + 'static,
    eth: ETH,
    ref_clk: impl Peripheral<P = impl eth::RefClkPin<ETH>> + 'static,
    mdio: impl Peripheral<P = impl eth::MDIOPin<ETH>> + 'static,
    mdc: impl Peripheral<P = impl eth::MDCPin<ETH>> + 'static,
    crs: impl Peripheral<P = impl eth::CRSPin<ETH>> + 'static,
    rx_d0: impl Peripheral<P = impl eth::RXD0Pin<ETH>> + 'static,
    rx_d1: impl Peripheral<P = impl eth::RXD1Pin<ETH>> + 'static,
    tx_d0: impl Peripheral<P = impl eth::TXD0Pin<ETH>> + 'static,
    tx_d1: impl Peripheral<P = impl eth::TXD1Pin<ETH>> + 'static,
    tx_en: impl Peripheral<P = impl eth::TXEnPin<ETH>> + 'static,
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
        irq,
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

    spawner.must_spawn(runner_task(runner));
    stack.wait_config_up().await;
    let _config = loop {
        if let Some(config) = stack.config_v4() {
            break config;
        }
        yield_now().await;
    };
    dhcp_up.send(());

    stack
}

#[allow(unused)]
fn dhcp_config(hostname: impl AsRef<str>) -> Result<embassy_net::DhcpConfig, ()> {
    let mut config = embassy_net::DhcpConfig::default();
    config.hostname = Some(String::from_str(hostname.as_ref())?);
    config.retry_config.discover_timeout = smoltcp::time::Duration::from_secs(16);
    config.retry_config.initial_request_timeout = smoltcp::time::Duration::from_secs(16);

    Ok(config)
}
