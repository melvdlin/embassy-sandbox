use core::borrow::BorrowMut;
use core::marker::PhantomData;

use dma2d::Dma2d;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Output;
use embassy_stm32::interrupt::typelevel as interrupt;
use embassy_stm32::time::Hertz;

pub mod dma2d;
mod dsi;
mod ltdc;
mod otm8009a;
pub use dma2d::InterruptHandler as Dma2dInterruptHandler;
pub use dsi::InterruptHandler as DsiInterruptHandler;
pub use ltdc::ErrorInterruptHandler as LtdcErrorInterruptHandler;
pub use ltdc::InterruptHandler as LtdcInterruptHandler;
pub use ltdc::LayerConfig;
pub use otm8009a::ColorMap;
pub use otm8009a::Config;
pub use otm8009a::FrameRateHz;
pub use otm8009a::HEIGHT;
pub use otm8009a::Orientation;
pub use otm8009a::WIDTH;

use super::accelerated::Backing;
use super::accelerated::Framebuffer;
use super::color::Argb8888;
use crate::util::typelevel::MapOnce;

pub struct Display<'a> {
    dsi: dsi::Dsi<'a>,
    ltdc: ltdc::Ltdc,
}

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct Layer1<'disp>(PhantomData<&'disp ltdc::Ltdc>);

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct Layer2<'disp>(PhantomData<&'disp ltdc::Ltdc>);

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
pub struct DynLayer<'a>(ltdc::Layer, PhantomData<&'a ltdc::Ltdc>);

pub trait Layer: Copy {
    fn as_index(&self) -> ltdc::Layer;
}

pub trait ConstLayer: Copy {
    const INDEX: ltdc::Layer;

    fn erase<'a>(self) -> DynLayer<'a>
    where
        Self: 'a,
    {
        DynLayer(Self::INDEX, PhantomData)
    }
}

impl<'a> Display<'a> {
    #[allow(clippy::too_many_arguments)]
    pub async fn init<'l1, 'l2, L1, L2>(
        dsi: dsi::Peripheral,
        ltdc: ltdc::Peripheral,
        irq: impl interrupt::Binding<interrupt::LTDC, ltdc::InterruptHandler>
        + interrupt::Binding<interrupt::LTDC_ER, ltdc::ErrorInterruptHandler>,
        config: &otm8009a::Config,
        layer_1_config: L1,
        layer_2_config: L2,
        hse: Hertz,
        ltdc_clock: Hertz,
        mut reset_pin: Output<'a>,
        te_pin: impl embassy_stm32::dsihost::TePin<dsi::Peripheral>,
        _button: &mut ExtiInput<'_>,
    ) -> (Self, L1::Output<Layer1<'a>>, L2::Output<Layer2<'a>>)
    where
        L1: MapOnce<&'l1 ltdc::LayerConfig>,
        L2: MapOnce<&'l2 ltdc::LayerConfig>,
    {
        let lane_byte_clock = Hertz::khz(62_500);

        let video_cfg = dsi::video_mode::Config {
            ltdc: otm8009a::ltdc_video_config(
                config.rows,
                config.cols,
                config.orientation,
            ),
            channel: 0,
            mode: dsi::video_mode::Mode::Burst,
            null_packet_size: 0xFFF,
            chunks: 0,
            packet_size: config.cols.get(),
            lp_commands: true,
            largest_lp_packet: 16,
            largest_lp_vact_packet: 0,
            lp_transitions: dsi::video_mode::LpTransitions::ALL,
            end_of_frame_ack: false,
        };

        let background = embassy_stm32::ltdc::RgbColor {
            red: 0,
            green: 0,
            blue: 0,
        };

        otm8009a::reset(&mut reset_pin).await;

        let mut dsi = dsi::Dsi::init(dsi, te_pin);
        dsi.clock_setup(hse, Hertz::khz(62_500), false, 2).await;
        dsi.video_mode_setup(&video_cfg, lane_byte_clock, ltdc_clock).await;
        let mut ltdc = ltdc::Ltdc::init(ltdc, irq, background, &video_cfg.ltdc);
        dsi.enable();

        otm8009a::init(&mut dsi, config).await;

        let layer_1 = layer_1_config.map_once(|cfg| {
            let layer = Layer1(PhantomData);
            ltdc.config_layer(layer.as_index(), cfg);
            layer
        });
        let layer_2 = layer_2_config.map_once(|cfg| {
            let layer = Layer2(PhantomData);
            ltdc.config_layer(layer.as_index(), cfg);
            layer
        });

        (Self { dsi, ltdc }, layer_1, layer_2)
    }

