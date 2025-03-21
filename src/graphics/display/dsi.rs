#![allow(dead_code)]

use core::array;
use core::iter;
use core::sync::atomic;
use core::sync::atomic::AtomicUsize;

use bitflags::bitflags;
use embassy_futures::yield_now;
use embassy_stm32::gpio;
use embassy_stm32::ltdc;
use embassy_stm32::pac;
use embassy_stm32::pac::dsihost::regs;
use embassy_stm32::pac::dsihost::regs::Ghcr;
use embassy_stm32::peripherals;
use embassy_stm32::time::Hertz;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::Timer;

use crate::util::until;

pub type Peripheral = peripherals::DSIHOST;
type PacDsi = pac::dsihost::Dsihost;

const DSI: PacDsi = pac::DSIHOST;

pub struct Dsi<'a> {
    _peripheral: Peripheral,
    _te_pin: gpio::Flex<'a>,
}

/// Flow control settings
#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[derive(Default)]
pub struct FlowControl {
    /// EoTp transmission enable
    pub ettxe: bool,
    /// EoTp reception enable
    pub etrxe: bool,
    /// Bus-turn-around enable
    pub btae: bool,
    /// ECC reception enable
    pub eccrxe: bool,
    /// CRC reception enable
    pub crcrxe: bool,
}

pub mod video_mode {
    /// Video Mode configuration parameters
    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq)]
    pub struct Config {
        /// LTDC peripheral config
        pub ltdc: embassy_stm32::ltdc::LtdcConfiguration,
        /// Virtual channel ID
        pub channel: u8,
        /// Video mode
        pub mode: Mode,
        /// Size of a null packet
        pub null_packet_size: u16,
        /// Number of chunks to transmit through DSI link
        pub chunks: u16,
        /// Video packet size
        pub packet_size: u16,
        /// Whether to transmit commands in LP mide
        pub lp_commands: bool,
        /// Defines the size of the largest LP packet that can fit in a line during
        /// VSA, VBP and VFP regions
        pub largest_lp_packet: u8,
        /// Defines the size of the largest LP packet that can fit in a line during
        /// VACT regions
        pub largest_lp_vact_packet: u8,
        /// Indicates when to consider transition to LP mode
        pub lp_transitions: LpTransitions,
        /// Whether to request and acknowledge response at the end of a frame
        pub end_of_frame_ack: bool,
    }

    /// The video mode to use
    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[repr(u8)]
    pub enum Mode {
        NonBurstSyncPulses = 0,
        NonBurstSyncEvents = 1,
        Burst = 2,
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Default)]
    #[derive(Hash)]
    /// Flags indicate whether to allow transition to LP mode
    /// during their respective regions
    pub struct LpTransitions {
        /// Horizontal Front Porch
        pub hfp: bool,
        /// Horizontal Back Porch
        pub hbp: bool,
        /// Vertical Active
        pub vact: bool,
        /// Vertical Front Porch
        pub vfp: bool,
        /// Vertical Back Porch
        pub vbp: bool,
        /// VSync active
        pub vsync: bool,
    }

    impl LpTransitions {
        pub const ALL: Self = Self {
            hfp: true,
            hbp: true,
            vact: true,
            vfp: true,
            vbp: true,
            vsync: true,
        };

        pub const NONE: Self = Self {
            hfp: false,
            hbp: false,
            vact: false,
            vfp: false,
            vbp: false,
            vsync: false,
        };

        pub const HFP: Self = Self {
            hfp: true,
            ..Self::NONE
        };

        pub const HBP: Self = Self {
            hbp: true,
            ..Self::NONE
        };

        pub const VACT: Self = Self {
            vact: true,
            ..Self::NONE
        };

        pub const VFP: Self = Self {
            vfp: true,
            ..Self::NONE
        };

        pub const VBP: Self = Self {
            vbp: true,
            ..Self::NONE
        };

        pub const VSYNC: Self = Self {
            vsync: true,
            ..Self::NONE
        };
    }

    impl core::ops::BitOr for LpTransitions {
        type Output = Self;

        fn bitor(self, other: Self) -> Self::Output {
            Self {
                hfp: self.hfp | other.hfp,
                hbp: self.hbp | other.hbp,
                vact: self.vact | other.vact,
                vfp: self.vfp | other.vfp,
                vbp: self.vbp | other.vbp,
                vsync: self.vsync | other.vsync,
            }
        }
    }

    impl core::ops::BitAnd for LpTransitions {
        type Output = Self;

        fn bitand(self, other: Self) -> Self::Output {
            Self {
                hfp: self.hfp & other.hfp,
                hbp: self.hbp & other.hbp,
                vact: self.vact & other.vact,
                vfp: self.vfp & other.vfp,
                vbp: self.vbp & other.vbp,
                vsync: self.vsync & other.vsync,
            }
        }
    }

    impl core::ops::BitXor for LpTransitions {
        type Output = Self;

        fn bitxor(self, other: Self) -> Self::Output {
            Self {
                hfp: self.hfp ^ other.hfp,
                hbp: self.hbp ^ other.hbp,
                vact: self.vact ^ other.vact,
                vfp: self.vfp ^ other.vfp,
                vbp: self.vbp ^ other.vbp,
                vsync: self.vsync ^ other.vsync,
            }
        }
    }
}

