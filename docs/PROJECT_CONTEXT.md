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
| 1. Dominio | `Order`, `Position`, `Quote`, `Money`, `Price`, `Quantity`, máquina de estados de órdenes. Sin I/O | Dominio con pruebas unitarias y de propiedades ✅ |
| 2. Aplicación | Puertos (traits) y casos de uso, probados con mocks y el broker simulado | Casos de uso probados sin red ✅ |
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
- [x] `trading/`: enums de orden, `OrderStatus` (máquina de estados), ids, `OrderRequest`, `Fill`, agregado `Order`, `OrderEvent`.
- [x] `portfolio/`: `Account` (+ `AccountBalances`), `Position`.
- [x] `market/`: `Quote`, `TradeTick`, `Bar` (+ `Ohlc`), `Timeframe`, `MarketClock`.
- [x] `risk/`: `RiskLimits`, `RiskPolicy` (+ `RiskContext`, `RiskAssessment`), `RiskViolation`.

**Fase 1 completa.**

### Fase 2 — Aplicación

- [x] Puertos: `BrokerGateway` (+ `OrderUpdate`), `MarketDataFeed` (+ `MarketEvent`), `OrderJournal`, `Clock`, `ClientOrderIdGenerator`, `EventPublisher`.
- [x] Casos de uso: `PlaceOrderUseCase`, `CancelOrderUseCase`, `GetPortfolioSnapshotUseCase`.
- [x] Simulador en memoria (`broker-paper-sim`): `PaperBroker`, `ManualClock`, `InMemoryOrderJournal`, `RecordingEventPublisher`, `SequentialClientOrderIdGenerator`.
- [x] Pruebas de integración de los casos de uso contra el simulador.
- [ ] Pendientes para fases posteriores: `WatchlistRepository` (fase 6), `SignalGenerator` (fase 9), reintentos con backoff (fase 7).

**Fase 2 completa.**

### Fase 3 — Adaptador de Alpaca

- [x] REST: `AlpacaBrokerGateway` (cuenta, posiciones, órdenes, envío idempotente, cancelación, reloj de mercado).
- [x] REST: `AlpacaMarketData` (última cotización, velas históricas).
- [x] Capa anticorrupción `wire/` → `mapping/` con validación completa.
- [x] Pruebas con servidor HTTP falso (`wiremock`) y prueba de humo opcional contra paper trading.
- [x] WebSocket: `trade_updates` (actualizaciones de órdenes) y flujo de cotizaciones, operaciones y velas de 1 minuto en tiempo real (verificado contra paper trading).
- [ ] Reconexión automática con backoff y reconciliación por REST tras un corte (fase 7).

**Fase 3 completa.** Siguiente: fase 4, TUI básica.

## 9. Decisiones de modelado del dominio

| Value object | Regla | Motivo |
|---|---|---|
| `Symbol` | Mayúsculas, 1–10 caracteres, empieza con letra, solo `A-Z 0-9 . -`, no termina en separador | Cubre tickers con clase (`BRK.B`, `BF-B`). El formato propio de cada broker (`BRK B` en IBKR) lo traduce el adaptador. |
| `Price` | Estrictamente positivo, máximo 4 decimales, normalizado | Las acciones bajo 1 USD cotizan en 0.0001. El tick mínimo por rango de precio (Regla 612 de Reg NMS) se valida al crear la orden, no aquí. |
| `Quantity` | No negativa, máximo 9 decimales (fraccionadas) | El cero es un estado válido (cantidad llenada inicial). Que una orden pida más de cero lo valida `OrderRequest`. Las ventas en corto quedan fuera del MVP. |
| `Money` | Puede ser negativo, precisión completa en cálculos, redondeo bancario solo al mostrar o reportar | Sirve para P&L. Operar monedas distintas es error (`CurrencyMismatch`). Solo existe `Currency::Usd` por ahora. |

- Toda la aritmética usa operaciones `checked_*`: un desbordamiento devuelve `DomainError::ArithmeticOverflow` en lugar de provocar un `panic`.
- La igualdad de `Decimal` es numérica: `1.5 == 1.50`.
- El dominio usa `chrono` **sin** la feature `clock`: no puede leer la hora del sistema. Toda marca
  de tiempo llega como parámetro (en la aplicación vendrá del puerto `Clock`), lo que hace las
  pruebas deterministas.

### Subdominio de trading

