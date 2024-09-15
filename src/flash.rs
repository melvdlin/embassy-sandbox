use bitflags::bitflags;
use embassy_stm32::mode::Async;
use embassy_stm32::qspi::enums::QspiWidth;
use embassy_stm32::qspi::{Instance, Qspi};

pub struct Device<'d, T: embassy_stm32::qspi::Instance> {
    size: u32,
    state: State,
    spi: Qspi<'d, T, Async>,
}

enum State {
    Uninit,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mode {
    SPI,
    QPI,
}

impl<'d, T: Instance> Device<'d, T> {
    async fn init(&mut self) {}
}

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct SR: u8 {
        const WIP  = 1 << 0;
        const WEL  = 1 << 1;
        const BP0  = 1 << 2;
        const BP1  = 1 << 3;
        const BP2  = 1 << 4;
        const BP3  = 1 << 5;
        const QE   = 1 << 6;
        const SRWD = 1 << 7;
        const _    = !0;
    }
}

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct CR: u8 {
        const ODS0    = 1 << 0;
        const ODS1    = 1 << 1;
        const ODS2    = 1 << 2;
        const TB      = 1 << 3;
        const PBE     = 1 << 4;
        const _4_BYTE = 1 << 5;
        const DC0     = 1 << 6;
        const DC1     = 1 << 7;
        const _       = !0;
    }
}

bitflags! {
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct SCUR: u8 {
        const OTP    = 1 << 0;
        const LDSO   = 1 << 1;
        const PSB    = 1 << 2;
        const ESB    = 1 << 3;
        const P_FAIL = 1 << 5;
        const E_FAIL = 1 << 6;
        const WPSEL  = 1 << 7;
        const _       = !0;
    }
}

impl From<Mode> for QspiWidth {
    fn from(value: Mode) -> Self {
        match value {
            | Mode::SPI => QspiWidth::SING,
            | Mode::QPI => QspiWidth::QUAD,
        }
    }
}

impl From<Mode> for usize {
    fn from(value: Mode) -> Self {
        match value {
            | Mode::SPI => 1,
            | Mode::QPI => 4,
        }
    }
}

pub mod instruction {
    pub const READ: u8 = 0x03;
    pub const FAST_READ: u8 = 0x0B;
    pub const _2READ: u8 = 0xBB;
    pub const DREAD: u8 = 0x3B;
    pub const _4READ: u8 = 0xEB;
    pub const QREAD: u8 = 0x6B;
    pub const FASTDTRD: u8 = 0x0D;
    pub const _2DTRD: u8 = 0xBD;
    pub const _4DTRD: u8 = 0xED;
    pub const PP: u8 = 0x02;
    pub const _4PP: u8 = 0x38;
    pub const SE: u8 = 0x20;
    pub const BE32K: u8 = 0x52;
    pub const BE: u8 = 0xD8;
    pub const CE: u8 = 0x60;

    pub const READ4B: u8 = 0x13;
    pub const FAST_READ4B: u8 = 0x0C;
    pub const _2READ4B: u8 = 0xBC;
    pub const DREAD4B: u8 = 0x3C;
    pub const _4READ4B: u8 = 0xEC;
    pub const QREAD4B: u8 = 0x6C;
    pub const FRDTRD4B: u8 = 0x0E;
    pub const _2DTRD4B: u8 = 0xBE;
    pub const _4DTRD4B: u8 = 0xEE;
    pub const PP4B: u8 = 0x12;
    pub const _4PP4B: u8 = 0x3E;
    pub const BE4B: u8 = 0xDC;
    pub const BE32K4B: u8 = 0x5C;
    pub const SE4B: u8 = 0x21;

