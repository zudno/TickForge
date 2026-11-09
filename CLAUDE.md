# CLAUDE.md — TickForge

TUI en Rust para operar acciones de EE. UU. (Alpaca primero, IBKR después), con arquitectura
hexagonal preparada para integrar modelos de ML. El contexto completo y las decisiones de diseño
están en [`docs/PROJECT_CONTEXT.md`](docs/PROJECT_CONTEXT.md): léelo antes de cualquier cambio
de arquitectura y actualízalo cuando una decisión cambie.

## Reglas de idioma (obligatorias)

- **Identificadores en inglés, sin excepción**: archivos, carpetas, crates, módulos, structs,
  enums, variantes, traits, funciones, variables, constantes, campos y nombres de pruebas.
- **Comentarios en español**: `//`, `///` y `//!`.
- Mensajes de error al usuario final (texto visible en la TUI): español.
- Mensajes de commit: inglés, formato Conventional Commits (`feat(domain): add Price value object`).

## Arquitectura y regla de dependencias

Workspace de Cargo con un crate por capa. El compilador hace cumplir las dependencias:

| Crate (paquete) | Puede depender de |
|---|---|
| `tickforge-domain` (`crates/domain`) | Nada interno. Solo crates puros (`rust_decimal`, `chrono`, `thiserror`). **Prohibido** `tokio`, `reqwest`, `serde_json`, `sqlx`, `ratatui` o cualquier I/O. |
| `tickforge-application` (`crates/application`) | `domain`. Puede usar `async-trait`, `futures`, `thiserror`. Sin clientes HTTP ni base de datos. |
| `tickforge-adapter-*` (`crates/adapters/*`) | `application`, `domain`. **Nunca** otro adaptador ni `presentation-tui`. |
| `tickforge-presentation-tui` (`crates/presentation-tui`) | `application`, `domain`. **Nunca** un adaptador. |
| `tickforge` (`apps/trader-tui`) | Todos. Es el único composition root: aquí se eligen las implementaciones concretas. |

- Los puertos (traits) viven en `crates/application/src/ports/`; los adaptadores los implementan.
- Los structs de la API externa (wire models) viven en `wire/` dentro de cada adaptador y se
  traducen al dominio en `mapping.rs`. Nunca exponer tipos wire fuera del adaptador.
- Nuevo crate: agregarlo a `members` y a `[workspace.dependencies]` del `Cargo.toml` raíz,
  heredar `version/edition/rust-version/publish` del workspace y declarar `[lints] workspace = true`.
- Dependencias externas: declarar la versión una sola vez en `[workspace.dependencies]` y usar
  `dep.workspace = true` en cada crate.

## Reglas de dominio y trading (seguridad)

- **Dinero, precios y cantidades con `rust_decimal::Decimal`**, envueltos en value objects
  (`Price`, `Quantity`, `Money`). Nunca `f32`/`f64` (clippy `float_arithmetic` lo vigila).
- Los value objects validan en su constructor (`new(...) -> Result<Self, DomainError>`); un valor
  inválido no debe poder existir.
- Las transiciones de estado de una orden pasan por `OrderStatus::can_transition_to`.
- **Toda orden pasa por `PlaceOrderUseCase` y `RiskPolicy`**, venga del usuario o de un modelo de ML.
  Ningún código fuera de ese flujo llama a `BrokerGateway::submit_order`.
- Cada orden lleva un `client_order_id` único para idempotencia.
- El modo por defecto es **paper**. No cambiar defaults ni configuraciones hacia modo real (LIVE),
  ni usar llaves reales, sin que el usuario lo pida explícitamente.
- Nunca escribir llaves de API en código, pruebas, logs ni documentación; se leen de `.env`
  (no versionado) o del `keyring`. `.env.example` solo contiene nombres de variables vacías.

## Estilo de código

