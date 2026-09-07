use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::mode::Async;
use embassy_stm32::usart::{Config, RingBufferedUartRx, Uart, UartTx};
use embassy_stm32::{bind_interrupts, dma, peripherals, usart, Peri};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USART1 => usart::InterruptHandler<peripherals::USART1>;
    DMA2_STREAM7 => dma::InterruptHandler<peripherals::DMA2_CH7>;
    DMA2_STREAM2 => dma::InterruptHandler<peripherals::DMA2_CH2>;
});

pub type Reader = RingBufferedUartRx<'static>;
pub type Writer = UartTx<'static, Async>;

pub async fn setup(
    uart: Peri<'static, peripherals::USART1>,
    rx_pin: Peri<'static, peripherals::PA10>,
    tx_pin: Peri<'static, peripherals::PA9>,
    tx_dma: Peri<'static, peripherals::DMA2_CH7>,
    rx_dma: Peri<'static, peripherals::DMA2_CH2>,
    _spawner: Spawner,
) -> (Reader, Writer) {
    let config = Config::default();
    let uart = defmt::unwrap!(Uart::new(uart, rx_pin, tx_pin, tx_dma, rx_dma, Irqs, config));
    let (tx, rx) = uart.split();
    static RX_RING: StaticCell<[u8; 256]> = StaticCell::new();
    let mut rx = rx.into_ring_buffered(RX_RING.init([0u8; 256]));
    rx.start_uart();
    info!("UART setup done");
    (rx, tx)
}
