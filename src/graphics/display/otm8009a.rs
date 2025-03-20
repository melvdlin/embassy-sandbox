#![allow(dead_code)]
#[allow(unused_imports)]
use core::arch::breakpoint;
use core::num::NonZeroU16;

use embassy_stm32::gpio::Output;
use embassy_time::Timer;

use super::dsi;

pub const WIDTH: u16 = 800;
pub const HEIGHT: u16 = 480;

pub const ID: u8 = 0x40;

pub const HSYNC: u16 = 2;
pub const HBP: u16 = 34;
pub const HFP: u16 = 34;
pub const VSYNC: u16 = 1;
pub const VBP: u16 = 15;
pub const VFP: u16 = 16;

pub const FREQUENCY_DIVIDER: u16 = 2;

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[repr(u8)]
pub enum FrameRateHz {
    _35 = 0,
    _40 = 1,
    _45 = 2,
    _50 = 3,
    _55 = 4,
    _60 = 5,
    _65 = 6,
    _70 = 7,
}

pub const fn ltdc_video_config(
    rows: NonZeroU16,
    cols: NonZeroU16,
    orientation: Orientation,
) -> embassy_stm32::ltdc::LtdcConfiguration {
    match orientation {
        | Orientation::Portrait => embassy_stm32::ltdc::LtdcConfiguration {
            active_width: cols.get(),
            active_height: rows.get(),
            h_back_porch: 34,
            h_front_porch: 34,
            v_back_porch: 15,
            v_front_porch: 16,
            h_sync: 2,
            v_sync: 1,
            h_sync_polarity: embassy_stm32::ltdc::PolarityActive::ActiveHigh,
            v_sync_polarity: embassy_stm32::ltdc::PolarityActive::ActiveHigh,
            data_enable_polarity: embassy_stm32::ltdc::PolarityActive::ActiveLow,
            pixel_clock_polarity: embassy_stm32::ltdc::PolarityEdge::RisingEdge,
        },
        | Orientation::Landscape => embassy_stm32::ltdc::LtdcConfiguration {
            active_width: cols.get(),
            active_height: rows.get(),
            h_back_porch: 15,
            h_front_porch: 16,
            v_back_porch: 34,
            v_front_porch: 34,
            h_sync: 1,
            v_sync: 2,
            h_sync_polarity: embassy_stm32::ltdc::PolarityActive::ActiveHigh,
            v_sync_polarity: embassy_stm32::ltdc::PolarityActive::ActiveHigh,
            data_enable_polarity: embassy_stm32::ltdc::PolarityActive::ActiveLow,
            pixel_clock_polarity: embassy_stm32::ltdc::PolarityEdge::RisingEdge,
        },
    }
}

pub async fn reset(pin: &mut Output<'_>) {
    // reset active low
    pin.set_low();
    Timer::after_millis(20).await;
    pin.set_high();
    Timer::after_millis(10).await;
}

