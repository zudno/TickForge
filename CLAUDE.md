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

## Pruebas

- Dominio: pruebas unitarias en el mismo archivo (`#[cfg(test)] mod tests`) y `proptest` para invariantes.
- Casos de uso: en `crates/application/tests/`, con `mockall` o el broker simulado
  (`broker-paper-sim`). Nunca pegarle a la red en pruebas.
- Adaptadores HTTP: `wiremock` con respuestas grabadas de la API.
- TUI: snapshots con `insta` sobre `ratatui::backend::TestBackend`.

## Comandos

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p tickforge
```

Antes de dar una tarea por terminada: `cargo fmt --all`, `cargo clippy ... -D warnings` y
`cargo test --workspace` deben pasar sin errores.

## Entorno

- Windows 11, toolchain fijado en `rust-toolchain.toml` (1.94.0, edición 2024, resolver 3).
- La carpeta `ml/` (Python) queda fuera del workspace de Rust.
