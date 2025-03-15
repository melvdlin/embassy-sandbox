use core::array;
use core::iter;
use core::sync::atomic;
use core::sync::atomic::AtomicUsize;

use embassy_futures::yield_now;
use embassy_stm32::pac::dsihost::Dsihost;
use embassy_stm32::pac::dsihost::regs::Ghcr;
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;
use embassy_sync::mutex::Mutex;

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

#[allow(dead_code)]
pub async fn generic_write<I>(dsi: Dsihost, channel: u8, tx: I)
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
    write(dsi, channel, ty, tx).await
}

pub async fn dcs_write<I>(dsi: Dsihost, channel: u8, cmd: impl Into<u8>, tx: I)
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
    write(dsi, channel, ty, iter::once(cmd.into()).chain(tx)).await
}

#[allow(dead_code)]
pub async fn dcs_long_write<I>(dsi: Dsihost, channel: u8, cmd: impl Into<u8>, tx: I)
where
    I: IntoIterator<Item = u8>,
{
    write(
        dsi,
        channel,
        packet::Long::DCSWrite.into(),
        iter::once(cmd.into()).chain(tx),
    )
    .await
}

async fn write<I>(dsi: Dsihost, channel: u8, ty: packet::Type, tx: I)
where
    I: IntoIterator<Item = u8>,
{
    match ty {
        | packet::Type::Long(ty) => long_write(dsi, channel, ty, tx).await,
        | packet::Type::Short(ty) => {
            let mut tx = tx.into_iter();
            short_transfer(dsi, channel, ty, tx.next(), tx.next()).await
        }
    }
}

async fn long_write(
    dsi: Dsihost,
    channel: u8,
    ty: packet::Long,
    tx: impl IntoIterator<Item = u8>,
) {
    let mut len: u16 = 0;

    let mut bytes = tx.into_iter().inspect(|_| len += 1).array_chunks::<4>();

    wait_command_fifo_empty(dsi).await;

    for chunk in &mut bytes {
        wait_command_fifo_not_full(dsi).await;
        write_word(dsi, u32::from_le_bytes(chunk));
        wait_command_fifo_empty(dsi).await;
    }

    let mut remainder = bytes.into_remainder().expect("remainder cannot be `None`");
    if remainder.len() > 0 {
        wait_command_fifo_not_full(dsi).await;
        write_word(
            dsi,
            u32::from_le_bytes(array::from_fn(|_| remainder.next().unwrap_or(0))),
        );

        wait_command_fifo_empty(dsi).await;
    }

    let [lsb, msb] = len.to_le_bytes();
    config_header(dsi, ty, channel, lsb, msb);

    wait_command_fifo_empty(dsi).await;
    wait_payload_write_fifo_empty(dsi).await;
}

async fn short_transfer(
    dsi: Dsihost,
    channel: u8,
    ty: packet::Short,
    p0: Option<u8>,
    p1: Option<u8>,
) {
    wait_command_fifo_empty(dsi).await;

    config_header(dsi, ty, channel, p0.unwrap_or(0), p1.unwrap_or(0));

    wait_command_fifo_empty(dsi).await;
    wait_payload_write_fifo_empty(dsi).await;
}

#[allow(dead_code)]
pub async fn generic_read(dsi: Dsihost, channel: u8, args: &[u8], dst: &mut [u8]) {
    assert!(args.len() <= 2);
    let ty = match args.len() {
        | 0 => packet::Short::GenericRead0P,
        | 1 => packet::Short::GenericRead1P,
        | 2 => packet::Short::GenericRead2P,
        | _ => unreachable!(),
    };

    read(
        dsi,
        channel,
        ty,
        #[allow(clippy::get_first)]
        args.get(0).copied(),
        args.get(1).copied(),
        dst,
    )
    .await
}

pub async fn dcs_read(dsi: Dsihost, channel: u8, cmd: u8, dst: &mut [u8]) {
    read(dsi, channel, packet::Short::DCSRead0P, Some(cmd), None, dst).await
}

async fn read(
    dsi: Dsihost,
    channel: u8,
    ty: packet::Short,
    p0: Option<u8>,
    p1: Option<u8>,
    dst: &mut [u8],
) {
    let len = u16::try_from(dst.len()).expect("read len out of bounds for u16");

    wait_command_fifo_empty(dsi).await;

    if len > 2 {
        set_max_return(dsi, channel, len);
    }

    config_header(dsi, ty, channel, p0.unwrap_or(0), p1.unwrap_or(0));

    wait_read_not_busy(dsi).await;

    let mut bytes = dst.array_chunks_mut::<4>();
    for chunk in &mut bytes {
        wait_payload_read_fifo_not_empty(dsi).await;
        *chunk = read_word(dsi).to_le_bytes();
    }

    let remainder = bytes.into_remainder();
    if !remainder.is_empty() {
        wait_payload_read_fifo_not_empty(dsi).await;
        let word = read_word(dsi).to_le_bytes();
        remainder.copy_from_slice(&word[..remainder.len()]);
    }
}

#[inline]
fn set_max_return(dsi: Dsihost, channel: u8, size: u16) {
    let [lsb, msb] = size.to_le_bytes();
    config_header(
        dsi,
        packet::Short::SetMaxReturnPacketSize,
        channel,
        lsb,
        msb,
    )
}

fn config_header(
    dsi: Dsihost,
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

    dsi.ghcr().write_value(ghcr);

    #[cfg(debug_assertions)]
    report_transaction(Transaction::header(ghcr.0));
}

fn write_word(dsi: Dsihost, word: u32) {
    dsi.gpdr().write_value(embassy_stm32::pac::dsihost::regs::Gpdr(word));

    #[cfg(debug_assertions)]
    {
        GPDR_WORDS_WRITTEN.fetch_add(1, atomic::Ordering::Relaxed);
        report_transaction(Transaction::write(word));
    }
}

fn read_word(dsi: Dsihost) -> u32 {
    let word = dsi.gpdr().read().0;

    #[cfg(debug_assertions)]
    report_transaction(Transaction::read(word));

    word
}

fn report_transaction(transaction: Transaction) {
    let mut t = TRANSACTIONS.try_lock().expect("deadlock");
    if t.is_full() {
        t.pop_front();
    }
    t.push_back(transaction).expect("transaction fifo has 0 capacity");
}

async fn wait_command_fifo_empty(dsi: Dsihost) {
    while !dsi.gpsr().read().cmdfe() {
        yield_now().await
    }
}

async fn wait_command_fifo_not_full(dsi: Dsihost) {
    while dsi.gpsr().read().cmdff() {
        yield_now().await
    }
}

async fn wait_read_not_busy(dsi: Dsihost) {
    while dsi.gpsr().read().rcb() {
        yield_now().await
    }
}

async fn wait_payload_read_fifo_not_empty(dsi: Dsihost) {
    while dsi.gpsr().read().prdfe() {
        yield_now().await
    }
}

async fn wait_payload_write_fifo_empty(dsi: Dsihost) {
    while !dsi.gpsr().read().pwrfe() {
        yield_now().await
    }
}