impl Dsi<'_> {
    pub const fn pixel_fifo_size<P>() -> usize {
        const WORDS: usize = 960;
        WORDS * size_of::<u32>() / size_of::<P>()
    }

    pub fn init(
        dsi: Peripheral,
        te_pin: impl embassy_stm32::dsihost::TePin<Peripheral>,
    ) -> Self {
        embassy_stm32::rcc::enable_and_reset::<peripherals::DSIHOST>();
        let mut te_pin = gpio::Flex::new(te_pin);
        te_pin.set_as_af_unchecked(
            13,
            gpio::AfType::output(gpio::OutputType::OpenDrain, gpio::Speed::Low),
        );

        Self {
            _peripheral: dsi,
            _te_pin: te_pin,
        }
    }

    pub async fn clock_setup(
        &mut self,
        hse_freq: Hertz,
        lanebyteclk: Hertz,
        auto_clock_lane_control: bool,
        data_lanes: u8,
    ) {
        // enable voltage regulator
        DSI.wrpcr().modify(|w| w.set_regen(true));
        until(|| DSI.wisr().read().rrs()).await;

        // PLL setup
        let f_vco = Hertz::mhz(1_000);
        let hs_clk = Hertz::mhz(500);
        let pll_idf = 1_u32;
        let pll_ndiv = f_vco / hse_freq / 2_u32 * pll_idf;
        let pll_odf = f_vco / 2_u32 / hs_clk;

        debug_assert!((10..=125).contains(&pll_ndiv));
        debug_assert!((1..=7).contains(&pll_idf));
        debug_assert!([1, 2, 4, 8].contains(&pll_odf));

        DSI.wrpcr().modify(|w| {
            w.set_ndiv(pll_ndiv as u8);
            w.set_idf(pll_idf as u8);
            w.set_odf(pll_odf.ilog2() as u8);
            w.set_pllen(true);
        });
        Timer::after_millis(1).await;
        until(|| DSI.wisr().read().pllls()).await;

        // enable D-PHY digital section and clock lane module
        DSI.pctlr().modify(|w| {
            w.set_den(true);
            w.set_cke(true);
        });

        // set clock lane control and enable HS clock lane
        DSI.clcr().modify(|w| {
            w.set_acr(auto_clock_lane_control);
            w.set_dpcc(true);
        });

        // configure number of active data lanes
        // 0 = lane 0
        // 1 = lanes 0 and 1 (default)
        assert!(matches!(data_lanes, 1 | 2));
        DSI.pconfr().modify(|w| w.set_nl(data_lanes - 1));

        // TX escape clock = lane_byte_clk / TXECKDIV < 20 MHz
        //  <=> TXECKDIV > lane_byte_clk / 20 MHz
        //  <=> TXECKDIV > (62.5 / 20) MHz
        //  <=> TXECKDIV > 3.125
        // Timeout clock div = 1
        let txeckdiv = lanebyteclk / Hertz::mhz(20) + 1;
        DSI.ccr().modify(|w| {
            w.set_txeckdiv(txeckdiv as u8);
        });

        // set unit interval (HS mode bit period) in multiples of .25 ns
        let unit_interval = Hertz::mhz(1_000) * 4_u32 / hs_clk;
        debug_assert!(unit_interval <= 0b11_1111);
        DSI.wpcr0().modify(|w| w.set_uix4(unit_interval as u8));

        // disable error interrupts
        DSI.ier0().write_value(regs::Ier0(0));
        DSI.ier1().write_value(regs::Ier1(0));
    }

    pub fn config_flow_control(&mut self, flow_control: FlowControl) {
        DSI.pcr().modify(|w| {
            w.set_ettxe(flow_control.ettxe);
            w.set_etrxe(flow_control.etrxe);
            w.set_btae(flow_control.btae);
            w.set_eccrxe(flow_control.eccrxe);
            w.set_crcrxe(flow_control.crcrxe);
        })
    }

    pub async fn video_mode_setup(
        &mut self,
        cfg: &video_mode::Config,
        lane_byte_clock: Hertz,
        ltdc_clock: Hertz,
    ) {
        // enable video mode (disable adapted command mode)
        DSI.mcr().modify(|w| w.set_cmdm(false));
        DSI.wcfgr().modify(|w| w.set_dsim(false));

        // configure transmission type
        DSI.vmcr().modify(|w| w.set_vmt(cfg.mode as u8));

        // configure video packet size
        DSI.vpcr().modify(|w| w.set_vpsize(cfg.packet_size));

        // configure number of chunks to be transmitted through DSI link
        DSI.vccr().modify(|w| w.set_numc(cfg.chunks));

        // configure null packet size
        DSI.vnpcr().modify(|w| w.set_npsize(cfg.null_packet_size));

        // select virtual channel
        DSI.lvcidr().modify(|w| w.set_vcid(cfg.channel));

        // configure control signal polarity
        let high = ltdc::PolarityActive::ActiveHigh;
        let low = ltdc::PolarityActive::ActiveLow;
        // 0 => high, 1 => low
        // DE is inverted
        let pol = |ltdc| low == ltdc;
        DSI.lpcr().modify(|w| {
            w.set_dep(!pol(cfg.ltdc.data_enable_polarity));
            w.set_vsp(pol(cfg.ltdc.v_sync_polarity));
            w.set_hsp(pol(cfg.ltdc.h_sync_polarity));
        });

        // set DSI host and wrapper to use 24 bit color
        DSI.lcolcr().modify(|w| w.set_colc(0b101));
        DSI.wcfgr().modify(|w| w.set_colmux(0b101));

        let lane_byte_cycles = |lcd_cycles: u16| {
            lcd_cycles as u32 * (lane_byte_clock.0 / 1000) / (ltdc_clock.0 / 1000)
        };

        // HSync active in lane byte clock cycles
        let hsa = lane_byte_cycles(cfg.ltdc.h_sync);
        // horizontal back porch in lane byte clock cycles
        let hbp = lane_byte_cycles(cfg.ltdc.h_back_porch);
        // total line time in lane byte clock cycles
        let hline = cfg.ltdc.active_width
            + cfg.ltdc.h_sync
            + cfg.ltdc.h_back_porch
            + cfg.ltdc.h_front_porch;
        let hline = lane_byte_cycles(hline);
        // VSYNC active in number of lines
        let vsa = cfg.ltdc.v_sync as u32;
        // vertical back porch in number of lines
        let vbp = cfg.ltdc.v_back_porch as u32;
        // vertical front porch in number of lines
        let vfp = cfg.ltdc.v_front_porch as u32;
        // vertical active in number of lines
        let vact = cfg.ltdc.active_height as u32;
        DSI.vhsacr().write_value(regs::Vhsacr(hsa));
        DSI.vhbpcr().write_value(regs::Vhbpcr(hbp));
        DSI.vlcr().write_value(regs::Vlcr(hline));
        DSI.vvsacr().write_value(regs::Vvsacr(vsa));
        DSI.vvbpcr().write_value(regs::Vvbpcr(vbp));
        DSI.vvfpcr().write_value(regs::Vvfpcr(vfp));
        DSI.vvacr().write_value(regs::Vvacr(vact));

        // configure LP commands
        DSI.vmcr().modify(|w| w.set_lpce(cfg.lp_commands));

        // configure largest LP packet size and VACT packet size
        DSI.lpmcr().modify(|w| {
            w.set_lpsize(cfg.largest_lp_packet);
            w.set_vlpsize(cfg.largest_lp_vact_packet);
        });

        // configure LP transitions during various periods
        DSI.vmcr().modify(|w| {
            w.set_lphfpe(cfg.lp_transitions.hfp);
            w.set_lphbpe(cfg.lp_transitions.hbp);
            w.set_lpvae(cfg.lp_transitions.vact);
            w.set_lpvfpe(cfg.lp_transitions.vfp);
            w.set_lpvbpe(cfg.lp_transitions.vbp);
            w.set_lpvsae(cfg.lp_transitions.vsync);
        });

        // configure end of frame ack response
        DSI.vmcr().modify(|w| w.set_fbtaae(cfg.end_of_frame_ack));
    }

    /// Enable DSI host and wrapper.
    pub fn enable(&mut self) {
        DSI.cr().modify(|w| w.set_en(true));
        DSI.wcr().modify(|w| w.set_dsien(true));
    }

    #[allow(dead_code)]
    pub async fn generic_write<I>(&mut self, channel: u8, tx: I)
    where
        I: IntoIterator<Item = u8>,
        I::IntoIter: ExactSizeIterator,
    {
        let tx = tx.into_iter();
        let ty = match tx.len() {
            | 0 => packet::Type::Short(packet::Short::GenericWrite0P),
            | 1 => packet::Type::Short(packet::Short::GenericWrite1P),
            | 2 => packet::Type::Short(packet::Short::GenericWrite2P),
            | 3.. => packet::Type::Long(packet::Long::GenericWrite),
        };
        self.write(channel, ty, tx).await
    }

    pub async fn dcs_write<I>(&mut self, channel: u8, cmd: impl Into<u8>, tx: I)
    where
        I: IntoIterator<Item = u8>,
        I::IntoIter: ExactSizeIterator,
    {
        let tx = tx.into_iter();
        let ty = match tx.len() {
            | 0 => packet::Type::Short(packet::Short::DCSWrite0P),
            | 1 => packet::Type::Short(packet::Short::DCSWrite1P),
            | 2.. => packet::Type::Long(packet::Long::DCSWrite),
        };
        self.write(channel, ty, iter::once(cmd.into()).chain(tx)).await
    }

    #[allow(dead_code)]
    pub async fn dcs_long_write<I>(&mut self, channel: u8, cmd: impl Into<u8>, tx: I)
    where
        I: IntoIterator<Item = u8>,
    {
        self.write(
            channel,
            packet::Long::DCSWrite.into(),
            iter::once(cmd.into()).chain(tx),
        )
        .await
    }

    async fn write<I>(&mut self, channel: u8, ty: packet::Type, tx: I)
    where
        I: IntoIterator<Item = u8>,
    {
        match ty {
            | packet::Type::Long(ty) => self.long_write(channel, ty, tx).await,
            | packet::Type::Short(ty) => {
                let mut tx = tx.into_iter();
                self.short_transfer(channel, ty, tx.next(), tx.next()).await
            }
        }
    }

    async fn long_write(
        &mut self,
        channel: u8,
        ty: packet::Long,
        tx: impl IntoIterator<Item = u8>,
    ) {
        let mut len: u16 = 0;

        let mut bytes = tx.into_iter().inspect(|_| len += 1).array_chunks::<4>();

        self.wait_command_fifo_empty().await;

        for chunk in &mut bytes {
            self.wait_command_fifo_not_full().await;
            self.write_word(u32::from_le_bytes(chunk));
            self.wait_command_fifo_empty().await;
        }

        let mut remainder = bytes.into_remainder().expect("remainder cannot be `None`");
        if remainder.len() > 0 {
            self.wait_command_fifo_not_full().await;
            self.write_word(u32::from_le_bytes(array::from_fn(|_| {
                remainder.next().unwrap_or(0)
            })));

            self.wait_command_fifo_empty().await;
        }

        let [lsb, msb] = len.to_le_bytes();
        self.config_header(ty, channel, lsb, msb);

        self.wait_command_fifo_empty().await;
        self.wait_payload_write_fifo_empty().await;
    }

    async fn short_transfer(
        &mut self,
        channel: u8,
        ty: packet::Short,
        p0: Option<u8>,
        p1: Option<u8>,
    ) {
        self.wait_command_fifo_empty().await;

        self.config_header(ty, channel, p0.unwrap_or(0), p1.unwrap_or(0));

        self.wait_command_fifo_empty().await;
        self.wait_payload_write_fifo_empty().await;
    }

    #[allow(dead_code)]
    pub async fn generic_read(&mut self, channel: u8, args: &[u8], dst: &mut [u8]) {
        assert!(args.len() <= 2);
        let ty = match args.len() {
            | 0 => packet::Short::GenericRead0P,
            | 1 => packet::Short::GenericRead1P,
            | 2 => packet::Short::GenericRead2P,
            | _ => unreachable!(),
        };

        self.read(
            channel,
            ty,
            #[allow(clippy::get_first)]
            args.get(0).copied(),
            args.get(1).copied(),
            dst,
        )
        .await
    }

    pub async fn dcs_read(&mut self, channel: u8, cmd: u8, dst: &mut [u8]) {
        self.read(channel, packet::Short::DCSRead0P, Some(cmd), None, dst).await
    }

    async fn read(
        &mut self,
        channel: u8,
        ty: packet::Short,
        p0: Option<u8>,
        p1: Option<u8>,
        dst: &mut [u8],
    ) {
        let len = u16::try_from(dst.len()).expect("read len out of bounds for u16");

        self.wait_command_fifo_empty().await;

        if len > 2 {
            self.set_max_return(channel, len);
        }

        self.config_header(ty, channel, p0.unwrap_or(0), p1.unwrap_or(0));

        self.wait_read_not_busy().await;

        let mut bytes = dst.array_chunks_mut::<4>();
        for chunk in &mut bytes {
            self.wait_payload_read_fifo_not_empty().await;
            *chunk = self.read_word().to_le_bytes();
        }

        let remainder = bytes.into_remainder();
        if !remainder.is_empty() {
            self.wait_payload_read_fifo_not_empty().await;
            let word = self.read_word().to_le_bytes();
            remainder.copy_from_slice(&word[..remainder.len()]);
        }
    }

    #[inline]
    fn set_max_return(&mut self, channel: u8, size: u16) {
        let [lsb, msb] = size.to_le_bytes();
        self.config_header(packet::Short::SetMaxReturnPacketSize, channel, lsb, msb)
    }

    fn config_header(
        &mut self,
        dt: impl Into<packet::Type>,
        channel: u8,
        wclsb: u8,
        wcmsb: u8,
    ) {
        let mut ghcr = Ghcr::default();
        ghcr.set_dt(dt.into().into());
        ghcr.set_vcid(channel);
        ghcr.set_wclsb(wclsb);
        ghcr.set_wcmsb(wcmsb);

        DSI.ghcr().write_value(ghcr);

        #[cfg(debug_assertions)]
        Self::report_transaction(Transaction::header(ghcr.0));
    }

    fn write_word(&mut self, word: u32) {
        DSI.gpdr().write_value(embassy_stm32::pac::dsihost::regs::Gpdr(word));

        #[cfg(debug_assertions)]
        {
            GPDR_WORDS_WRITTEN.fetch_add(1, atomic::Ordering::Relaxed);
            Self::report_transaction(Transaction::write(word));
        }
    }

    fn read_word(&mut self) -> u32 {
        let word = DSI.gpdr().read().0;

        #[cfg(debug_assertions)]
        Self::report_transaction(Transaction::read(word));

        word
    }

    fn report_transaction(transaction: Transaction) {
        let mut t = TRANSACTIONS.try_lock().expect("deadlock");
        if t.is_full() {
            t.pop_front();
        }
        t.push_back(transaction).expect("transaction fifo has 0 capacity");
    }

    async fn wait_command_fifo_empty(&mut self) {
        while !DSI.gpsr().read().cmdfe() {
            yield_now().await
        }
    }

    async fn wait_command_fifo_not_full(&mut self) {
        while DSI.gpsr().read().cmdff() {
            yield_now().await
        }
    }

    async fn wait_read_not_busy(&mut self) {
        while DSI.gpsr().read().rcb() {
            yield_now().await
        }
    }

    async fn wait_payload_read_fifo_not_empty(&mut self) {
        while DSI.gpsr().read().prdfe() {
            yield_now().await
        }
    }

    async fn wait_payload_write_fifo_empty(&mut self) {
        while !DSI.gpsr().read().pwrfe() {
            yield_now().await
        }
    }
}

