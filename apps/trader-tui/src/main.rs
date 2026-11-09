//! Punto de entrada de TickForge.
//!
//! Este binario es el composition root: aquí, y solo aquí, se eligen las
//! implementaciones concretas de cada puerto y se cablean con la TUI.

fn main() {
    // Marcador temporal hasta implementar el arranque real (configuración, DI y bucle de la TUI)
    println!("TickForge {}", env!("CARGO_PKG_VERSION"));
}
