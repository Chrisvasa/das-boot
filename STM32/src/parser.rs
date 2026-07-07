use defmt::{info, warn};
use embassy_time::Timer;
use embedded_io_async::{Read, Write};

use crate::transport;

#[embassy_executor::task]
pub async fn echo(rx: transport::Reader, tx: transport::Writer) -> ! {
    run(rx, tx).await
}

async fn run<R: Read, W: Write>(mut rx: R, mut tx: W) -> ! {
    let wbuff = [1, 2, 3, 4, 5];
    let mut buff = [0u8; 64];
    let _ = tx.write(&wbuff).await;
    loop {
        match rx.read(&mut buff).await {
            Ok(n) if n > 0 => {
                info!("Data: {:x}", &buff[..n]);
                if let Err(error) = tx.write_all(&buff[..n]).await {
                    warn!("Write error: {:?}", defmt::Debug2Format(&error));
                }
            }
            Ok(_) => Timer::after_millis(50).await,
            Err(error) => {
                warn!("Read error: {:?}", defmt::Debug2Format(&error));
                Timer::after_millis(100).await;
            }
        }
    }
}
