use core::marker::PhantomData;

use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Output;
use embassy_stm32::time::Hertz;

pub mod dma2d;
mod dsi;
pub mod image;
mod ltdc;
mod otm8009a;
pub use dma2d::InterruptHandler as Dma2dInterruptHandler;
pub use dsi::InterruptHandler as DSIInterruptHandler;
pub use ltdc::LayerConfig;
pub use otm8009a::ColorMap;
pub use otm8009a::Config;
pub use otm8009a::FrameRateHz;
pub use otm8009a::HEIGHT;
pub use otm8009a::Orientation;
pub use otm8009a::WIDTH;

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
        let mut ltdc = ltdc::Ltdc::init(ltdc, background, &video_cfg.ltdc);
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