| Concepto | Decisión |
|---|---|
| `OrderRequest` vs `Order` | `OrderRequest` es la intención validada (lo que evalúa el riesgo). `Order` existe solo cuando el broker asigna un `OrderId`. |
| `ClientOrderId` | Lo genera TickForge (puerto de la capa de aplicación) para idempotencia en reintentos. Formato: `A-Z a-z 0-9 - _`, máximo 48 caracteres. |
| `OrderId` / `FillId` | Opacos (UUID en Alpaca, entero en IBKR): solo se valida ASCII visible, máximo 128 caracteres. |
| Tick mínimo | Regla 612 de Reg NMS: 0.01 desde 1 USD, 0.0001 por debajo. Los incrementos de 0.005 de la enmienda de 2024 no están modelados (se rechazan de forma conservadora). |
| Máquina de estados | `match` exhaustivo en `OrderStatus::can_transition_to`. Se permiten saltos (`PendingNew -> Filled`) porque el broker puede agrupar o perder eventos. Solo `PendingNew` puede pasar a `Rejected`. |
| Cancelación | `PendingCancel` es un estado propio: una ejecución parcial no lo cambia, una ejecución total gana la carrera (`Filled`) y un rechazo de la cancelación regresa a `New` o `PartiallyFilled`. |
| Ejecuciones | Se acumula el nocional (`precio * cantidad`) y el precio promedio se deriva con precisión completa. Un `FillId` repetido se ignora (`Ok(None)`): reprocesar eventos es seguro. |
| Atomicidad | Si un comando del agregado falla, el `Order` queda exactamente igual (verificado con `proptest`). |
| `Order::restore` | Reconstruye una orden desde el estado del broker (`OrderSnapshot`) y valida coherencia entre estado, cantidad llenada y precio promedio. |
| Tipos de orden | MVP: `Market` y `Limit`; `TimeInForce`: `Day` y `Gtc`. Reglas propias de cada broker (por ejemplo, fraccionadas solo con `Day` en Alpaca) se validan en el adaptador. |

### Subdominio de portafolio

| Concepto | Decisión |
|---|---|
| `Account` | Saldos del broker (`equity`, `cash`, `buying_power`, `last_equity`) en una sola moneda. `cash` puede ser negativo (margen); `buying_power` no. P&L del día = `equity - last_equity`. |
| `Position` | Solo posiciones largas. Guarda el costo total, no el precio promedio, para no acumular redondeos. Ventas por costo promedio; cerrar por completo deja el costo exactamente en cero. Vender más de lo que se tiene es error. |

### Subdominio de mercado

| Concepto | Decisión |
|---|---|
| `Quote` | `bid` y `ask` opcionales (puede faltar un lado fuera de horario). Se aceptan cotizaciones cruzadas y se exponen con `is_crossed`. Precio ejecutable: `ask` para comprar, `bid` para vender. |
| `Bar` | Valida que el máximo y el mínimo contengan a la apertura y al cierre. Incluye símbolo y `Timeframe` (`1m`, `5m`, `15m`, `1h`, `1d`). |
| `MarketClock` | Lo calcula el broker (feriados y cierres anticipados); el dominio solo valida coherencia entre el estado abierto/cerrado y el próximo evento. |

### Subdominio de riesgo

| Concepto | Decisión |
|---|---|
| Resultado | `RiskAssessment` con **todas** las violaciones, para que la TUI muestre todo lo que hay que corregir de una vez. `Err` se reserva para datos incoherentes (por ejemplo, cotización de otro símbolo). |
| Reglas | Cuenta bloqueada, cotización vieja, sin precio ejecutable, valor máximo por orden, poder de compra, valor máximo de la posición resultante, venta mayor a lo que se tiene (sin cortos) y fat-finger. |
| Precio de referencia | El límite si existe; si no, el precio ejecutable (`ask`/`bid`). Una orden a mercado sin precio ejecutable se bloquea. |
| Fat-finger | Desviación del precio límite respecto al precio ejecutable, en ambas direcciones. |
| Límites | `RiskLimits` valida que todos sean positivos; se cargarán desde la configuración. |

## 10. Decisiones de la capa de aplicación

| Concepto | Decisión |
|---|---|
| Puertos asíncronos | `async-trait` para poder usar `Arc<dyn Trait>` (los `async fn` nativos en traits aún no son compatibles con `dyn`). `Clock`, `ClientOrderIdGenerator` y `EventPublisher` son síncronos. |
| `TradingPorts` | Agrupa los puertos que comparten los casos de uso de trading; evita constructores largos y deja la elección de implementaciones al composition root. |
| Flujo de `PlaceOrderUseCase` | id de cliente → `OrderRequest` → cuenta, posición y cotización en paralelo (`try_join!`) → `RiskPolicy` → diario `Intent` → broker → diario `Submitted` → evento `OrderSubmitted`. |
| Diario estricto antes de enviar | Si no se puede registrar la intención, **no se envía** la orden. |
| Diario tolerante después de enviar | Si el broker ya aceptó la orden y el diario falla, se devuelve la orden y se publica `JournalWriteFailed`: ocultar una orden viva sería peor. La reconciliación (fase 6) corrige el diario. |
| Cancelación | `CancelOrderUseCase` consulta la orden y deja que el agregado (`Order::request_cancel`) decida si es cancelable antes de llamar al broker. |
| `OrderUpdate::apply_to` | Traduce cada actualización del broker al comando del agregado; con esto la TUI mantiene su copia de las órdenes solo a partir del flujo de eventos. Es **idempotente**: si la orden ya refleja la actualización (por ejemplo, `new` llega por REST y por WebSocket, o una ejecución sobre una orden ya completa) devuelve `Ok(None)`. Los conflictos reales (cancelada → expirada) siguen siendo error. Limitación: una orden reconstruida por REST no conoce los ids de sus ejecuciones previas; lo resuelve la reconciliación (fase 6). |
| `ClientOrderIdGenerator` falible | Devuelve `Result` para que ninguna implementación necesite `unwrap` al validar el formato. |
| Simulador (`PaperBroker`) | Cuenta de efectivo sin margen, solo largos. Órdenes a mercado se llenan al `ask`/`bid`; límites cuando la cotización las vuelve ejecutables, al precio de mercado. Llenados siempre totales. Idempotente por `ClientOrderId`. `fail_next_submit` simula caídas. |
| Pruebas de casos de uso | En `crates/application/tests/` contra el simulador (dependencia solo de desarrollo). Las utilidades de `tests/common/` permiten `unwrap` explícitamente porque clippy no las reconoce como pruebas. |

