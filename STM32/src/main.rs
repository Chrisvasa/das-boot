#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::{
    rcc::{
        APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv, PllSource, Sysclk,
    },
    time::mhz,
};

use crate::parser::communication_task;
use crate::servo_handler::pwm_task;

use {defmt_rtt as _, panic_probe as _};

mod parser;
mod servo_handler;
mod transport;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    fn clock_cfg() -> embassy_stm32::Config {
        let mut conf = embassy_stm32::Config::default();
        conf.rcc.hse = Some(Hse {
            freq: mhz(25),
            mode: HseMode::Oscillator,
        });
        conf.rcc.pll_src = PllSource::HSE;
        conf.rcc.pll = Some(Pll {
            prediv: PllPreDiv::DIV25,
            mul: PllMul::MUL336,
            divp: Some(PllPDiv::DIV4),
            divq: Some(PllQDiv::DIV7),
            divr: None,
        });
        conf.rcc.sys = Sysclk::PLL1_P;
        conf.rcc.apb1_pre = APBPrescaler::DIV2;
        conf
    }

    let p = embassy_stm32::init(clock_cfg());
    const NAME: &str = env!("CARGO_PKG_NAME");
    info!("{} booting!", NAME);

    #[cfg(feature = "usb")]
    let (rx, tx) = transport::setup(p.USB_OTG_FS, p.PA12, p.PA11, spawner).await;
    #[cfg(feature = "uart")]
    let (rx, tx) = transport::setup(p.USART1, p.PA10, p.PA9, p.DMA2_CH7, p.DMA2_CH2, spawner).await;

    spawner.spawn(defmt::unwrap!(communication_task(rx, tx)));
    spawner.spawn(defmt::unwrap!(pwm_task(p.TIM4, p.PB6, p.PB7, p.PB8, p.PB9)));
}
