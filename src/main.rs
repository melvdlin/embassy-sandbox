#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(core_intrinsics)]
#![feature(layout_for_ptr)]
#![allow(internal_features)]

#[allow(unused)]
use core::intrinsics::breakpoint;
use core::mem::MaybeUninit;

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::join::join3;
use embassy_net::Ipv4Address;
use embassy_sandbox::*;
use embassy_stm32::bind_interrupts;
use embassy_stm32::gpio;
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
    let (config, _ahb_freq) = config();
    let p = embassy_stm32::init(config);
    let mut _button =
        embassy_stm32::exti::ExtiInput::new(p.PA0, p.EXTI0, gpio::Pull::Down);

    // 128 Kib
    const SDRAM_SIZE: usize = (128 / 8) << 10;
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

// D0  = PC9
// D1  = PC10
// D2  = PE2
// D3  = PD13
// sck = PB2
// nss = DMA1
