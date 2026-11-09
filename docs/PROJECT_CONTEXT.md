# TickForge — Contexto del proyecto

> Documento vivo con las decisiones acordadas durante el diseño inicial (2026-11-09).
> Actualízalo cuando cambie una decisión; las decisiones grandes van además como ADR en `docs/adr/`.

## 1. Visión

TickForge es una **TUI (Terminal User Interface) en Rust** para operar acciones del mercado
de EE. UU. (TradFi): ver cotizaciones en tiempo real, consultar cuenta y posiciones, y colocar
o cancelar órdenes desde el teclado. La arquitectura debe permitir integrar en el futuro
**modelos de Machine Learning** para análisis predictivo y apoyo a la toma de decisiones.

Principios no negociables:

- Todo se desarrolla y prueba primero en **paper trading**. El modo real (LIVE) requiere una
  bandera explícita, confirmación doble y un banner rojo permanente en la interfaz.
- **Nunca `f64` para dinero, precios ni cantidades**: se usa `rust_decimal::Decimal`.
- Toda orden, venga del usuario o de un modelo, pasa por la **misma política de riesgo**.
- El modelo **propone**, la política de riesgo **decide**. Un modelo nunca llama al broker.

## 2. Convenciones de idioma

| Elemento | Idioma |
|---|---|
| Identificadores (archivos, carpetas, crates, módulos, structs, enums, funciones, variables, constantes) | **Inglés** estrictamente |
| Comentarios en código (`//`, `///`, `//!`) | **Español** |
| Mensajes de commit | Inglés (convención Conventional Commits) |
| Documentación (`docs/`, `CLAUDE.md`) | Español |

## 3. Proveedores y APIs

### Ejecución (brokers)

| Broker | Rol | Notas |
|---|---|---|
| **Alpaca** | Proveedor principal del MVP | REST + WebSocket, paper trading gratuito con la misma API que producción, `trade_updates` en tiempo real. Verificar disponibilidad para residentes de México. |
| **Interactive Brokers (IBKR)** | Segundo adaptador / operación real | Acepta residentes de México. Requiere TWS o IB Gateway local. Crate `ibapi` (protocolo TWS nativo en Rust). |
| Tradier / Charles Schwab | Alternativas | Solo si hay una razón concreta. |

Para cuentas de no residentes en EE. UU. se requiere el formulario **W-8BEN**.

### Datos de mercado

| Proveedor | Uso |
|---|---|
| **Alpaca Market Data** | MVP (feed IEX gratuito; SIP completo es de pago). |
| **Databento** | Histórico de alta calidad para ML; tiene cliente oficial en Rust (`databento`). |
| **Polygon.io (ahora Massive)** | Alternativa SIP completa e histórico amplio. |
| Finnhub / Twelve Data | Fundamentales y noticias como features de ML. |

IEX Cloud cerró en 2024: no usar.

### Crates de referencia

| Área | Crates |
|---|---|
| TUI | `ratatui`, `crossterm` |
| Async | `tokio`, `futures`, `tokio-stream`, `async-trait` |
| HTTP / WebSocket | `reqwest` (rustls), `tokio-tungstenite` |
| Serialización | `serde`, `serde_json` |
| Dinero | `rust_decimal` |
| Tiempo | `chrono` + `chrono-tz` (horario de Nueva York) |
| Errores | `thiserror` en librerías, `anyhow` solo en el binario |
| Configuración y secretos | `figment` o `config`, `secrecy`, `keyring` |
| Persistencia | `sqlx` (SQLite) |
| Observabilidad | `tracing`, `tracing-subscriber`, `tracing-appender` (logs a archivo: la TUI ocupa stdout) |
| Pruebas | `mockall`, `wiremock`, `insta`, `proptest` |
| ML | `ort` (ONNX Runtime), `candle`, `linfa`, `polars`, `tonic` (gRPC) |

## 4. Fases de desarrollo

