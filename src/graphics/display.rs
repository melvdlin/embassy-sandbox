#![allow(dead_code)]

use core::array;
use core::mem::MaybeUninit;

use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::Output;
use embassy_stm32::time::Hertz;
use embedded_graphics::pixelcolor;

mod dsi;
mod ltdc;
mod otm8009a;
pub use dsi::InterruptHandler as DSIInterruptHandler;
pub use ltdc::Layer;
pub use ltdc::LayerConfig;
pub use otm8009a::ColorMap;
pub use otm8009a::Config;
pub use otm8009a::FrameRateHz;
pub use otm8009a::HEIGHT;
pub use otm8009a::Orientation;
pub use otm8009a::WIDTH;

use super::framebuffer::Framebuffer;

#[derive(Debug)]
#[derive(Clone, Copy)]
#[derive(PartialEq, Eq)]
#[derive(Hash)]
#[derive(Default)]
struct LayerState {
    init: bool,
    enable: bool,
}

pub struct Display<'a, C: pixelcolor::PixelColor> {
    dsi: dsi::Dsi<'a>,
    ltdc: ltdc::Ltdc,
    framebuffer: Framebuffer<'a, <C::Raw as pixelcolor::raw::ToBytes>::Bytes>,
    layers: [LayerState; 2],
}

impl<'a, C> Display<'a, C>
where
    C: pixelcolor::PixelColor,
{
    #[allow(clippy::too_many_arguments)]
    pub async fn init(
        dsi: dsi::Peripheral,
        ltdc: ltdc::Peripheral,
        buf: &'a mut [MaybeUninit<u8>],
        config: &otm8009a::Config,
        hse: Hertz,
        ltdc_clock: Hertz,
        mut reset_pin: Output<'a>,
        te_pin: impl embassy_stm32::dsihost::TePin<dsi::Peripheral>,
        _button: &mut ExtiInput<'_>,
    ) -> Self {
        let framebuffer = super::framebuffer::Framebuffer::new(
            buf,
            config.rows.get() as usize,
            config.cols.get() as usize,
        );
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
        let ltdc = ltdc::Ltdc::init(ltdc, background, &video_cfg.ltdc);
        dsi.enable();

        otm8009a::init(&mut dsi, config).await;

        #[allow(unreachable_code)]
        Self {
            dsi,
            ltdc,
            framebuffer,
            layers: array::from_fn(|_| Default::default()),
        }
    }

    pub fn framebuffer(
        &mut self,
    ) -> Framebuffer<'_, <C::Raw as pixelcolor::raw::ToBytes>::Bytes> {
        self.framebuffer.reborrow()
    }

    pub fn init_layer(
        &mut self,
        layer: embassy_stm32::ltdc::LtdcLayer,
        framebuffer: *const (),
        cfg: &ltdc::LayerConfig,
    ) {
        self.ltdc.config_layer(layer, framebuffer, cfg);
        self.layers[layer as usize].init = true;
    }

    pub fn enable_layer(&mut self, layer: embassy_stm32::ltdc::LtdcLayer, enable: bool) {
        assert!(self.layers[layer as usize].init);
        self.layers[layer as usize].enable = enable;
        self.ltdc.enable_layer(layer, enable);
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
