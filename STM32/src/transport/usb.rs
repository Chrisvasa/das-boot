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

const EP_OUT_SIZE: usize = 256;
const RX_BUFF_SIZE: usize = 256;
const DESCRIPTOR_SIZE: usize = 256;
const CTRL_BUFF_SIZE: usize = 64;
const MAX_PKT_SIZE: u16 = 64;

pub async fn setup(
    up: Peri<'static, peripherals::USB_OTG_FS>,
    dp: Peri<'static, peripherals::PA12>,
    dm: Peri<'static, peripherals::PA11>,
    spawner: Spawner,
) -> (Reader, Writer) {
    static EP_OUT_BUFFER: StaticCell<[u8; EP_OUT_SIZE]> = StaticCell::new();
    static RX_BUFF: StaticCell<[u8; RX_BUFF_SIZE]> = StaticCell::new();
    let ep_out_buff = EP_OUT_BUFFER.init([0u8; EP_OUT_SIZE]);
    let config = embassy_stm32::usb::Config::default();

    let driver = Driver::new_fs(up, Irqs, dp, dm, ep_out_buff, config);
    let usb_conf = {
        let mut usb_conf = embassy_usb::Config::new(0xdead, 0xbeef);
        usb_conf.product = Some("boot-stm");
        usb_conf
    };

    let mut builder = {
        static CONFIG_DESCRIPTOR: StaticCell<[u8; DESCRIPTOR_SIZE]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; DESCRIPTOR_SIZE]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; CTRL_BUFF_SIZE]> = StaticCell::new();
        let builder = embassy_usb::Builder::new(
            driver,
            usb_conf,
            CONFIG_DESCRIPTOR.init([0; DESCRIPTOR_SIZE]),
            BOS_DESCRIPTOR.init([0; DESCRIPTOR_SIZE]),
            &mut [],
            CONTROL_BUF.init([0; CTRL_BUFF_SIZE]),
        );
        builder
    };

    let class = {
        static STATE: StaticCell<State> = StaticCell::new();
        let state = STATE.init(State::new());
        CdcAcmClass::new(&mut builder, state, MAX_PKT_SIZE)
    };

    let device = builder.build();
    spawner.spawn(defmt::unwrap!(usb_run(device)));

    let (tx, rx) = class.split();
    let rx = rx.into_buffered(RX_BUFF.init([0; RX_BUFF_SIZE]));
    info!("USB setup done");
    (rx, tx)
}

#[embassy_executor::task]
async fn usb_run(mut device: MyUsbDevice) -> ! {
    device.run().await
}