| Fase | Objetivo | Entregable |
|---|---|---|
| 0. Fundamentos | Workspace, CI (`fmt`, `clippy -D warnings`, `test`, `cargo-deny`), configuración, logging a archivo | Esqueleto que compila ✅ (workspace + CI; logging pasa a la fase 4) |
| 1. Dominio | `Order`, `Position`, `Quote`, `Money`, `Price`, `Quantity`, máquina de estados de órdenes. Sin I/O | Dominio con pruebas unitarias y de propiedades |
| 2. Aplicación | Puertos (traits) y casos de uso, probados con mocks y el broker simulado | Casos de uso probados sin red |
| 3. Adaptador Alpaca | REST (cuenta, posiciones, órdenes), WebSocket de datos y `trade_updates`, mapeo wire ⇄ dominio | Integración con paper trading |
| 4. TUI básica | Bucle de eventos, layout, watchlist en vivo, barra de estado | Pantalla funcional |
| 5. Órdenes | Ticket, vista previa, modal de confirmación, riesgo pre-trade, cancelación | Comprar/vender en paper |
| 6. Persistencia | Diario de órdenes, watchlists, reconciliación al iniciar | Auditoría y recuperación |
| 7. Robustez | Reconexión con backoff, rate limits, idempotencia (`client_order_id`), apagado limpio, kill switch, panic hook | Listo para modo real |
| 8. Modo real | Bandera explícita + doble confirmación + banner LIVE | v1.0 |
| 9. ML | Grabar datos → entrenar offline → exportar ONNX → modo sombra → asesor → (opcional) autónomo | Señales predictivas |

## 5. Alcance del MVP

### Incluido

1. **Autenticación y modo**: llaves desde variables de entorno o `keyring`; indicador permanente PAPER / LIVE.
2. **Resumen de cuenta**: equity, efectivo, poder de compra, P&L del día.
3. **Watchlist en tiempo real**: bid, ask, último, cambio %, volumen; agregar y quitar símbolos.
4. **Detalle de símbolo**: velas o sparkline (1m / 5m / 1D) y datos básicos.
5. **Ticket de orden**: compra/venta, `Market` / `Limit`, cantidad, TIF (`Day` / `GTC`), vista previa con costo estimado y modal de confirmación.
6. **Órdenes abiertas**: estado en tiempo real y cancelación con una tecla.
7. **Posiciones**: cantidad, precio promedio, valor de mercado, P&L no realizado.
8. **Reloj de mercado**: abierto/cerrado, próxima apertura, hora de Nueva York.
9. **Riesgo pre-trade**: nocional máximo por orden, posición máxima por símbolo, control fat-finger (desviación máxima del precio límite respecto a la cotización), saldo suficiente.
10. **Notificaciones y log**: fills, rechazos, desconexiones.
11. **Uso 100 % con teclado**: atajos estilo vim y ayuda con `?`.

### Fuera del MVP

Opciones, ventas en corto, órdenes bracket/OCO, extended hours, multi-broker simultáneo,
backtesting, ML, alertas de precio, Nivel 2.

## 6. Arquitectura

### Estilo

- **Clean Architecture + DDD + puertos y adaptadores (hexagonal)**, implementada como
  **workspace de Cargo con varios crates** para que el compilador haga cumplir la regla de dependencias.
- **TUI unidireccional (estilo Elm)**: `AppEvent → Action → reducer → AppState → render`.
- **ML como un adaptador más**, detrás del puerto `SignalGenerator`.

### Regla de dependencias

```
                ┌──────────────────────┐
                │  apps/trader-tui     │  ← composition root (DI)
                └─────────┬────────────┘
        ┌─────────────────┼───────────────────────┐
        ▼                 ▼                       ▼
 presentation-tui     adapters/*              ml-features
        │           (alpaca, ibkr, sqlite,         │
        │            onnx, paper-sim)              │
        └────────┬────────┘                        │
                 ▼                                 │
            application  (ports + use cases)       │
                 │                                 │
                 ▼                                 ▼
               domain  ◄───────────────────────────┘
          (sin async, sin I/O)
```

