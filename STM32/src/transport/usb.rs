use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts,
    peripherals::{self},
    usb::{self, Driver},
    Peri,
};
use embassy_usb::class::cdc_acm::{BufferedReceiver, Sender};
use embassy_usb::{class::cdc_acm::CdcAcmClass, class::cdc_acm::State, UsbDevice};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    OTG_FS => usb::InterruptHandler<peripherals::USB_OTG_FS>;
});

type MyUsbDriver = Driver<'static, peripherals::USB_OTG_FS>;
type MyUsbDevice = UsbDevice<'static, MyUsbDriver>;
pub type Reader = BufferedReceiver<'static, MyUsbDriver>;
pub type Writer = Sender<'static, MyUsbDriver>;

pub async fn setup(
    up: Peri<'static, peripherals::USB_OTG_FS>,
    dp: Peri<'static, peripherals::PA12>,
    dm: Peri<'static, peripherals::PA11>,
    spawner: Spawner,
) -> (Reader, Writer) {
    static EP_OUT_BUFFER: StaticCell<[u8; 256]> = StaticCell::new();
    static RX_BUFF: StaticCell<[u8; 256]> = StaticCell::new();
    let ep_out_buff = EP_OUT_BUFFER.init([0u8; 256]);
    let config = embassy_stm32::usb::Config::default();

    let driver = Driver::new_fs(up, Irqs, dp, dm, ep_out_buff, config);
    let usb_conf = {
        let mut usb_conf = embassy_usb::Config::new(0xdead, 0xbeef);
        usb_conf.product = Some("boot-stm");
        usb_conf
    };

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

    let class = {
        static STATE: StaticCell<State> = StaticCell::new();
        let state = STATE.init(State::new());
        CdcAcmClass::new(&mut builder, state, 64)
    };

    let device = builder.build();
    spawner.spawn(usb_run(device).unwrap());

    let (tx, rx) = class.split();
    let rx = rx.into_buffered(RX_BUFF.init([0; 256]));
    info!("USB setup done");
    (rx, tx)
}

#[embassy_executor::task]
async fn usb_run(mut device: MyUsbDevice) -> ! {
    device.run().await
}