    pub fn enable_layer(&mut self, layer: impl Layer, enable: bool) {
        self.ltdc.enable_layer(layer.as_index(), enable);
    }

    pub fn set_buffer(&mut self, buffer: *const (), layer: impl Layer) {
        self.ltdc.set_framebuffer(buffer, layer.as_index());
    }

    pub async fn enable(&mut self, enable: bool) {
        otm8009a::enable(&mut self.dsi, enable).await;
    }

    pub async fn sleep(&mut self, sleep: bool) {
        otm8009a::sleep(&mut self.dsi, sleep).await;
    }

    pub async fn set_brightness(&mut self, brightness: u8) {
        otm8009a::set_brightness(&mut self.dsi, brightness).await;
    }
}

impl ConstLayer for Layer1<'_> {
    const INDEX: ltdc::Layer = ltdc::Layer::Layer1;
}

impl ConstLayer for Layer2<'_> {
    const INDEX: ltdc::Layer = ltdc::Layer::Layer2;
}

impl<T> Layer for T
where
    T: ConstLayer,
{
    fn as_index(&self) -> ltdc::Layer {
        T::INDEX
    }
}

impl Layer for DynLayer<'_> {
    fn as_index(&self) -> ltdc::Layer {
        self.0
    }
}

pub struct DoubleBuffer<B, D, L> {
    width: u16,
    height: u16,
    front: B,
    back: B,
    dma: D,
    layer: L,
}

impl<B, D, L> DoubleBuffer<B, D, L>
where
    B: AsMut<[Argb8888]>,
{
    /// Construct a new double buffer.
    ///
    /// # Panics
    ///
    /// - Panics if `front.as_mut().len() != width * height`
    /// - Panics if `back.as_mut().len() != width * height`
    pub fn new(
        width: u16,
        height: u16,
        mut front: B,
        mut back: B,
        dma: D,
        layer: L,
    ) -> Self {
        assert_eq!(front.as_mut().len(), width as usize * height as usize);
        assert_eq!(back.as_mut().len(), width as usize * height as usize);

        Self {
            width,
            height,
            front,
            back,
            dma,
            layer,
        }
    }
}

impl<B, D, L> DoubleBuffer<B, D, L>
where
    B: AsMut<[Argb8888]>,
    D: BorrowMut<Dma2d>,
    L: Layer,
{
    pub async fn swap(&mut self, display: &mut Display<'_>) -> Framebuffer<'_, &mut B> {
        self.swap_front(display).await.0
    }

    pub async fn swap_front(
        &mut self,
        display: &mut Display<'_>,
    ) -> (Framebuffer<'_, &mut B>, &mut B) {
        core::mem::swap(&mut self.front, &mut self.back);
        display.set_buffer(self.front.as_mut() as *mut _ as *const _, self.layer);
        display.ltdc.reload().await;
        (
            Framebuffer::new(
                &mut self.back,
                self.width,
                self.height,
                self.dma.borrow_mut(),
            ),
            &mut self.front,
        )
    }

    pub fn back(&mut self) -> Framebuffer<'_, &mut B> {
        Framebuffer::new(
            &mut self.back,
            self.width,
            self.height,
            self.dma.borrow_mut(),
        )
    }
}