pub struct InterruptHandler {}
impl
    embassy_stm32::interrupt::typelevel::Handler<embassy_stm32::interrupt::typelevel::DSI>
    for InterruptHandler
{
    unsafe fn on_interrupt() {
        let dsihost = embassy_stm32::pac::DSIHOST;
        let wrapper_flags = dsihost.wisr().read();
        let tearing_effect = wrapper_flags.teif();
        let end_of_refresh = wrapper_flags.erif();
        let isr1 = dsihost.isr1().read();
        if isr1.lpwre()
            || isr1.gcwre()
            || isr1.gpwre()
            || isr1.gptxe()
            || isr1.gprde() | isr1.gprxe()
        {
            panic!()
        }
        _ = tearing_effect;
        _ = end_of_refresh;
        dsihost.wifcr().modify(|w| {
            w.set_cteif(true);
            w.set_cerif(true);
        });
    }
}

#[used]
#[unsafe(no_mangle)]
pub static GPDR_WORDS_WRITTEN: AtomicUsize = AtomicUsize::new(0);

/// MUST NOT BE HELD ACROSS AWAIT POINTS
#[used]
#[unsafe(no_mangle)]
pub static TRANSACTIONS: Mutex<ThreadModeRawMutex, heapless::Deque<Transaction, 1024>> =
    Mutex::new(heapless::Deque::new());

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[repr(C)]
pub struct Transaction {
    pub ty: TransactionType,
    pub data: u32,
}

