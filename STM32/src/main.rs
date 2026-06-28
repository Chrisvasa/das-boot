#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::{
    gpio::{Level, Output, Speed},
    Peripherals,
};
use embassy_time::Timer;
// Pull in the defmt RTT transport and the panic handler. The `as _` keeps them
// linked without bringing names into scope.
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::task]
async fn blink(p: Peripherals) {
    let mut led_array = [
        Output::new(p.PC13, Level::High, Speed::Low),
        Output::new(p.PB12, Level::Low, Speed::Low),
    ];
    loop {
        for led in led_array.iter_mut() {
            led.toggle();
        }
        Timer::after_millis(1000).await;
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());
    const NAME: &str = env!("CARGO_PKG_NAME");
    info!("{} up!", NAME);
    _spawner.spawn(blink(p).unwrap());
}
