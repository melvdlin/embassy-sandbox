use core::cmp::max;

use embassy_stm32::{
    gpio,
    qspi::{self, enums::AddressSize},
    Peripheral,
};
use embassy_time::{block_for, Duration};
use itertools::Itertools;

#[derive(Debug)]
#[derive(Default)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub enum Cpol {
    #[default]
    _0,
    _1,
}

#[derive(Debug)]
#[derive(Default)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub enum Cpha {
    #[default]
    _0,
    _1,
}

#[derive(Debug)]
#[derive(Default)]
#[derive(Copy, Clone)]
#[derive(Eq, PartialEq)]
pub enum Mode {
    #[default]
    Single,
    Quad,
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

pub struct QuadSpi<'d> {
    min_sck_half_cycle: Duration,
    cs_high_time: Duration,
    #[allow(unused)]
    cpol: Cpol,
    cpha: Cpha,
    ncs: gpio::Output<'d>,
    sck: gpio::Output<'d>,
    d0_mosi: gpio::Flex<'d>,
    d1_miso: gpio::Flex<'d>,
    d2_nwp: gpio::Flex<'d>,
    d3_nhold: gpio::Flex<'d>,
}

#[derive(Default)]
#[derive(Clone, Copy)]
pub struct QuadTransfer {
    pub instruction: Option<(u8, Mode)>,
    pub address: Option<(u32, Mode, qspi::enums::AddressSize)>,
    pub data: Option<Mode>,
    pub dummy_cycles: usize,
}

#[derive(Debug)]
#[derive(Eq, PartialEq)]
enum Direction<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