- Errores: `thiserror` en crates de librería; `anyhow` solo en `apps/trader-tui`.
- Prohibido `unwrap()`, `expect()` y `panic!()` fuera de pruebas; propagar con `?` y errores tipados.
- `unsafe` está prohibido en todo el workspace (`unsafe_code = "forbid"`).
- Logging con `tracing`, siempre a archivo: la TUI ocupa stdout. Nunca `println!` en librerías.
- Funciones de dominio puras y deterministas; el tiempo se inyecta mediante el puerto `Clock`.
- Un tipo principal por archivo, nombrado en `snake_case` igual que el tipo (`order_status.rs` → `OrderStatus`).
- **Mostrar decimales**: `format!("{:.2}", decimal)` **trunca** en `rust_decimal`. Redondear siempre
  antes de formatear; en la TUI usar las funciones de `crates/presentation-tui/src/format.rs`.
- TUI: el estado solo cambia en el reducer (`state/reducer.rs`); el render no tiene estado propio
  y la red nunca se llama desde el bucle de dibujo (se piden `Command`).

## Pruebas

- Dominio: pruebas unitarias en el mismo archivo (`#[cfg(test)] mod tests`) y `proptest` para invariantes.
- Casos de uso: en `crates/application/tests/`, con `mockall` o el broker simulado
  (`broker-paper-sim`). Nunca pegarle a la red en pruebas.
- Adaptadores HTTP: `wiremock` con respuestas grabadas de la API.
- TUI: snapshots con `insta` sobre `ratatui::backend::TestBackend`.
- Archivos de pruebas de integración (`tests/*.rs` y `tests/common/mod.rs`): clippy no reconoce
  las funciones auxiliares sin `#[test]` como pruebas, así que cada archivo raíz de prueba lleva
  `#![allow(clippy::unwrap_used, clippy::expect_used)]` con un comentario. Nunca en código de `src/`.
- Pruebas contra servicios reales (por ejemplo `paper_smoke`): siempre `#[ignore]`, solo lectura,
  con salvaguarda de entorno paper. Se ejecutan a mano:
  `cargo test -p tickforge-adapter-alpaca --test paper_smoke -- --ignored --nocapture`.
- Única excepción de escritura: `paper_orders_smoke` coloca una compra límite de 1 acción muy
  por debajo del mercado y la cancela de inmediato, solo en paper. Cualquier prueba nueva que
  escriba en la cuenta debe seguir esas mismas garantías (no ejecutable, cancelada, paper).

## Comandos

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo run -p tickforge
```

Antes de dar una tarea por terminada: `cargo fmt --all`, `cargo clippy ... -D warnings` y
`cargo test --workspace` deben pasar sin errores. Al agregar dependencias, `cargo deny check`
también (si una licencia nueva es necesaria, agregarla a `deny.toml` con justificación).

## Flujo de Git

- Nunca commitear directo a `main`: rama por tarea (`feat/...`, `fix/...`, `chore/...`) y PR.
- El CI (`.github/workflows/ci.yml`) corre `fmt`, `clippy`, `test` (Linux y Windows) y `cargo-deny`;
  un PR solo se fusiona con el CI en verde.
- Saltos de línea: LF en todo el repositorio (`.gitattributes`).
- Scripts que verifican y luego commitean: empezar con `set -euo pipefail` y **no mezclar `;` con
  `&&`**. Si una verificación falla a mitad de cadena, un `git commit` posterior separado por `;`
  commitea solo lo que ya estaba en el stage (pasó dos veces). Antes de commitear, comprobar que
  ni `.env`, ni `logs/`, ni `target/` estén en el stage.

## Entorno

- Windows 11, toolchain fijado en `rust-toolchain.toml` (1.94.0, edición 2024, resolver 3).
- MSRV (`rust-version` del workspace): 1.88. Con `resolver = "3"` Cargo elige versiones compatibles
  con el MSRV declarado; si una corrección de seguridad exige un Rust más nuevo, hay que subirlo.
- La carpeta `ml/` (Python) queda fuera del workspace de Rust.