- `domain` no depende de ningún crate interno ni de crates de I/O o async.
- `application` depende solo de `domain`.
- `presentation-tui` y `adapters/*` dependen de `application` y `domain`, **nunca entre sí**.
- `apps/trader-tui` es el único lugar donde se eligen implementaciones concretas.

### Crates (nombres de paquete)

| Carpeta | Paquete | Estado |
|---|---|---|
| `apps/trader-tui` | `tickforge` (binario) | creado |
| `crates/domain` | `tickforge-domain` | creado |
| `crates/application` | `tickforge-application` | creado |
| `crates/presentation-tui` | `tickforge-presentation-tui` | creado |
| `crates/adapters/broker-paper-sim` | `tickforge-adapter-paper-sim` | creado |
| `crates/adapters/broker-alpaca` | `tickforge-adapter-alpaca` | pendiente (fase 3) |
| `crates/adapters/persistence-sqlite` | `tickforge-adapter-sqlite` | pendiente (fase 6) |
| `crates/adapters/broker-ibkr` | `tickforge-adapter-ibkr` | futuro |
| `crates/adapters/ml-onnx` | `tickforge-adapter-onnx` | futuro (fase 9) |
| `crates/ml-features` | `tickforge-ml-features` | futuro (fase 9) |
| `crates/telemetry` | `tickforge-telemetry` | pendiente (fase 0/4) |

En código Rust los crates se importan como `tickforge_domain`, `tickforge_application`, etc.

### Estructura de directorios objetivo

```
tickforge/
├── Cargo.toml                         # workspace virtual (resolver = "3", edición 2024)
├── rust-toolchain.toml
├── clippy.toml / rustfmt.toml
├── deny.toml                          # auditoría de dependencias (cargo-deny)
├── .gitattributes                     # saltos de línea LF
├── .env.example
├── CLAUDE.md
├── config/ (default.toml, paper.toml, live.toml)
├── docs/
│   ├── PROJECT_CONTEXT.md
│   └── adr/
├── .github/workflows/ci.yml           # fmt, clippy, test (Linux + Windows), cargo-deny
├── apps/
│   └── trader-tui/src/
│       ├── main.rs
│       ├── bootstrap.rs               # inyección de dependencias
│       ├── settings.rs
│       └── shutdown.rs
├── crates/
│   ├── domain/src/
│   │   ├── error.rs
│   │   ├── shared/   (money.rs, price.rs, quantity.rs, symbol.rs)
│   │   ├── trading/  (order.rs, order_id.rs, order_request.rs, order_status.rs,
│   │   │              order_side.rs, order_type.rs, time_in_force.rs, fill.rs, events.rs)
│   │   ├── portfolio/ (account.rs, position.rs)
│   │   ├── market/   (quote.rs, trade_tick.rs, bar.rs, timeframe.rs, market_clock.rs)
│   │   ├── risk/     (risk_policy.rs, risk_limits.rs, risk_violation.rs)
│   │   └── signals/  (signal.rs, signal_direction.rs, confidence.rs)
│   ├── application/
│   │   ├── src/
│   │   │   ├── error.rs
│   │   │   ├── ports/ (broker_gateway.rs, market_data_feed.rs, order_journal.rs,
│   │   │   │           watchlist_repository.rs, signal_generator.rs, event_bus.rs, clock.rs)
│   │   │   ├── use_cases/
│   │   │   │   ├── trading/   (place_order.rs, cancel_order.rs, list_open_orders.rs)
│   │   │   │   ├── portfolio/ (get_account_summary.rs, get_positions.rs)
│   │   │   │   ├── market/    (stream_quotes.rs, get_bars.rs)
│   │   │   │   └── signals/   (evaluate_signals.rs)
│   │   │   ├── services/ (order_reconciler.rs, decision_engine.rs)
│   │   │   └── dto/
│   │   └── tests/
│   ├── adapters/
│   │   ├── broker-alpaca/src/ (config.rs, rest_client.rs, stream_client.rs, wire/,
│   │   │                       mapping.rs, gateway.rs, market_data.rs)
│   │   ├── broker-ibkr/
│   │   ├── broker-paper-sim/
│   │   ├── persistence-sqlite/ (migrations/, src/repositories/)
│   │   └── ml-onnx/src/ (onnx_signal_generator.rs, model_registry.rs)
│   ├── ml-features/src/ (pipeline.rs, window.rs, indicators/{sma,ema,rsi,atr,vwap}.rs)
│   ├── presentation-tui/src/
│   │   ├── app.rs, terminal.rs, event.rs, action.rs, command.rs, keymap.rs, theme.rs
│   │   ├── state/ (app_state.rs, reducer.rs)
│   │   ├── screens/ (dashboard.rs, symbol_detail.rs, orders.rs)
│   │   └── components/ (watchlist.rs, quote_panel.rs, price_chart.rs, order_ticket.rs,
│   │                    orders_table.rs, positions_table.rs, account_bar.rs,
│   │                    confirm_dialog.rs, notifications.rs, help_overlay.rs)
│   └── telemetry/
└── ml/                                # Python, fuera del workspace de Rust
    ├── pyproject.toml
    ├── data/                          # Parquet grabado desde Rust (no versionado)
    ├── training/ (train_direction_model.py, export_onnx.py)
    └── models/ (direction_v1.onnx, direction_v1.metadata.json)
```