impl Transaction {
    pub const fn new(ty: TransactionType, data: u32) -> Self {
        Self { ty, data }
    }

    pub const fn header(data: u32) -> Self {
        Self::new(TransactionType::HeaderWrite, data)
    }

    pub const fn write(data: u32) -> Self {
        Self::new(TransactionType::DataWrite, data)
    }

    pub const fn read(data: u32) -> Self {
        Self::new(TransactionType::DataRead, data)
    }
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[repr(u32)]
pub enum TransactionType {
    HeaderWrite = 0x11111111,
    DataWrite = 0x22222222,
    DataRead = 0x33333333,
}

pub mod packet {
    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    pub enum Type {
        Short(Short),
        Long(Long),
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[repr(u8)]
    pub enum Short {
        GenericWrite0P = 0x03,
        GenericWrite1P = 0x13,
        GenericWrite2P = 0x23,
        GenericRead0P = 0x04,
        GenericRead1P = 0x14,
        GenericRead2P = 0x24,
        DCSWrite0P = 0x05,
        DCSWrite1P = 0x15,
        DCSRead0P = 0x06,
        SetMaxReturnPacketSize = 0x37,
    }

    #[derive(Debug)]
    #[derive(Clone, Copy)]
    #[derive(PartialEq, Eq)]
    #[derive(Hash)]
    #[repr(u8)]
    pub enum Long {
        Null = 0x09,
        Blanking = 0x19,
        GenericWrite = 0x29,
        DCSWrite = 0x39,
        YCbCr20LooselyPacked = 0x0c,
        YCbCr24Packed = 0x1c,
        YCbCr16Packed = 0x2c,
        RGB30Packed = 0x0d,
        RGB36Packed = 0x1d,
        YCbCr12Packed = 0x3d,
        RGB16Packed = 0x0e,
        RGB18Packed = 0x1e,
        RGB18LooselyPacked = 0x2e,
        RGB24Packed = 0x3e,
    }

    impl TryFrom<u8> for Type {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Short::try_from(value)
                .map(Type::Short)
                .or_else(|()| Long::try_from(value).map(Type::Long))
        }
    }