    pub const WREN: u8 = 0x06;
    pub const WRDI: u8 = 0x04;
    pub const FMEN: u8 = 0x41;
    pub const RDSR: u8 = 0x05;
    pub const RDCR: u8 = 0x15;
    pub const WRSR: u8 = 0x01;
    pub const RDEAR: u8 = 0xC8;
    pub const WREAR: u8 = 0xC5;
    pub const WPSEL: u8 = 0x68;
    pub const EQIO: u8 = 0x35;
    pub const RSTQIO: u8 = 0xF5;
    pub const EN4B: u8 = 0xB7;
    pub const EX4B: u8 = 0xE9;
    pub const PGM_ERS_SUSPEND: u8 = 0xB0;
    pub const PGM_ERS_RESUME: u8 = 0x30;
    pub const DP: u8 = 0xB9;
    pub const RDP: u8 = 0xAB;
    pub const SBL: u8 = 0xC0;
    pub const RDFBR: u8 = 0x16;
    pub const WRFBR: u8 = 0x17;
    pub const ESFBR: u8 = 0x18;
    pub const RDID: u8 = 0x9F;
    pub const RES: u8 = 0xAB;
    pub const REMS: u8 = 0x90;
    pub const QPIID: u8 = 0xAF;
    pub const ENSO: u8 = 0xB1;
    pub const EXSO: u8 = 0xC1;
    pub const RDSCUR: u8 = 0x2B;
    pub const WRSCUR: u8 = 0x2F;
    pub const GBLK: u8 = 0x7E;
    pub const GBULK: u8 = 0x98;
    pub const WRLR: u8 = 0x2C;
    pub const RDLR: u8 = 0x2D;
    pub const WRPASS: u8 = 0x28;
    pub const RDPASS: u8 = 0x27;
    pub const PASSULK: u8 = 0x29;
    pub const WRSPB: u8 = 0xE3;
    pub const ESSPB: u8 = 0xE4;
    pub const RDSPB: u8 = 0xE2;
    pub const SPBLK: u8 = 0xA6;
    pub const RDSPBLK: u8 = 0xA7;
    pub const WRDPB: u8 = 0xE1;
    pub const RDDPB: u8 = 0xE0;
    pub const NOP: u8 = 0x00;
    pub const RSTEN: u8 = 0x66;
    pub const RST: u8 = 0x99;
}

// noinspection DuplicatedCode
#[allow(clippy::needless_update)]
pub mod transfer {
    use super::{instruction, Mode};
    use embassy_stm32::qspi::enums::{DummyCycles, QspiWidth};
    use embassy_stm32::qspi::TransferConfig;