### Puertos principales (borrador)

```rust
#[async_trait]
pub trait BrokerGateway: Send + Sync {
    async fn account(&self) -> Result<Account, BrokerError>;
    async fn positions(&self) -> Result<Vec<Position>, BrokerError>;
    async fn open_orders(&self) -> Result<Vec<Order>, BrokerError>;
    async fn submit_order(&self, request: &OrderRequest) -> Result<Order, BrokerError>;
    async fn cancel_order(&self, order_id: &OrderId) -> Result<(), BrokerError>;
    async fn order_updates(&self) -> Result<BoxStream<'static, OrderUpdate>, BrokerError>;
}

#[async_trait]
pub trait MarketDataFeed: Send + Sync {
    async fn subscribe(&self, symbols: &[Symbol])
        -> Result<BoxStream<'static, MarketEvent>, MarketDataError>;
    async fn latest_quote(&self, symbol: &Symbol) -> Result<Quote, MarketDataError>;
    async fn bars(&self, symbol: &Symbol, timeframe: Timeframe, limit: usize)
        -> Result<Vec<Bar>, MarketDataError>;
}

#[async_trait]
pub trait SignalGenerator: Send + Sync {
    fn model_info(&self) -> ModelInfo;
    async fn generate(&self, context: &SignalContext) -> Result<Option<Signal>, SignalError>;
}
```

### Flujo del caso de uso `PlaceOrderUseCase`

1. Convertir el comando en un `OrderRequest` válido para el dominio.
2. Reunir cuenta, posiciones y cotización (en paralelo).
3. Evaluar `RiskPolicy`; si falla, la orden nunca sale del sistema.
4. Registrar la intención en el diario (auditoría e idempotencia).
5. Enviar al broker, registrar el resultado y publicar `DomainEvent::OrderSubmitted`.

### Concurrencia de la TUI

```
[crossterm input] ─┐
[tick 250ms]      ─┤
[MarketDataFeed]  ─┼──► mpsc<AppEvent> ──► reducer(State, Action) ──► render (máx. ~30 FPS)
[order_updates]   ─┤                              │
[SignalGenerator] ─┘                              ▼
                                         mpsc<Command> ──► worker ──► casos de uso
                                                                  └──► resultado como AppEvent
```

- El render nunca espera a la red; los efectos secundarios se ejecutan como `Command` en otra tarea.
- El render se limita a una tasa fija; no se redibuja con cada cotización.

## 7. Integración futura de ML

1. **Mismas features para entrenamiento e inferencia**: `ml-features` es Rust puro; se expone a
   Python con PyO3/maturin o se graban las features en Parquet desde Rust (evita training-serving skew).
