#![no_std]
#![no_main]

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts,
    gpio::{Level, Output, Speed},
    peripherals,
    rcc::{
        APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv, PllSource, Sysclk,
    },
    time::mhz,
    usb::{self, Driver, Instance},
};
use embassy_time::Timer;
use embassy_usb::{
    class::cdc_acm::CdcAcmClass, class::cdc_acm::State, driver::EndpointError, UsbDevice,
};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

#[embassy_executor::task]
async fn blink(mut leds: [Output<'static>; 2]) {
    loop {
        for led in &mut leds {
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
    _spawner.spawn(
        blink([
            Output::new(p.PC13, Level::High, Speed::Low),
            Output::new(p.PB12, Level::Low, Speed::Low),
        ])
        .unwrap(),
    );

    static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
    let ep_out_buff = EP_OUT_BUFFER.init([0u8; 256]);
    let config = embassy_stm32::usb::Config::default();

    let driver = Driver::new_fs(p.USB_OTG_FS, Irqs, p.PA12, p.PA11, ep_out_buff, config);
    let usb_conf = embassy_usb::Config::new(0x0483, 0x5740);
    let mut builder = {
        static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
        let builder = embassy_usb::Builder::new(
            driver,
            usb_conf,
            CONFIG_DESCRIPTOR.init([0; 256]),
            BOS_DESCRIPTOR.init([0; 256]),
            &mut [],
            CONTROL_BUF.init([0; 64]),
        );
        builder
    };

    let mut class = {
        static STATE: StaticCell<State> = StaticCell::new();
        let state = STATE.init(State::new());
        CdcAcmClass::new(&mut builder, state, 64)
    };

    let usb = builder.build();
    _spawner.spawn(unwrap!(usb_task(usb)));

    loop {
        class.wait_connection().await;
        info!("Connected");
        let _ = echo(&mut class).await;
        info!("Disconnected");
    }
}

type MyUsbDriver = Driver<'static, peripherals::USB_OTG_FS>;
type MyUsbDevice = UsbDevice<'static, MyUsbDriver>;

#[embassy_executor::task]
async fn usb_task(mut usb: MyUsbDevice) -> ! {
    usb.run().await
}

struct Disconnected {}

impl From<EndpointError> for Disconnected {
    fn from(val: EndpointError) -> Self {
        match val {
            EndpointError::BufferOverflow => panic!("Buffer overflow"),
            EndpointError::Disabled => Disconnected {},
        }
    }
}

async fn echo<'d, T: Instance + 'd>(
    class: &mut CdcAcmClass<'d, Driver<'d, T>>,
) -> Result<(), Disconnected> {
    let mut buf = [0; 64];
    loop {
        let n = class.read_packet(&mut buf).await?;
        let data = &buf[..n];
        info!("Data: {:x}", data);
        class.write_packet(data).await?;
    }
}