    impl From<Type> for u8 {
        fn from(value: Type) -> Self {
            match value {
                | Type::Short(short) => u8::from(short),
                | Type::Long(long) => u8::from(long),
            }
        }
    }

    impl From<Short> for Type {
        fn from(short: Short) -> Self {
            Type::Short(short)
        }
    }

    impl From<Long> for Type {
        fn from(long: Long) -> Self {
            Type::Long(long)
        }
    }

    impl From<Short> for u8 {
        fn from(short: Short) -> Self {
            short as Self
        }
    }

    impl TryFrom<u8> for Short {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0x03 => Short::GenericWrite0P,
                | 0x13 => Short::GenericWrite1P,
                | 0x23 => Short::GenericWrite2P,
                | 0x04 => Short::GenericRead0P,
                | 0x14 => Short::GenericRead1P,
                | 0x24 => Short::GenericRead2P,
                | 0x05 => Short::DCSWrite0P,
                | 0x15 => Short::DCSWrite1P,
                | 0x06 => Short::DCSRead0P,
                | 0x37 => Short::SetMaxReturnPacketSize,
                | _ => return Err(()),
            })
        }
    }

    impl From<Long> for u8 {
        fn from(long: Long) -> Self {
            long as Self
        }
    }

    impl TryFrom<u8> for Long {
        type Error = ();

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                | 0x09 => Long::Null,
                | 0x19 => Long::Blanking,
                | 0x29 => Long::GenericWrite,
                | 0x39 => Long::DCSWrite,
                | 0x0c => Long::YCbCr20LooselyPacked,
                | 0x1c => Long::YCbCr24Packed,
                | 0x2c => Long::YCbCr16Packed,
                | 0x0d => Long::RGB30Packed,
                | 0x1d => Long::RGB36Packed,
                | 0x3d => Long::YCbCr12Packed,
                | 0x0e => Long::RGB16Packed,
                | 0x1e => Long::RGB18Packed,
                | 0x2e => Long::RGB18LooselyPacked,
                | 0x3e => Long::RGB24Packed,
                | _ => return Err(()),
            })
        }
    }
}