pub async fn init(dsi: &mut dsi::Dsi<'_>, config: Config) {
    let transactions = &dsi::TRANSACTIONS;
    let _transactions = transactions;

    async fn write_reg(dsi: &mut dsi::Dsi<'_>, addr: u16, data: &[u8]) {
        let [base, offset] = addr.to_be_bytes();
        dsi.dcs_write(0, 0x00, [offset]).await;
        dsi.dcs_write(0, base, data.iter().copied()).await;
    }

    async fn read_reg(dsi: &mut dsi::Dsi<'_>, addr: u16, dst: &mut [u8]) {
        let [base, offset] = addr.to_be_bytes();
        dsi.dcs_write(0, 0x00, [offset]).await;
        dsi.dcs_read(0, base, dst).await;
    }

    let mut id = [0; 3];
    let [id1, id2, id3] = &mut id;
    dsi.dcs_read(0, 0xda, core::slice::from_mut(id1)).await;
    dsi.dcs_read(0, 0xdb, core::slice::from_mut(id2)).await;
    dsi.dcs_read(0, 0xdc, core::slice::from_mut(id3)).await;
    assert_eq!(id, [0x40, 0x00, 0x00]);

    // let mut id_two = [0; 3];
    // dsi::generic_read(dsihost, 0, &[0x04], &mut id_two).await;
    // assert_eq!(id_two, [0x40, 0x00, 0x00]);

    // button.wait_for_rising_edge().await;

    // enable command 2 (Manufacturer Command Set); enable param shift
    // address: 0xFF
    // params:  0x08, 0x09: MCS
    //          0x01:       EXTC (enable param shift)
    // enable MCS access
    // enable orise command 2 access
    write_reg(dsi, 0xff00, &[0x80, 0x09, 0x01]).await;
    write_reg(dsi, 0xff80, &[0x80, 0x09]).await;

    // set source output levels during porch and non-display area to GND
    write_reg(dsi, 0xc480, &[0b11 << 4]).await;
    Timer::after_millis(10).await;

    // register not documented
    write_reg(dsi, 0xc48a, &[0b100 << 4]).await;
    Timer::after_millis(10).await;

    // enable VCOM test mode (gvdd_en_test)
    // default: 0xa8
    write_reg(dsi, 0xc5b1, &[0xa9]).await;

    // set pump 4 and 5 VGH to 13V and -9V, respectively
    write_reg(dsi, 0xc591, &[0x34]).await;

    // enable column inversion
    write_reg(dsi, 0xc0b4, &[0x50]).await;

    // set VCOM to -1.2625 V
    write_reg(dsi, 0xd900, &[0x4e]).await;

    // set idle and normal framerate
    let framerate = config.framerate as u8;
    write_reg(dsi, 0xc181, &[framerate | framerate << 4]).await;

    // set RGB video mode VSync source to external;
    // HSync, Data Enable and clock to internal
    write_reg(dsi, 0xc1a1, &[0x08]).await;

    // set pump 4 and 5 to x6 => VGH = 6 * VDD and VGL = -6 * VDD
    write_reg(dsi, 0xc592, &[0x01]).await;

    // set pump 4 (VGH/VGL) clock freq to from line to 1/2 line
    write_reg(dsi, 0xc595, &[0x34]).await;

    // set GVDD/NGVDD from +- 5V to +- 4.625V
    write_reg(dsi, 0xd800, &[0x79, 0x79]).await;

    // set pump 1 clock freq to line (default)
    write_reg(dsi, 0xc594, &[0x33]).await;

    // set Source Driver Pull Low phase to 0x1b + 1 MCLK cycles
    write_reg(dsi, 0xc0a3, &[0x1b]).await;

    // enable flying Cap23, Cap24 and Cap32
    write_reg(dsi, 0xc582, &[0x83]).await;

    // set bias current of source OP to 1.2µA
    write_reg(dsi, 0xc481, &[0x83]).await;

    // set RGB video mode VSync, HSync and Data Enable sources to external;
    // clock to internal
    write_reg(dsi, 0xc1a1, &[0x0e]).await;

    // set panel type to normal
    write_reg(dsi, 0xb3a6, &[0x00, 0x01]).await;

    // GOA VST:     (reference point is end of back porch; unit = lines)
    // - tcon_goa_vst 1 shift: rising edge 5 cycles before reference point
    // - tcon_goa_vst 1 pulse width: 1 + 1 cyles
    // - tcon_goa_vst 1 tchop: delay rising edge by 0 cycles
    // - tcon_goa_vst 2 shift: rising edge 4 cycles before reference point
    // - tcon_goa_vst 2 pulse width: 1 + 1 cyles
    // - tcon_goa_vst 2 tchop: delay rising edge by 0 cycles
    write_reg(dsi, 0xce80, &[0x85, 0x01, 0x00, 0x84, 0x01, 0x00]).await;

    // GOA CLK A1:  (reference point is end of back porch)
    // - width: period = 2 * (1 + 1) units
    // - shift: rising edge 4 units before reference point
    // - switch: clock ends 825 (0x339) units after reference point
    // - extend: don't extend pulse
    // - tchop: delay rising edge by 0 units
    // - tglue: delay falling edge by 0 units
    // GOA CLK A2:
    // - period: 2 * (1 + 1) units
    // - shift: rising edge 3 units before reference point
    // - switch: clock ends 826 (0x33a) units after reference point
    // - extend: don't extend pulse
    // - tchop: delay rising edge by 0 units
    // - tglue: delay falling edge by 0 units
    write_reg(
        dsi,
        0xcea0,
        &[
            0x18, 0x04, 0x03, 0x39, 0x00, 0x00, 0x00, //
            0x18, 0x03, 0x03, 0x3A, 0x00, 0x00, 0x00,
        ],
    )
    .await;

    // GOA CLK A2:  (reference point is end of back porch; unit = lines)
    // - width: period = 2 * (1 + 1) units
    // - shift: rising edge 2 units before reference point
    // - switch: clock ends 827 (0x33b) units after reference point
    // - extend: don't extend pulse
    // - tchop: delay rising edge by 0 units
    // - tglue: delay falling edge by 0 units
    // GOA CLK A2:
    // - period: 2 * (1 + 1) units
    // - shift: rising edge 1 units before reference point
    // - switch: clock ends 828 (0x33c) units after reference point
    // - extend: don't extend pulse
    // - tchop: delay rising edge by 0 units
    // - tglue: delay falling edge by 0 units
    write_reg(
        dsi,
        0xceb0,
        &[
            0x18, 0x02, 0x03, 0x3B, 0x00, 0x00, 0x00, //
            0x18, 0x01, 0x03, 0x3C, 0x00, 0x00, 0x00,
        ],
    )
    .await;

    // GOA ECLK:    (unit = frames)
    // - normal  mode width: period = 2 * (1 + 1) units
    // - partial mode width: period = 2 * (1 + 1) units
    // - normal  mode tchop: rising edge delay = 32 (0x20) units
    // - partial mode tchop: rising edge delay = 32 (0x20) units
    // - eclk 1-4 follow: no effect because width > 0
    // - output level = tcon_goa_dir2
    // - set tcon_goa_clkx to toggle continuously until frame boundary + 1 line
    // - duty cycle = 50%
    // - 0 VSS lines before VGH
    // - pre-charge to GND period = 0
    write_reg(
        dsi,
        0xcfc0,
        &[0x01, 0x01, 0x20, 0x20, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00],
    )
    .await;

    // register not documented
    write_reg(dsi, 0xcfd0, &[0x00]).await;

    // GOA PAD output level during sleep = VGL
    write_reg(dsi, 0xcb80, &[0x00; 10]).await;
    // GOA PAD L output level = VGL
    write_reg(dsi, 0xcb90, &[0x00; 15]).await;
    // write_reg(dsihost, 0xcba0, &[0x00; 15]).await;
    write_reg(dsi, 0xcba0, &[0x00; 15]).await;
    write_reg(dsi, 0xcbb0, &[0x00; 10]).await;
    // write_reg(dsihost, 0xcbb0, &[0x00; 10]).await;
    // GOA PAD H 2..=6 to internal tcon_goa in normal mode
    write_reg(
        dsi,
        0xcbc0,
        &[
            0x00, 0x04, 0x04, 0x04, 0x04, //
            0x04, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    )
    .await;
    // GOA PAD H 22..=26 to internal tcon_goa in normal mode
    write_reg(
        dsi,
        0xcbd0,
        &[
            0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x04, 0x04, 0x04, 0x04, //
            0x04, 0x00, 0x00, 0x00, 0x00,
        ],
    )
    .await;
    // GOA PAD H ..=40 output level = VGL
    write_reg(dsi, 0xcbe0, &[0x00; 10]).await;
    // GOA PAD LVD output level = VGH
    write_reg(dsi, 0xcbf0, &[0xFF; 10]).await;

    // map GOA output pads to internal signals:
    // normal scan:
    // GOUT1:       none
    // GOUT2:       dir2
    // GOUT3:       clka1
    // GOUT4:       clka3
    // GOUT5:       vst1
    // GOUT6:       dir1
    // GOUT7..=21:  none
    // GOUT22:      dir2
    // GOUT23:      clka2
    // GOUT24:      clka4
    // GOUT25:      vst2
    // GOUT26:      dir1
    // GOUT27..=40: none
    write_reg(
        dsi,
        0xcc80,
        &[
            0x00, 0x26, 0x09, 0x0B, 0x01, //
            0x25, 0x00, 0x00, 0x00, 0x00,
        ],
    )
    .await;
    write_reg(
        dsi,
        0xcc90,
        &[
            0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x26, 0x0A, 0x0C, 0x02,
        ],
    )
    .await;
    write_reg(
        dsi,
        0xcca0,
        &[
            0x25, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    )
    .await;
    // reverse scan:
    // GOUT1:       none
    // GOUT2:       dir1
    // GOUT3:       clka4
    // GOUT4:       clka2
    // GOUT5:       vst2
    // GOUT6:       dir2
    // GOUT7..=21:  none
    // GOUT22:      dir1
    // GOUT23:      clka3
    // GOUT24:      clka1
    // GOUT25:      vst1
    // GOUT26:      dir2
    // GOUT27..=40: none
    write_reg(
        dsi,
        0xccb0,
        &[
            0x00, 0x25, 0x0C, 0x0A, 0x02, //
            0x26, 0x00, 0x00, 0x00, 0x00,
        ],
    )
    .await;
    write_reg(
        dsi,
        0xccc0,
        &[
            0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x25, 0x0B, 0x09, 0x01,
        ],
    )
    .await;
    write_reg(
        dsi,
        0xccd0,
        &[
            0x26, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00, //
            0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    )
    .await;

    // set pump 1 min/max DM
    write_reg(dsi, 0xc581, &[0x66]).await;

    // register not documented
    write_reg(dsi, 0xf5b6, &[0x06]).await;

    // set PWM freq to 19.531kHz
    write_reg(dsi, 0xc6b1, &[0x06]).await;

    // Gamma correction 2.2+ table
    write_reg(
        dsi,
        0xe100,
        &[
            0x00, 0x09, 0x0F, 0x0E, 0x07, 0x10, 0x0B, 0x0A, //
            0x04, 0x07, 0x0B, 0x08, 0x0F, 0x10, 0x0A, 0x01,
        ],
    )
    .await;
    // Gamma correction 2.2- table
    write_reg(
        dsi,
        0xe200,
        &[
            0x00, 0x09, 0x0F, 0x0E, 0x07, 0x10, 0x0B, 0x0A, //
            0x04, 0x07, 0x0B, 0x08, 0x0F, 0x10, 0x0A, 0x01,
        ],
    )
    .await;

    let mut gamma = [0x00; 16];
    read_reg(dsi, 0xe100, &mut gamma).await;
    // gamma.fill(0);
    // read_reg(dsi, 0xe200, &mut gamma).await;

    // exit CMD2 mode
    write_reg(dsi, 0xff00, &[0xff, 0xff, 0xff]).await;
    dsi.dcs_write(0, 0x00, [0x00]).await;

    // standard DCS initialisation
    dsi.dcs_write(0, cmd::Dcs::SLPOUT, None).await;
    Timer::after_millis(120).await;
    dsi.dcs_write(0, cmd::Dcs::COLMOD, [cmd::Colmod::Rgb888 as u8]).await;

    // configure orientation and screen area
    let madctr =
        cmd::Madctr::from(config.orientation) | cmd::Madctr::from(config.color_map);
    dsi.dcs_write(0, cmd::Dcs::MADCTR, [madctr.bits()]).await;
    let [col_hi, col_lo] = (config.cols.get() - 1).to_be_bytes();
    let [row_hi, row_lo] = (config.rows.get() - 1).to_be_bytes();
    dsi.dcs_write(0, cmd::Dcs::CASET, [0, 0, col_hi, col_lo]).await;
    dsi.dcs_write(0, cmd::Dcs::PASET, [0, 0, row_hi, row_lo]).await;

    // set display brightness
    dsi.dcs_write(0, cmd::Dcs::WRDISBV, [0x7F]).await;

    // display backlight control config
    let wctrld = cmd::Ctrld::BRIGHTNESS_CONTROL_ON
        | cmd::Ctrld::DIMMING_ON
        | cmd::Ctrld::BACKLIGHT_ON;
    dsi.dcs_write(0, cmd::Dcs::WRCTRLD, [wctrld.bits()]).await;

    // content adaptive brightness control config
    dsi.dcs_write(0, cmd::Dcs::WRCABC, [Cabc::StillPicture as u8]).await;

    // set CABC minimum brightness
    dsi.dcs_write(0, cmd::Dcs::WRCABCMB, [0xFF]).await;

    // turn display on
    dsi.dcs_write(0, cmd::Dcs::DISPON, None).await;

    dsi.dcs_write(0, cmd::Dcs::NOP, None).await;

    // send GRAM memory write to initiate frame write
    // via other DSI commands sent by LTDC

    dsi.dcs_write(0, cmd::Dcs::RAMWR, None).await;
}

mod cmd {
    use super::ColorMap;
    use super::Format;
    use super::Orientation;

    #[repr(u8)]
    #[allow(clippy::upper_case_acronyms)]
    pub enum Dcs {
        NOP = 0x00,
        SWRESET = 0x01,
        RDDMADCTR = 0x0b, // read memory data access ctrl
        RDDCOLMOD = 0x0c, // read display pixel format
        SLPIN = 0x10,     // sleep in
        SLPOUT = 0x11,    // sleep out
        PTLON = 0x12,     // partialmode on

        INVOFF = 0x20, // Inversion off
        INVON = 0x21,  // Inversion on

        DISPOFF = 0x28, // display on
        DISPON = 0x29,  // display off

        CASET = 0x2A, // Column address set
        PASET = 0x2B, // Page address set

        RAMWR = 0x2C, // Memory (GRAM) write
        RAMRD = 0x2E, // Memory (GRAM) read

        PLTAR = 0x30, // Partial area

        TEOFF = 0x34, // Tearing Effect Line Off
        TEEON = 0x35, // Tearing Effect Line On; 1 param: 'TELOM'

        MADCTR = 0x36, // memory access data ctrl; 1 param

        IDMOFF = 0x38, // Idle mode Off
        IDMON = 0x39,  // Idle mode On

        COLMOD = 0x3A, // Interface Pixel format

        RAMWRC = 0x3C, // Memory write continue
        RAMRDC = 0x3E, // Memory read continue

        WRTESCN = 0x44, // Write Tearing Effect Scan line
        RDSCNL = 0x45,  // Read  Tearing Effect Scan line

        // CABC Management, ie, Content Adaptive, Back light Control in IC OTM8009a
        WRDISBV = 0x51,  // Write Display Brightness; 1 param
        WRCTRLD = 0x53,  // Write CTRL Display; 1 param
        WRCABC = 0x55,   // Write Content Adaptive Brightness; 1 param
        WRCABCMB = 0x5E, // Write CABC Minimum Brightness; 1 param

        ID1 = 0xDA, // Read ID1
        ID2 = 0xDB, // Read ID2
        ID3 = 0xDC, // Read ID3
    }

    impl From<Dcs> for u8 {
        fn from(cmd: Dcs) -> Self {
            cmd as u8
        }
    }

    impl TryFrom<u8> for Dcs {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0x00 => Dcs::NOP,
                | 0x01 => Dcs::SWRESET,
                | 0x0b => Dcs::RDDMADCTR,
                | 0x0c => Dcs::RDDCOLMOD,
                | 0x10 => Dcs::SLPIN,
                | 0x11 => Dcs::SLPOUT,
                | 0x12 => Dcs::PTLON,
                | 0x28 => Dcs::DISPOFF,
                | 0x29 => Dcs::DISPON,
                | 0x2A => Dcs::CASET,
                | 0x2B => Dcs::PASET,
                | 0x2C => Dcs::RAMWR,
                | 0x2E => Dcs::RAMRD,
                | 0x30 => Dcs::PLTAR,
                | 0x34 => Dcs::TEOFF,
                | 0x35 => Dcs::TEEON,
                | 0x36 => Dcs::MADCTR,
                | 0x38 => Dcs::IDMOFF,
                | 0x39 => Dcs::IDMON,
                | 0x3A => Dcs::COLMOD,
                | 0x3C => Dcs::RAMWRC,
                | 0x3E => Dcs::RAMRDC,
                | 0x44 => Dcs::WRTESCN,
                | 0x45 => Dcs::RDSCNL,
                | 0x51 => Dcs::WRDISBV,
                | 0x53 => Dcs::WRCTRLD,
                | 0x55 => Dcs::WRCABC,
                | 0x5E => Dcs::WRCABCMB,
                | 0xDA => Dcs::ID1,
                | 0xDB => Dcs::ID2,
                | 0xDC => Dcs::ID3,
                | _ => return Err(()),
            })
        }
    }

    /// Tearing Effect Line Output Mode
    #[repr(u8)]
    pub enum TeeonTelom {
        VBlankOnly = 0x00,
        Both = 0x01,
    }

    impl TryFrom<u8> for TeeonTelom {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0x00 => TeeonTelom::VBlankOnly,
                | 0x01 => TeeonTelom::Both,
                | _ => return Err(()),
            })
        }
    }

    bitflags::bitflags! {
        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(Default)]
        #[derive(Hash)]
        pub struct Madctr: u8 {
            const RGB = 0 << 3;
            const BGR = 1 << 3;

            const VERT_REFRESH_TTB = 0 << 4;
            const VERT_REFRESH_BTT = 1 << 4;

            const ROW_COL_SWAP = 1 << 5;

            const COL_ADDR_LTR = 0 << 6;
            const COL_ADDR_RTL = 1 << 6;

            const ROW_ADDR_TTB = 0 << 7;
            const ROW_ADDR_BTT = 1 << 7;

            const PORTRAIT = Madctr::empty().bits();
            const LANDSCAPE = Madctr::ROW_COL_SWAP.bits() | Madctr::COL_ADDR_RTL.bits();
        }

    }

    impl From<Orientation> for Madctr {
        fn from(value: Orientation) -> Self {
            match value {
                | Orientation::Portrait => Madctr::PORTRAIT,
                | Orientation::Landscape => Madctr::LANDSCAPE,
            }
        }
    }

    impl From<ColorMap> for Madctr {
        fn from(value: ColorMap) -> Self {
            match value {
                | ColorMap::Rgb => Madctr::RGB,
                | ColorMap::Bgr => Madctr::BGR,
            }
        }
    }

    #[repr(u8)]
    pub enum Colmod {
        Rgb565 = 0x55,
        Rgb888 = 0x77,
    }

    impl TryFrom<u8> for Colmod {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0x55 => Colmod::Rgb565,
                | 0x77 => Colmod::Rgb565,
                | _ => return Err(()),
            })
        }
    }

    impl From<Format> for Colmod {
        fn from(value: Format) -> Self {
            match value {
                | Format::RGB888 => Colmod::Rgb888,
                | Format::RGB565 => Colmod::Rgb565,
            }
        }
    }

    impl From<Colmod> for Format {
        fn from(value: Colmod) -> Self {
            match value {
                | Colmod::Rgb565 => Format::RGB565,
                | Colmod::Rgb888 => Format::RGB888,
            }
        }
    }

    bitflags::bitflags! {
        #[derive(Debug)]
        #[derive(Clone, Copy)]
        #[derive(PartialEq, Eq)]
        #[derive(Default)]
        #[derive(Hash)]
        pub struct Ctrld: u8 {
            const BACKLIGHT_OFF = 0 << 2;
            const BACKLIGHT_ON = 1 << 2;

            const DIMMING_OFF = 0 << 3;
            const DIMMING_ON = 1 << 3;

            const BRIGHTNESS_CONTROL_OFF = 0 << 5;
            const BRIGHTNESS_CONTROL_ON = 1 << 5;
        }

    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Default)]
#[derive(Hash)]
#[repr(u8)]
pub enum Cabc {
    #[default]
    Off = 0b00,
    UserInterface = 0b01,
    StillPicture = 0b10,
    MovingImage = 0b11,
}

impl From<Cabc> for u8 {
    fn from(value: Cabc) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for Cabc {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            | 0b00 => Cabc::Off,
            | 0b01 => Cabc::UserInterface,
            | 0b10 => Cabc::StillPicture,
            | 0b11 => Cabc::MovingImage,
            | _ => return Err(()),
        })
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]

pub struct Config {
    pub framerate: FrameRateHz,
    pub orientation: Orientation,
    pub color_map: ColorMap,
    pub rows: NonZeroU16,
    pub cols: NonZeroU16,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[repr(u8)]
pub enum Format {
    RGB888 = 0,
    RGB565 = 2,
}

impl TryFrom<u8> for Format {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            | 0 => Self::RGB888,
            | 2 => Self::RGB565,
            | _ => return Err(()),
        })
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Default)]
#[derive(Hash)]

pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Default)]
#[derive(Hash)]
pub enum ColorMap {
    #[default]
    Rgb,
    Bgr,
}