2. **Entrenar en Python** (scikit-learn, LightGBM, PyTorch) → **exportar a ONNX** → **inferir en Rust** con `ort`.
   Alternativa para modelos pesados: servicio remoto gRPC (`tonic`) que implemente el mismo `SignalGenerator`.
3. **Modos progresivos** (`enum SignalMode`): `Off` → `Shadow` (solo registra) → `Advisory`
   (se muestra en la TUI) → `Autonomous` (genera órdenes vía `PlaceOrderUseCase` + `RiskPolicy`
   con límites más estrictos y kill switch).
4. **Desacoplamiento por eventos**: el generador se suscribe al `EventBus` (`tokio::sync::broadcast`)
   y publica `DomainEvent::SignalGenerated`; ni la TUI ni el broker conocen el modelo.
5. **Trazabilidad**: cada `Signal` lleva `model_id`, versión y marca de tiempo; cada orden autónoma
   guarda la señal que la originó.

## 8. Estado actual

- [x] Workspace de Cargo creado (edición 2024, resolver 3, toolchain 1.94.0 fijado).
- [x] Crates base: `domain`, `application`, `presentation-tui`, `broker-paper-sim`, binario `tickforge`.
- [x] Lints del workspace (`unsafe_code = "forbid"`, clippy contra `unwrap`/`expect`/`panic`/aritmética flotante).
- [x] `.gitignore`, `.env.example`, `CLAUDE.md`, este documento.
- [x] Repositorio público en GitHub: https://github.com/zudno/tickforge
- [x] `.gitattributes` (saltos de línea LF en todo el repositorio).
- [x] `deny.toml` (licencias permisivas, vulnerabilidades, solo crates.io).
- [x] CI en GitHub Actions: `fmt`, `clippy -D warnings` y `test` en Linux y Windows, más `cargo-deny`.
- [ ] Crate `telemetry` y logging a archivo (se hará junto con la TUI, fase 4).
- [ ] Elegir licencia del proyecto (el repo es público y aún no tiene `LICENSE`).

### Fase 1 — Dominio

- [x] `DomainError` (`thiserror`, mensajes en español).
- [x] Value objects de `shared/`: `Symbol`, `Price`, `Quantity`, `Money` + `Currency`.
- [ ] `trading/`: enums de orden, `OrderStatus` (máquina de estados), ids, `OrderRequest`, `Fill`, agregado `Order`.
- [ ] `portfolio/`: `Account`, `Position`.
- [ ] `market/`: `Quote`, `Bar`, `Timeframe`, `MarketClock`.
- [ ] `risk/`: `RiskLimits`, `RiskPolicy`, `RiskViolation`.

## 9. Decisiones de modelado del dominio

| Value object | Regla | Motivo |
|---|---|---|
| `Symbol` | Mayúsculas, 1–10 caracteres, empieza con letra, solo `A-Z 0-9 . -`, no termina en separador | Cubre tickers con clase (`BRK.B`, `BF-B`). El formato propio de cada broker (`BRK B` en IBKR) lo traduce el adaptador. |
| `Price` | Estrictamente positivo, máximo 4 decimales, normalizado | Las acciones bajo 1 USD cotizan en 0.0001. El tick mínimo por rango de precio (Regla 612 de Reg NMS) se valida al crear la orden, no aquí. |
| `Quantity` | No negativa, máximo 9 decimales (fraccionadas) | El cero es un estado válido (cantidad llenada inicial). Que una orden pida más de cero lo valida `OrderRequest`. Las ventas en corto quedan fuera del MVP. |
| `Money` | Puede ser negativo, precisión completa en cálculos, redondeo bancario solo al mostrar o reportar | Sirve para P&L. Operar monedas distintas es error (`CurrencyMismatch`). Solo existe `Currency::Usd` por ahora. |

- Toda la aritmética usa operaciones `checked_*`: un desbordamiento devuelve `DomainError::ArithmeticOverflow` en lugar de provocar un `panic`.
- La igualdad de `Decimal` es numérica: `1.5 == 1.50`.
- [ ] Fase 1: modelado del dominio.
