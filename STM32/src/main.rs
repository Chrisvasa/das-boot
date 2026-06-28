#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts,
    gpio::{Level, Output, Speed},
    peripherals,
    rcc::{
        APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv, PllSource, Sysclk,
    },
    time::mhz,
    usb::{self, Driver},
    Peri,
};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

#[embassy_executor::task]
async fn blink(pcb: Peri<'static, peripherals::PC13>, ext: Peri<'static, peripherals::PB12>) {
    let mut led_array = [
        Output::new(pcb, Level::High, Speed::Low),
        Output::new(ext, Level::Low, Speed::Low),
    ];
    loop {
        for led in &mut led_array {
            led.toggle();
        }
        Timer::after_millis(1000).await;
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
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
    info!("{} up!", NAME);
    _spawner.spawn(blink(p.PC13, p.PB12).unwrap());

    let mut ep_out_buff = [0u8; 256];
    let config = embassy_stm32::usb::Config::default();

    let driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, &mut ep_out_buff, config);
}
