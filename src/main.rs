#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_time::Timer;
#[allow(unused_imports)]
use panic_halt as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    _main(spawner).await
}

async fn _main(_spawner: Spawner) -> ! {
    let p = embassy_stm32::init(Default::default());
    let mut ld1 = Output::new(p.PJ13, Level::High, Speed::Low);
    let mut ld2 = Output::new(p.PJ5, Level::High, Speed::Low);

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
