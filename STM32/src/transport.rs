//! Transport layer. Exactly one of `usb` / `uart` is compiled, selected by a
//! Cargo feature. Both expose the same interface to the rest of the firmware.

#[cfg(all(feature = "usb", feature = "uart"))]
compile_error!("enable exactly one transport feature: `usb` or `uart`");
#[cfg(not(any(feature = "usb", feature = "uart")))]
compile_error!("enable one transport feature: `usb` or `uart`");

#[cfg(feature = "usb")]
pub mod usb;
#[cfg(feature = "usb")]
pub use usb::{setup, Reader, Writer};

#[cfg(feature = "uart")]
pub mod uart;
#[cfg(feature = "uart")]
pub use uart::{setup, Reader, Writer};