    pub fn wren(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::WREN,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn wrdi(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRDI,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn fmen(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::FMEN,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn rdid() -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDID,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn rdp(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDP,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn res(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RES,
            dummy: match mode {
                | Mode::SPI => DummyCycles::_8,
                | Mode::QPI => DummyCycles::_2,
            },
            iwidth: mode.into(),
            dwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn rems(device_id_first: bool) -> TransferConfig {
        TransferConfig {
            instruction: instruction::REMS,
            address: Some(device_id_first as u32),
            iwidth: Mode::SPI.into(),
            awidth: QspiWidth::SING,
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn qpiid() -> TransferConfig {
        TransferConfig {
            instruction: instruction::QPIID,
            iwidth: Mode::QPI.into(),
            dwidth: Mode::QPI.into(),
            ..Default::default()
        }
    }

    pub fn rdsr(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDSR,
            iwidth: mode.into(),
            dwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn rdcr(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDCR,
            iwidth: mode.into(),
            dwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn wrsr(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRSR,
            iwidth: mode.into(),
            dwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn en4b(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::EN4B,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn ex4b(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::EX4B,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn read(address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::READ,
            address: Some(address),
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn fast_read(address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::FAST_READ,
            address: Some(address),
            dummy,
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn dread(address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::DREAD,
            address: Some(address),
            dummy,
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            dwidth: QspiWidth::DUAL,
            ..Default::default()
        }
    }

    pub fn _2read(address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::_2READ,
            address: Some(address),
            dummy,
            iwidth: Mode::SPI.into(),
            awidth: QspiWidth::DUAL,
            dwidth: QspiWidth::DUAL,
            ..Default::default()
        }
    }

    pub fn qread(address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::QREAD,
            address: Some(address),
            dummy,
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            dwidth: QspiWidth::QUAD,
            ..Default::default()
        }
    }

    pub fn _4read(mode: Mode, address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::_4READ,
            address: Some(address),
            dummy,
            iwidth: mode.into(),
            awidth: QspiWidth::QUAD,
            dwidth: QspiWidth::QUAD,
            ..Default::default()
        }
    }

    pub fn fastdtrd(address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::FASTDTRD,
            address: Some(address),
            dummy,
            iwidth: Mode::SPI.into(),
            awidth: QspiWidth::SING,
            dwidth: QspiWidth::SING,
            ..Default::default()
        }
    }

    pub fn _2dtrd(address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::_2DTRD,
            address: Some(address),
            dummy,
            iwidth: Mode::SPI.into(),
            awidth: QspiWidth::DUAL,
            dwidth: QspiWidth::DUAL,
            ..Default::default()
        }
    }

    pub fn _4dtrd(mode: Mode, address: u32, dummy: DummyCycles) -> TransferConfig {
        TransferConfig {
            instruction: instruction::_4DTRD,
            address: Some(address),
            dummy,
            iwidth: mode.into(),
            awidth: QspiWidth::QUAD,
            dwidth: QspiWidth::QUAD,
            ..Default::default()
        }
    }

    pub fn rdfbr() -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDFBR,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn wrfbr() -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRFBR,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn esfbr() -> TransferConfig {
        TransferConfig {
            instruction: instruction::ESFBR,
            iwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn se(mode: Mode, address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::SE,
            address: Some(address),
            iwidth: mode.into(),
            awidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn be32k(mode: Mode, address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::BE32K,
            address: Some(address),
            iwidth: mode.into(),
            awidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn be(mode: Mode, address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::BE,
            address: Some(address),
            iwidth: mode.into(),
            awidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn ce(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::CE,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn pp(mode: Mode, address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::BE,
            address: Some(address),
            iwidth: mode.into(),
            awidth: mode.into(),
            dwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn _4pp(address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::_4PP,
            address: Some(address),
            iwidth: Mode::SPI.into(),
            awidth: QspiWidth::QUAD,
            dwidth: QspiWidth::QUAD,
            ..Default::default()
        }
    }

    pub fn dp(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::DP,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn enso(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::ENSO,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn exso(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::EXSO,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn rdscur(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDSCUR,
            iwidth: mode.into(),
            dwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn wrscur(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRSCUR,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn rdlr() -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDLR,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn wrlr() -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRLR,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn spblk() -> TransferConfig {
        TransferConfig {
            instruction: instruction::SPBLK,
            iwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn rdspblk() -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDSPBLK,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn rdspb(address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDSPB,
            address: Some(address),
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn esspb() -> TransferConfig {
        TransferConfig {
            instruction: instruction::ESSPB,
            iwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn wrspb(address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRSPB,
            address: Some(address),
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn rddpb(address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDDPB,
            address: Some(address),
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn wrdpb(address: u32) -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRSPB,
            address: Some(address),
            iwidth: Mode::SPI.into(),
            awidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn gblk() -> TransferConfig {
        TransferConfig {
            instruction: instruction::GBLK,
            iwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn gbulk() -> TransferConfig {
        TransferConfig {
            instruction: instruction::GBULK,
            iwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn wrpass() -> TransferConfig {
        TransferConfig {
            instruction: instruction::WRPASS,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn rdpass() -> TransferConfig {
        TransferConfig {
            instruction: instruction::RDPASS,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn passulk() -> TransferConfig {
        TransferConfig {
            instruction: instruction::PASSULK,
            iwidth: Mode::SPI.into(),
            dwidth: Mode::SPI.into(),
            ..Default::default()
        }
    }

    pub fn pgm_ers_suspend(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::PGM_ERS_SUSPEND,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn pgm_ers_resume(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::PGM_ERS_RESUME,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn nop(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::NOP,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn rsten(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RSTEN,
            iwidth: mode.into(),
            ..Default::default()
        }
    }

    pub fn rst(mode: Mode) -> TransferConfig {
        TransferConfig {
            instruction: instruction::RST,
            iwidth: mode.into(),
            ..Default::default()
        }
    }
}