## 11. Decisiones del adaptador de Alpaca

| Concepto | Decisión |
|---|---|
| Entornos | `AlpacaConfig::paper` es el único constructor de conveniencia. Para `Live` hay que armar la configuración a mano en el composition root (decisión explícita). |
| Credenciales | `secrecy::SecretString` (no aparecen en `Debug`) y encabezados HTTP marcados como sensibles. Se leen de `.env`, nunca del código. |
| TLS | `reqwest` con `rustls`: sin depender de OpenSSL del sistema (importante en Windows). |
| Números | La Trading API manda texto (`"100.25"`) y la Market Data API números JSON; ambos se convierten a `Decimal` sin pasar por aritmética flotante. |
| Estados de orden | Los estados "vivos" de Alpaca (`new`, `partially_filled`, `held`, `done_for_day`, …) se derivan de la cantidad llenada para tolerar respuestas cruzadas. `accepted`/`pending_new` → `PendingNew`; `replaced` → `Canceled`. Estado desconocido → error. |
| No soportado (error explícito) | Órdenes por monto (`notional`), tipos distintos de `market`/`limit`, vigencias distintas de `day`/`gtc`, posiciones en corto. Si existen en la cuenta (por ejemplo, creadas desde la web), la consulta falla con un mensaje claro en lugar de ignorarlas. |
| Idempotencia | Si `POST /v2/orders` responde 422 `client_order_id must be unique`, se recupera la orden existente con `GET /v2/orders:by_client_order_id`. |
| Errores HTTP | 401 → `Unauthorized`; 429 → `RateLimited`; 5xx y fallas de red → `Unavailable` (reintentables); 403/422 → `Rejected` con el mensaje de Alpaca; 404 → `None` / `OrderNotFound` / `NoData` según el caso. |
| Cotizaciones | Precio `0` significa que ese lado no tiene ofertas → `None`. Feed configurable (`iex` gratuito por defecto, `sip` de pago). |
| Velas | Se piden en orden descendente con `limit` (una sola página) y una ventana de inicio holgada que cubre noches, fines de semana y feriados; se devuelven en orden ascendente. |
| WebSocket de trading | `wss://paper-api.alpaca.markets/stream`: `auth` → `authorization: authorized` → `listen trade_updates` → `listening`. Alpaca responde en **frames binarios** con JSON; se aceptan texto y binario. Eventos: `new` → `Accepted`; `fill`/`partial_fill` → `Filled` (con `execution_id`, precio y cantidad de esa ejecución); `canceled`/`replaced` → `Canceled`; `expired`; `rejected`; `order_cancel_rejected`. El resto (`pending_new`, `accepted`, `done_for_day`, …) no cambia el dominio y se ignora. |
| WebSocket de datos | `wss://stream.data.alpaca.markets/v2/{feed}`: `connected` → `auth` → `authenticated` → `subscribe` (quotes, trades, bars) → `subscription`. Cada mensaje es un arreglo; se despacha por el campo `T` (`q`, `t`, `b`) y lo demás se ignora. Errores: 401/402 → `Unauthorized`; 406 → límite de conexiones (**el plan gratuito permite una sola conexión de datos**: suscribir todos los símbolos en una llamada); 409 → feed no incluido en el plan. |
| Saludo con tiempo límite | Conexión y cada respuesta del saludo con 10 s de espera máxima y un tope de mensajes; así un servidor mudo nunca bloquea la TUI. |
| Datos inválidos en vivo | Se descartan sin cortar el flujo (el tipo del flujo no admite errores por elemento). |
| Fin del flujo | Si se cierra la conexión, el flujo termina. Reconectar con backoff y reconciliar lo ocurrido es responsabilidad del consumidor (fase 7). |
| TLS de los WebSocket | `tokio-tungstenite` con `rustls` y el mismo proveedor criptográfico (`ring`) que `reqwest`: un solo proveedor, sin conflictos ni dependencias nativas extra. |
| Pruebas | Servidor WebSocket local con guion (`tests/websocket_streams.rs`) y prueba de humo `paper_streams_authenticate_and_deliver_market_data` contra paper (solo lectura). |
| Prueba de humo | `cargo test -p tickforge-adapter-alpaca --test paper_smoke -- --ignored --nocapture`. Solo lectura, verifica que el entorno sea paper. No corre en CI. |
