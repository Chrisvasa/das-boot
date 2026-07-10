//! Tells the linker to use cortex-m-rt's `link.x`. The `memory.x` it relies on
//! is provided automatically by embassy-stm32's `memory-x` feature, so there is
//! nothing chip-specific to maintain here.
fn main() {
    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    // Send defmt logs through the RTT control block.
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");
}
