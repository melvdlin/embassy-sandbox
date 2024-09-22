use core::cmp::max;

use embassy_stm32::{gpio, Peripheral};
use embassy_time::{block_for, Duration};
use itertools::Itertools;

#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub enum Cpol {
    _0,
    _1,
}

#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub enum Cpha {
    _0,
    _1,
}

pub struct Spi<'d> {
    min_sck_half_cycle: Duration,
    cs_high_time: Duration,
    #[allow(unused)]
    cpol: Cpol,
    cpha: Cpha,
    ncs: gpio::Output<'d>,
    sck: gpio::Output<'d>,
    mosi: gpio::Output<'d>,
    miso: gpio::Input<'d>,
}

impl<'d> Spi<'d> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_sck_freq: Duration,
        cs_high_time: Duration,
        cpol: Cpol,
        cpha: Cpha,
        ncs: impl Peripheral<P = impl gpio::Pin> + 'd,
        sck: impl Peripheral<P = impl gpio::Pin> + 'd,
        mosi: impl Peripheral<P = impl gpio::Pin> + 'd,
        miso: impl Peripheral<P = impl gpio::Pin> + 'd,
    ) -> Self {
        let ncs = gpio::Output::new(ncs, gpio::Level::High, gpio::Speed::VeryHigh);
        let sck = gpio::Output::new(sck, cpol.idle(), gpio::Speed::VeryHigh);
        let mosi = gpio::Output::new(mosi, gpio::Level::Low, gpio::Speed::VeryHigh);
        let miso = gpio::Input::new(miso, gpio::Pull::Down);
        let min_sck_half_cycle = max(Duration::from_ticks(1), max_sck_freq / 2);
        let cs_high_time = max(Duration::from_ticks(1), cs_high_time);

        Self {
            min_sck_half_cycle,
            cs_high_time,
            cpol,
            cpha,
            ncs,
            sck,
            miso,
            mosi,
        }
    }

    pub fn write(&mut self, tx: &[u8]) {
        self.transmit(tx, &mut []);
    }

    pub fn read(&mut self, rx: &mut [u8]) {
        self.transmit(&[], rx);
    }

    pub fn transmit(&mut self, tx: &[u8], rx: &mut [u8]) {
        self.ncs.toggle();
        if self.cpha == Cpha::_1 {
            block_for(self.min_sck_half_cycle);
        }

        let discard = &mut 0;
        for x in tx.iter().copied().zip_longest(rx.iter_mut()) {
            let (tx, rx) = x.or(0, discard);
            self.transmit_byte(tx, rx, self.cpha);
        }

        if self.cpha == Cpha::_0 {
            block_for(self.min_sck_half_cycle);
        }
        self.ncs.toggle();
        block_for(self.cs_high_time);
    }

    fn transmit_byte(&mut self, tx: u8, rx: &mut u8, cpha: Cpha) {
        *rx = 0;
        for bit_pos in (0..8).rev() {
            if cpha == Cpha::_1 {
                self.sck.toggle();
            }

            self.mosi.set_level(gpio::Level::from(tx >> bit_pos & 1 == 1));
            block_for(self.min_sck_half_cycle);

            self.sck.toggle();
            *rx |= (self.miso.get_level() as u8) << bit_pos;
            block_for(self.min_sck_half_cycle);

            if cpha == Cpha::_0 {
                self.sck.toggle();
            }
        }
    }
}

impl Cpol {
    pub fn idle(self) -> gpio::Level {
        match self {
            | Cpol::_0 => gpio::Level::Low,
            | Cpol::_1 => gpio::Level::High,
        }
    }
}
