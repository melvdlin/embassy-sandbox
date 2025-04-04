use embassy_time::Delay;
use static_cell::StaticCell;

type Sdram = stm32_fmc::Sdram<
    embassy_stm32::fmc::Fmc<'static, embassy_stm32::peripherals::FMC>,
    stm32_fmc::devices::is42s32400f_6::Is42s32400f6,
>;

/// # Safety
/// SIZE must be at most the SDRAM size in bytes
pub unsafe fn init<const SIZE: usize>(sdram: Sdram) -> &'static mut [u32] {
    static SDRAM: StaticCell<Sdram> = StaticCell::new();
    let sdram = SDRAM.init(sdram);

    let ptr = sdram.init(&mut Delay);
    // Safety:
    // - it is assumed that `embassy_stm32::fmc::Fmc::sdram_a13bits_d32bits_4banks_bank1`
    //   returns a read/write valid pointer
    // - the source ptr does not escape this scope
    let size = SIZE / size_of::<u32>();
    assert!(size <= isize::MAX as usize);
    assert!(ptr.wrapping_add(size) >= ptr);
    unsafe { core::slice::from_raw_parts_mut(ptr, SIZE) }
}

#[macro_export]
macro_rules! create_sdram {
    ($peripherals:ident) => {
        embassy_stm32::fmc::Fmc::sdram_a13bits_d32bits_4banks_bank1(
            $peripherals.FMC,
            $peripherals.PF0,
            $peripherals.PF1,
            $peripherals.PF2,
            $peripherals.PF3,
            $peripherals.PF4,
            $peripherals.PF5,
            $peripherals.PF12,
            $peripherals.PF13,
            $peripherals.PF14,
            $peripherals.PF15,
            $peripherals.PG0,
            $peripherals.PG1,
            $peripherals.PG2,
            $peripherals.PG4,
            $peripherals.PG5,
            $peripherals.PD14,
            $peripherals.PD15,
            $peripherals.PD0,
            $peripherals.PD1,
            $peripherals.PE7,
            $peripherals.PE8,
            $peripherals.PE9,
            $peripherals.PE10,
            $peripherals.PE11,
            $peripherals.PE12,
            $peripherals.PE13,
            $peripherals.PE14,
            $peripherals.PE15,
            $peripherals.PD8,
            $peripherals.PD9,
            $peripherals.PD10,
            $peripherals.PH8,
            $peripherals.PH9,
            $peripherals.PH10,
            $peripherals.PH11,
            $peripherals.PH12,
            $peripherals.PH13,
            $peripherals.PH14,
            $peripherals.PH15,
            $peripherals.PI0,
            $peripherals.PI1,
            $peripherals.PI2,
            $peripherals.PI3,
            $peripherals.PI6,
            $peripherals.PI7,
            $peripherals.PI9,
            $peripherals.PI10,
            $peripherals.PE0,
            $peripherals.PE1,
            $peripherals.PI4,
            $peripherals.PI5,
            $peripherals.PH2,
            $peripherals.PG8,
            $peripherals.PG15,
            $peripherals.PH3,
            $peripherals.PF11,
            $peripherals.PH5,
            stm32_fmc::devices::is42s32400f_6::Is42s32400f6 {},
        )
    };
}

pub use create_sdram;