impl<'d> QuadSpi<'d> {
    // transmission methods suffixed by an underscore (e.g., [single_transmit_byte_]
    // do not engage or disengage chip select.

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_sck_freq: Duration,
        cs_high_time: Duration,
        cpol: Cpol,
        cpha: Cpha,
        ncs: impl Peripheral<P = impl gpio::Pin> + 'd,
        sck: impl Peripheral<P = impl gpio::Pin> + 'd,
        d0_mosi: impl Peripheral<P = impl gpio::Pin> + 'd,
        d1_miso: impl Peripheral<P = impl gpio::Pin> + 'd,
        d2_nwp: impl Peripheral<P = impl gpio::Pin> + 'd,
        d3_nhold: impl Peripheral<P = impl gpio::Pin> + 'd,
    ) -> Self {
        let ncs = gpio::Output::new(ncs, gpio::Level::High, gpio::Speed::VeryHigh);
        let sck = gpio::Output::new(sck, cpol.idle(), gpio::Speed::VeryHigh);
        let d0_mosi = gpio::Flex::new(d0_mosi);
        let d1_miso = gpio::Flex::new(d1_miso);
        let d2_nwp = gpio::Flex::new(d2_nwp);
        let d3_nhold = gpio::Flex::new(d3_nhold);

        let min_sck_half_cycle = max(Duration::from_ticks(1), max_sck_freq / 2);
        let cs_high_time = max(Duration::from_ticks(1), cs_high_time);

        let mut qspi = Self {
            min_sck_half_cycle,
            cs_high_time,
            cpol,
            cpha,
            ncs,
            sck,
            d0_mosi,
            d1_miso,
            d2_nwp,
            d3_nhold,
        };

        qspi.single_mode();

        qspi
    }

    pub fn single_read(&mut self, rx: &mut [u8]) {
        self.single_transfer(&[], rx);
    }

    pub fn single_write(&mut self, tx: &[u8]) {
        self.single_transfer(tx, &mut []);
    }

    pub fn single_transfer(&mut self, tx: &[u8], rx: &mut [u8]) {
        self.single_mode();
        self.ncs.set_low();

        if self.cpha == Cpha::_1 {
            block_for(self.min_sck_half_cycle);
        }

        self.single_transmit_(tx, rx);

        if self.cpha == Cpha::_0 {
            block_for(self.min_sck_half_cycle);
        }

        self.ncs.set_high();
        block_for(self.cs_high_time);
    }

    pub fn single_transfer_in_place(&mut self, trx: &mut [u8]) {
        self.single_mode();
        self.ncs.set_low();

        if self.cpha == Cpha::_1 {
            block_for(self.min_sck_half_cycle);
        }

        self.single_transmit_in_place_(trx);

        if self.cpha == Cpha::_0 {
            block_for(self.min_sck_half_cycle);
        }

        self.ncs.set_high();
        block_for(self.cs_high_time);
    }

    pub fn quad_read(&mut self, data: &mut [u8], transfer: &QuadTransfer) {
        self.quad_transfer(Direction::Read(data), transfer);
    }

    pub fn quad_write(&mut self, data: &[u8], transfer: &QuadTransfer) {
        self.quad_transfer(Direction::Write(data), transfer);
    }

    fn quad_transfer(&mut self, direction: Direction, transfer: &QuadTransfer) {
        self.single_mode();
        self.ncs.set_low();

        if self.cpha == Cpha::_1 {
            block_for(self.min_sck_half_cycle);
        }

        if let Some((instruction, mode)) = transfer.instruction {
            self.transfer_(Direction::Write(&[instruction]), mode);
        }

        if let Some((address, mode, size)) = transfer.address {
            let address = &address.to_be_bytes()[match size {
                | qspi::enums::AddressSize::_8Bit => 3,
                | qspi::enums::AddressSize::_16Bit => 2,
                | qspi::enums::AddressSize::_24bit => 1,
                | qspi::enums::AddressSize::_32bit => 0,
            }..];
            self.transfer_(Direction::Write(address), mode);
        }

        for _ in 0..transfer.dummy_cycles {
            self.dummy_cycle();
        }

        if let Some(mode) = transfer.data {
            self.transfer_(direction, mode);
        }

        if self.cpha == Cpha::_0 {
            block_for(self.min_sck_half_cycle);
        }

        self.ncs.set_high();
        block_for(self.cs_high_time);
    }

    fn transfer_(&mut self, direction: Direction, mode: Mode) {
        match mode {
            | Mode::Single => {
                self.single_mode();
                match direction {
                    | Direction::Read(data) => self.single_read_(data),
                    | Direction::Write(data) => self.single_write_(data),
                }
            }
            | Mode::Quad => match direction {
                | Direction::Read(data) => {
                    self.quad_read_mode();
                    self.quad_read_(data);
                }
                | Direction::Write(data) => {
                    self.quad_write_mode();
                    self.quad_write_(data);
                }
            },
        }
    }

    fn single_mode(&mut self) {
        self.d0_mosi.set_low();
        self.d0_mosi.set_as_output(gpio::Speed::VeryHigh);
        self.d1_miso.set_as_input(gpio::Pull::Down);

        self.d2_nwp.set_high();
        self.d2_nwp.set_as_output(gpio::Speed::VeryHigh);
        self.d3_nhold.set_high();
        self.d3_nhold.set_as_output(gpio::Speed::VeryHigh);
    }

    fn quad_read_mode(&mut self) {
        self.d0_mosi.set_as_input(gpio::Pull::Down);
        self.d1_miso.set_as_input(gpio::Pull::Down);
        self.d2_nwp.set_as_input(gpio::Pull::Down);
        self.d3_nhold.set_as_input(gpio::Pull::Down);
    }

    fn quad_write_mode(&mut self) {
        self.d0_mosi.set_low();
        self.d1_miso.set_low();
        self.d2_nwp.set_low();
        self.d3_nhold.set_low();
        self.d0_mosi.set_as_output(gpio::Speed::VeryHigh);
        self.d1_miso.set_as_output(gpio::Speed::VeryHigh);
        self.d2_nwp.set_as_output(gpio::Speed::VeryHigh);
        self.d3_nhold.set_as_output(gpio::Speed::VeryHigh);
    }

    fn single_read_(&mut self, rx: &mut [u8]) {
        self.single_transmit_(&[], rx);
    }

    fn single_write_(&mut self, tx: &[u8]) {
        self.single_transmit_(tx, &mut []);
    }

    fn single_transmit_(&mut self, tx: &[u8], rx: &mut [u8]) {
        let discard = &mut 0;
        for trx in tx.iter().copied().zip_longest(rx.iter_mut()) {
            let (tx, rx) = trx.or(0, discard);
            *rx = self.single_transmit_byte_(tx);
        }
    }

    fn single_transmit_in_place_(&mut self, trx: &mut [u8]) {
        for trx in trx {
            *trx = self.single_transmit_byte_(*trx);
        }
    }

    fn single_transmit_byte_(&mut self, tx: u8) -> u8 {
        let mut rx = 0;
        for bit_pos in (0..8).rev() {
            if self.cpha == Cpha::_1 {
                self.sck.toggle();
            }

            self.d0_mosi.set_level(gpio::Level::from(tx >> bit_pos & 1 == 1));
            block_for(self.min_sck_half_cycle);

            self.sck.toggle();
            rx |= (self.d1_miso.get_level() as u8) << bit_pos;
            block_for(self.min_sck_half_cycle);

            if self.cpha == Cpha::_0 {
                self.sck.toggle();
            }
        }
        rx
    }

    fn dummy_cycle(&mut self) {
        if self.cpha == Cpha::_1 {
            self.sck.toggle();
        }

        block_for(self.min_sck_half_cycle);
        self.sck.toggle();
        block_for(self.min_sck_half_cycle);

        if self.cpha == Cpha::_0 {
            self.sck.toggle();
        }
    }

    fn quad_read_(&mut self, rx: &mut [u8]) {
        for rx in rx {
            *rx = self.quad_read_byte_();
        }
    }

    fn quad_write_(&mut self, tx: &[u8]) {
        for tx in tx {
            self.quad_write_byte_(*tx);
        }
    }

    fn quad_write_byte_(&mut self, tx: u8) {
        for half in [1, 0] {
            if self.cpha == Cpha::_1 {
                self.sck.toggle();
            }

            let tx = (tx >> (half * 4)) & 0b1111;
            for (shift, pin) in [
                &mut self.d0_mosi,
                &mut self.d1_miso,
                &mut self.d2_nwp,
                &mut self.d3_nhold,
            ]
            .into_iter()
            .enumerate()
            {
                let level = gpio::Level::from((tx >> shift) & 1 == 1);
                pin.set_level(level);
            }

            block_for(self.min_sck_half_cycle);
            self.sck.toggle();
            block_for(self.min_sck_half_cycle);

            if self.cpha == Cpha::_0 {
                self.sck.toggle();
            }
        }
    }

    fn quad_read_byte_(&mut self) -> u8 {
        let mut rx = 0;
        for half in [1, 0] {
            if self.cpha == Cpha::_1 {
                self.sck.toggle();
            }

            block_for(self.min_sck_half_cycle);
            self.sck.toggle();
            block_for(self.min_sck_half_cycle);

            for (shift, pin) in [
                &mut self.d0_mosi,
                &mut self.d1_miso,
                &mut self.d2_nwp,
                &mut self.d3_nhold,
            ]
            .into_iter()
            .enumerate()
            {
                rx |= (pin.is_high() as u8) << shift;
            }
            rx <<= 4 * half;

            if self.cpha == Cpha::_0 {
                self.sck.toggle();
            }
        }
        rx
    }
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
            *rx = self.transmit_byte(tx, self.cpha);
        }

        if self.cpha == Cpha::_0 {
            block_for(self.min_sck_half_cycle);
        }
        self.ncs.toggle();
        block_for(self.cs_high_time);
    }

    fn transmit_byte(&mut self, tx: u8, cpha: Cpha) -> u8 {
        let mut rx = 0;
        for bit_pos in (0..8).rev() {
            if cpha == Cpha::_1 {
                self.sck.toggle();
            }

            self.mosi.set_level(gpio::Level::from(tx >> bit_pos & 1 == 1));
            block_for(self.min_sck_half_cycle);

            self.sck.toggle();
            rx |= (self.miso.get_level() as u8) << bit_pos;
            block_for(self.min_sck_half_cycle);

            if cpha == Cpha::_0 {
                self.sck.toggle();
            }
        }

        rx
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

impl Mode {
    pub fn from_width(width: embassy_stm32::qspi::enums::QspiWidth) -> Option<Self> {
        match width {
            | qspi::enums::QspiWidth::NONE => None,
            | qspi::enums::QspiWidth::SING => Some(Self::Single),
            | qspi::enums::QspiWidth::DUAL => panic!("dual SPI unsupported"),
            | qspi::enums::QspiWidth::QUAD => Some(Self::Quad),
        }
    }
}

impl QuadTransfer {
    pub fn from_config(
        transfer: embassy_stm32::qspi::TransferConfig,
        address_size: AddressSize,
    ) -> Self {
        Self {
            instruction: Mode::from_width(transfer.iwidth)
                .map(|mode| (transfer.instruction, mode)),
            address: transfer.address.and_then(|address| {
                Mode::from_width(transfer.awidth)
                    .map(|mode| (address, mode, address_size))
            }),
            data: Mode::from_width(transfer.dwidth),
            dummy_cycles: u8::from(transfer.dummy).into(),
        }
    }
}
