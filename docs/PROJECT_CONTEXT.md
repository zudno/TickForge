# TickForge — Contexto del proyecto

> Documento vivo con las decisiones acordadas durante el diseño inicial (2026-11-09).
> Actualízalo cuando cambie una decisión; las decisiones grandes van además como ADR en `docs/adr/`.

## 1. Visión

TickForge es una **TUI (Terminal User Interface) en Rust** para operar acciones del mercado
de EE. UU. (TradFi): ver cotizaciones en tiempo real, consultar cuenta y posiciones, y colocar
o cancelar órdenes desde el teclado. La arquitectura debe permitir integrar en el futuro
**modelos de Machine Learning** para análisis predictivo y apoyo a la toma de decisiones.

Principios no negociables:

- Todo se desarrolla y prueba primero en **paper trading**. El modo real (LIVE) requiere tres
  llaves (`TICKFORGE_MODE=live`, `--live` y escribir una frase), llaves guardadas en el almacén de
  credenciales del sistema, límites propios y un banner rojo permanente en la interfaz.
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
| Persistencia | `rusqlite` con SQLite incluido (`bundled`) — ver sección 14 |
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
| 4. TUI básica | Bucle de eventos, layout, watchlist en vivo, barra de estado | Pantalla funcional ✅ |
| 5. Órdenes | Ticket, vista previa, modal de confirmación, riesgo pre-trade, cancelación | Comprar/vender en paper ✅ |
| 6. Persistencia | Diario de órdenes, watchlists, reconciliación al iniciar | Auditoría y recuperación ✅ |
| 7. Robustez | Reconexión con backoff, rate limits, idempotencia (`client_order_id`), apagado limpio, kill switch, panic hook | Listo para modo real ✅ |
| 8. Modo real | 8a: triple llave, llaves en el keyring, base y límites propios, arranque bloqueado. 8b: TUI en real. 8c: pérdida diaria máxima y tope de órdenes | v1.0 |
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
| `crates/adapters/persistence-sqlite` | `tickforge-adapter-sqlite` | creado (fase 6) |
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
│   │   ├── persistence-sqlite/ (migrations/, src/{store, order_journal, watchlist_repository, rows}.rs)
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

- [x] Workspace de Cargo creado (edición 2024, resolver 3, toolchain 1.94.0 fijado, MSRV 1.89).
- [x] Crates base: `domain`, `application`, `presentation-tui`, `broker-paper-sim`, binario `tickforge`.
- [x] Lints del workspace (`unsafe_code = "forbid"`, clippy contra `unwrap`/`expect`/`panic`/aritmética flotante).
- [x] `.gitignore`, `.env.example`, `CLAUDE.md`, este documento.
- [x] Repositorio público en GitHub: https://github.com/zudno/tickforge
- [x] `.gitattributes` (saltos de línea LF en todo el repositorio).
- [x] `deny.toml` (licencias permisivas, vulnerabilidades, solo crates.io).
- [x] CI en GitHub Actions: `fmt`, `clippy -D warnings` y `test` en Linux y Windows, más `cargo-deny`.
- [ ] Crate `telemetry` y logging a archivo (se hará junto con la TUI, fase 4).
- [x] Licencia: doble `MIT OR Apache-2.0` (convención del ecosistema Rust), en `LICENSE-MIT` y `LICENSE-APACHE`, declarada en `[workspace.package]`.
- [x] GitHub borra la rama de cada PR al fusionarlo ("Automatically delete head branches").

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

**Fase 3 completa.**

### Fase 4 — TUI básica

- [x] Casos de uso de mercado: `WatchMarketUseCase` (foto inicial + flujo en vivo) y `GetMarketClockUseCase`.
- [x] TUI: estado, acciones, reducer puro, mapa de teclas, ejecutor de comandos y bucle de eventos.
- [x] Pantalla principal: barra de cuenta, watchlist en vivo, posiciones, barra de estado con PAPER siempre visible, ayuda (`?`).
- [x] Composition root: configuración desde `.env`, logs a archivo diario (`logs/`), reloj del sistema, cableado de Alpaca.
- [x] Verificado contra Alpaca paper con el mercado abierto (precios, cambio %, flujo en vivo).
- [ ] Modo simulador sin red para demos (pendiente).

**Fase 4 completa.**

### Fase 5 — Órdenes

- [x] `PlaceOrderUseCase` en dos pasos: `preview` (evalúa riesgo sin enviar) y `submit` (reevalúa con datos frescos y envía la solicitud previsualizada); `execute` sigue disponible para un motor automático.
- [x] `WatchOrderUpdatesUseCase`: flujo `trade_updates` en la TUI (notificaciones de aceptación, ejecuciones, cancelaciones y rechazos).
- [x] TUI: ticket (`b`/`s`), confirmación con costo, efecto en la posición y violaciones de riesgo; panel de órdenes abiertas (`Tab`) y cancelación con confirmación (`c`).
- [x] Composition root: límites de riesgo en `.env`, generador de `client_order_id`, diario provisional en el log (`tickforge::journal`).
- [x] Verificado contra Alpaca paper: orden límite creada, visible, reintento idempotente, eventos `Accepted`/`Canceled` por WebSocket y cancelación.
- [ ] Diario persistente en SQLite y reconciliación al iniciar (fase 6).

**Fase 5 completa.**

### Fase 6 — Persistencia

- [x] Adaptador SQLite (`tickforge-adapter-sqlite`): diario de órdenes (solo se agregan filas) y watchlist, con migraciones embebidas.
- [x] Puertos: `OrderJournal::pending_intents`, `BrokerGateway::order_by_client_order_id`, `WatchlistRepository`; entrada `JournalEntry::Reconciled`.
- [x] `ReconcileOrdersUseCase` al iniciar: recupera órdenes que llegaron al broker antes de una caída y marca las que nunca llegaron.
- [x] `ManageWatchlistUseCase`: watchlist guardada (con semilla en el primer arranque), agregar con `a` (verifica que haya datos) y quitar con `d`.
- [x] Verificado contra Alpaca paper con SQLite en disco: caída simulada tras el envío → orden recuperada al reiniciar.

**Fase 6 completa.**

### Fase 7 — Robustez

- [x] Ya cubiertos antes: idempotencia por `client_order_id`, restauración de la terminal ante pánicos, cierre ordenado de la base, reconciliación al iniciar.
- [x] Reconexión automática de ambos flujos (datos y órdenes) con espera exponencial; la conexión cuenta como sana si entregó datos o duró 1 minuto.
- [x] Resincronización: al reconectar el flujo de órdenes se recarga la cuenta por REST; al reconectar el de mercado se recarga la foto inicial.
- [x] Reintentos HTTP para errores pasajeros (red, 429, 5xx), respetando `Retry-After`; nunca para cancelaciones ni errores 4xx.
- [x] Limitador de peticiones del lado del cliente (190/min por API).
- [x] Detección de flujo congelado: 2 minutos sin datos con el mercado abierto fuerza la reconexión.
- [x] Kill switch (`!`): bloquea toda orden nueva desde la política de riesgo y cancela las abiertas; persistido en SQLite.

**Fase 7 completa.**

### Fase 8 — Modo real (LIVE)

- [x] 8a. Triple llave: `TICKFORGE_MODE=live` + `--live` + escribir `OPERAR EN REAL` en la consola.
- [x] 8a. Llaves reales solo en el almacén de credenciales del sistema (`tickforge credentials set-live`).
- [x] 8a. Validación del adaptador: URLs de la cuenta real exigen `LiveTradingAuthorization`; entorno, URLs y prefijo de llaves deben coincidir.
- [x] 8a. Base de datos propia (`data/live/tickforge.db`) y límites de riesgo propios (`TICKFORGE_LIVE_*`).
- [x] 8a. Cada sesión en real arranca con las órdenes nuevas bloqueadas (sin cancelar las abiertas).
- [x] 8a. Prueba de arquitectura: el acceso real solo puede construirse en el composition root.
- [x] 8b. Resumen de la cuenta real al abrir (número enmascarado, saldos, posiciones, órdenes, estado del bloqueo); no se puede saltar antes de que cargue.
- [x] 8b. Bordes rojos en todos los paneles e insignia `LIVE · DINERO REAL` en la barra de estado, la cuenta y cada confirmación.
- [x] 8b. Segunda confirmación por orden en real: escribir el símbolo exacto después de la vista previa.
- [x] 8b. Órdenes a mercado solo con el mercado abierto en real, como regla de la política de riesgo.
- [x] 8c. Pérdida máxima del día: la política bloquea órdenes nuevas y `DailyLossGuard` activa el kill switch solo (cancela lo abierto).
- [x] 8c. Tope de órdenes por minuto y por día (ventanas móviles), contado en el diario.

**Fase 8 completa.** Siguiente: fase 9 (ML).

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

## 12. Decisiones de la TUI

| Concepto | Decisión |
|---|---|
| Flujo | Unidireccional: evento de terminal → `keymap` → `Action` → `reduce(&mut AppState, Action) -> Vec<Command>` → `render(&AppState)`. El reducer no hace I/O ni lee el reloj: se prueba sin terminal. |
| Efectos secundarios | `Command` (cargar portafolio, reloj, abrir flujo) lo ejecuta `CommandExecutor` en tareas de tokio; los resultados vuelven como `Action` por un canal. La interfaz nunca espera a la red. |
| Redibujado | Teclado y cambios de tamaño: de inmediato. Datos de mercado: se agrupan y se dibujan en el siguiente tick (100 ms, ~10 FPS), porque pueden llegar cientos por segundo. |
| Refrescos | Portafolio cada 30 s; reloj de mercado cada 60 s y justo después de abrir o cerrar la sesión. `r` refresca y reconecta el flujo si está caído. |
| Un solo flujo de datos | `CommandExecutor` mantiene una sola tarea de mercado y aborta la anterior al reconectar (límite de una conexión del plan gratuito). |
| Watchlist | Foto inicial (`latest_quote` + 2 velas diarias para el cierre anterior y el cambio %) y luego datos en vivo. Configurable con `TICKFORGE_WATCHLIST`. |
| Modo visible | Insignia `PAPER` (amarilla) o `LIVE` (roja) siempre en la barra de estado; en modo real todo además pasa por las salvaguardas de la sección 17. |
| Teclas | `j/k` o flechas, `r`, `?`, `Esc`, `q` y `Ctrl+C`. Solo se procesan eventos de **presionar**: Windows también envía los de soltar. |
| Formato | El formato `{:.N}` de `Decimal` **trunca**; siempre se redondea antes (`format.rs`). Hora siempre en Nueva York (`ET`). |
| Terminal | `ratatui::try_init`/`try_restore`, que restauran la terminal incluso ante un pánico. Logs a `logs/tickforge.log.AAAA-MM-DD`, nunca a la pantalla. |
| Pruebas | Reducer, keymap, watchlist y formato con pruebas unitarias; pantallas con snapshots de `insta` sobre `TestBackend` (`src/ui/snapshots/`). |
| Dependencias | `ratatui` 0.30 + `crossterm` 0.29 (0.29 de ratatui arrastraba `paste`, sin mantenimiento). MSRV subido a 1.88 para poder usar `time` 0.3.47 (RUSTSEC-2026-0009). |

## 13. Decisiones del flujo de órdenes

| Concepto | Decisión |
|---|---|
| Dos pasos | `preview` construye la solicitud (con su `ClientOrderId`), consulta cuenta, posición y cotización, y evalúa la política **sin enviar**. `submit` envía exactamente esa solicitud: confirmar dos veces nunca duplica la orden (verificado contra Alpaca). |
| Riesgo dos veces | `submit` vuelve a evaluar la política con datos frescos: entre la vista previa y la confirmación el precio o el saldo pueden cambiar. |
| Confirmación | No se puede confirmar una vista previa con violaciones; se muestran todas y `Esc` vuelve al ticket con los datos para corregir. Mientras se espera al broker, `Esc` y un segundo `Enter` no hacen nada. |
| Ticket | Cantidad y precio se capturan como texto (solo dígitos y un punto) y se validan al pedir la vista previa con mensajes en español. Al cambiar a límite se propone el ask (compra) o el bid (venta). |
| Teclado en ventanas | El ticket captura dígitos, flechas, Tab y Enter; letras como `q` o `b` no tienen efecto mientras se escribe. Las confirmaciones solo aceptan una respuesta explícita. |
| Órdenes abiertas | Panel propio; `Tab` cambia el foco y `c` pide cancelar la seleccionada (con confirmación). |
| Eventos de órdenes | El flujo `trade_updates` genera notificaciones y pide refrescar el portafolio, agrupando ráfagas (máximo un refresco por segundo). Las órdenes en pantalla salen del portafolio REST, así no hay doble conteo de ejecuciones (ver limitación de `apply_to`). |
| Mercado cerrado | La confirmación avisa que la orden esperará a la apertura. |
| Límites de riesgo | `.env`: `TICKFORGE_MAX_ORDER_NOTIONAL` (5000), `TICKFORGE_MAX_POSITION_NOTIONAL` (20000), `TICKFORGE_MAX_PRICE_DEVIATION_PCT` (5), `TICKFORGE_MAX_QUOTE_AGE_SECS` (15). |
| Diario provisional | `TracingOrderJournal` escribe cada intención, envío, rechazo y cancelación en el log con destino `tickforge::journal` hasta tener SQLite (fase 6). |
| `client_order_id` | `tf-<milisegundos>-<secuencia>`: único entre reinicios y dentro de la sesión. |

## 14. Decisiones de persistencia

| Concepto | Decisión |
|---|---|
| Motor | **`rusqlite` 0.41 con SQLite incluido (`bundled`)**, no `sqlx`. `cargo-deny` detectó RUSTSEC-2026-0459 (SQLite < 3.54) y `sqlx` 0.9 todavía no admite `libsqlite3-sys` 0.39, la primera con SQLite 3.54. `rusqlite` además evita arrastrar los controladores de MySQL y PostgreSQL. Si `sqlx` se actualiza, se puede reevaluar sin tocar los puertos. |
| Asincronía | SQLite es síncrono: cada operación corre en `spawn_blocking` sobre una conexión compartida (`Arc<Mutex<Connection>>`), así nunca bloquea la TUI ni la red. |
| Archivo | `data/tickforge.db` (configurable con `TICKFORGE_DATA_DIR`), ignorado por git. Modo WAL + `synchronous = NORMAL`: tolerante a cortes; al salir se hace `wal_checkpoint(TRUNCATE)` y si la app se cierra de golpe SQLite lo recupera al abrir. |
| Migraciones | Archivos `.sql` embebidos con `include_str!` y versionados con `PRAGMA user_version`; cada una en su transacción. Una base con versión más nueva que la aplicación se rechaza con un mensaje claro. |
| Tipos | Decimales como `TEXT` (exactos, nunca `REAL`), fechas en RFC 3339 UTC, enums con los códigos estables del dominio (`as_str`). Al leer, todo se valida otra vez con el dominio; un dato inválido es `Corrupted`, nunca un valor aproximado. |
| Diario | Solo se agregan filas (`order_journal`): intención, rechazo de riesgo, envío, fallo de envío, cancelación y reconciliación. |
| Intenciones pendientes | Las que no tienen `submitted` ni `reconciled`. **Incluye las de `submission_failed`**: un error de red puede ocurrir después de que el broker recibió la orden. |
| Reconciliación | Al iniciar (antes que todo lo demás), cada intención pendiente se busca en el broker por `client_order_id`: encontrada → `FoundAtBroker` (se avisa y se refresca la cuenta); no encontrada → `NotFoundAtBroker`; sin respuesta del broker → queda pendiente para el próximo arranque. |
| Watchlist | Tabla `watchlist` (posición, símbolo). La guardada tiene prioridad; `TICKFORGE_WATCHLIST` solo siembra el primer arranque. `a` agrega (verifica que el proveedor tenga datos del símbolo; máximo 30), `d` quita (nunca la deja vacía). Cada cambio se guarda y reabre el flujo en vivo con la lista nueva. |
| MSRV | 1.89 (lo exige `rusqlite` 0.41). |

## 15. Decisiones de robustez

| Concepto | Decisión |
|---|---|
| Espera exponencial | `ExponentialBackoff` en `application::resilience` (pura, sin `tokio`): flujos 1 s → 60 s; peticiones REST 250 ms → 4 s. La usan el adaptador HTTP y la TUI. |
| Reconexión de flujos | Bucle en las tareas de `CommandExecutor`. Tras perder la conexión se publica `StreamDisconnected` + `StreamReconnecting { attempt, delay }`, se espera y se reintenta. La cuenta de intentos solo se reinicia tras una conexión **sana** (entregó datos o duró 60 s): un servidor que acepta y corta de inmediato recibe esperas crecientes. |
| Avisos | Solo se avisa al perder una conexión que funcionaba y al recuperarla; los intentos fallidos intermedios no llenan la barra de mensajes. La barra muestra `↻ Datos: reintento N en Xs`. `r` reintenta de inmediato. |
| Resincronización | Al reconectar el flujo de órdenes se recarga el portafolio por REST (fuente de verdad): cubre ejecuciones o cancelaciones ocurridas mientras estuvo caído. Cada reconexión del flujo de mercado recarga la foto inicial. |
| Flujo congelado | Si el flujo está "en vivo", el mercado abierto y no llega ningún dato en 120 s, se considera conexión zombi y se reconecta. Con el mercado cerrado el silencio es normal y no se fuerza nada. |
| Reintentos HTTP | En `AlpacaHttp`: hasta 3 reintentos ante red, 429 o 5xx, con espera exponencial o la de `Retry-After` (tope 60 s). **Se reintentan** lecturas y el envío de órdenes (el `client_order_id` evita duplicados). **No se reintentan** cancelaciones (un segundo intento daría un error engañoso) ni errores 4xx. |
| Límite de peticiones | Ventana deslizante de 60 s, 190 peticiones por API (trading y datos por separado), solo con aritmética entera. Configurable en `AlpacaConfig`. |
| Pruebas | Tiempo simulado de `tokio` (`start_paused`) para el limitador y el bucle de reconexión; `wiremock` para reintentos y `Retry-After`. Verificado en vivo: una segunda instancia rechazada por el límite de conexiones reintenta con esperas crecientes y se conecta sola al liberarse la conexión. |

## 16. Decisiones del kill switch

| Concepto | Decisión |
|---|---|
| Dónde vive | En el **dominio** (`risk::KillSwitch`) y en `RiskPolicy`: `RiskContext` lleva el estado y, si está activo, toda orden recibe `RiskViolation::KillSwitchEngaged`. Así bloquea cualquier origen (usuario, reintentos, futuro motor de ML), no solo la TUI. |
| Lectura | `PlaceOrderUseCase` lo lee del almacenamiento en **cada** evaluación (vista previa y envío). Si no se puede leer, la orden no sale. |
| Persistencia | Puerto `KillSwitchRepository`; en SQLite una fila única (migración `0002_kill_switch.sql`). Sobrevive a reinicios y caídas: al iniciar, la TUI avisa si sigue activo. |
| Orden de activación | 1) guardar el bloqueo (si falla, se informa y **no** se cancela nada: el usuario debe saber que no hay protección); 2) registrar en el diario; 3) cancelar en paralelo todas las órdenes cancelables. Las fallas parciales se reportan y `!` permite reintentar. |
| Qué no hace | No cierra posiciones (lo dice la confirmación). Desactivarlo solo vuelve a permitir órdenes nuevas. |
| Diario | Entradas `kill_switch_engaged` (con motivo) y `kill_switch_released`. |
| TUI | `!` abre la confirmación (activar o desactivar según el estado); solo `Enter`/`Esc`. Activo: marco de la cuenta en rojo con aviso, insignia `⛔ DETENIDO` en la barra de estado y `b`/`s` explican el bloqueo en lugar de abrir el ticket. |
| Prueba de humo | `paper_kill_switch_smoke` (escribe en paper) se **niega a correr si la cuenta tiene órdenes abiertas**: el kill switch las cancelaría todas, incluidas las del usuario. |

## 17. Decisiones del modo real (LIVE)

| Concepto | Decisión |
|---|---|
| Triple llave | Se necesitan las tres: `TICKFORGE_MODE=live` en `.env`, el parámetro `--live` y escribir `OPERAR EN REAL` en la consola antes de que se abra la TUI. Si solo hay una de las dos primeras, TickForge no arranca en **ningún** modo (la discrepancia indica un error, no se cae a paper en silencio). Cualquier otra respuesta a la frase cancela. |
| Autorización en el tipo | `LiveTradingAuthorization` (adaptador de Alpaca) solo se obtiene con la frase exacta y no es `Clone`. `AlpacaConfig::live` la exige; `AlpacaConfig` tiene un campo privado, así que no se puede armar con un literal fuera del crate. |
| Validación del adaptador | `AlpacaBrokerGateway::new` y `AlpacaMarketData::new` validan la configuración: una URL de `api.alpaca.markets` sin autorización se rechaza (aunque se cambie el campo `environment` a mano); el entorno debe coincidir con las URLs; llaves `PK…` en real o `AK…` en paper se rechazan. Alpaca describe esos prefijos como pistas visuales, por eso solo se usan para **rechazar**, nunca para aprobar. |
| Llaves reales | Solo en el almacén de credenciales del sistema (crate `keyring`; en Windows, el Administrador de credenciales), servicio `tickforge`. `tickforge credentials set-live` las pide sin mostrarlas; `delete-live` las borra. Nunca se leen de `.env` ni de variables de entorno. Las de paper siguen en `.env`. |
| Base de datos | Paper: `data/tickforge.db` (ruta histórica). Real: `data/live/tickforge.db`. Diario, watchlist y kill switch nunca se cruzan entre cuentas. |
| Límites de riesgo | Independientes de paper, con variables `TICKFORGE_LIVE_*`. Por defecto: 250 USD por orden, 1,000 USD por posición, 3 % de desviación máxima del límite y cotizaciones de máximo 10 s. |
| Arranque bloqueado | Cada sesión real ejecuta `KillSwitchUseCase::hold`: bloquea las órdenes nuevas **sin cancelar** las abiertas (motivo "inicio de sesión en modo real"). Si ya estaba activo, conserva el motivo original. La persona revisa la cuenta y desbloquea con `!`. |
| Pruebas | Ninguna prueba toca la cuenta real. `no_live_access_in_tests` recorre todo el workspace y falla si la autorización, `AlpacaConfig::live` o el entorno `Live` aparecen fuera de `apps/trader-tui/src` y de los archivos del adaptador que los definen; también verifica que `main` pida la frase antes de construir la configuración real. |
| Resumen al iniciar | En real, `AppState` arranca con `Overlay::LiveSessionStart`: cuenta (`Alpaca ••••7890`, con `Account::number` que informa el broker), saldos, posiciones, órdenes abiertas y estado del kill switch. `Enter`/`Esc` no lo cierran hasta que la cuenta cargó; `q` siempre sale. |
| Marcas visuales | En real: bordes rojos en todos los paneles sin foco (`theme::panel_border`), insignia `LIVE · DINERO REAL` en la barra de estado, el marco de la cuenta y los títulos de la vista previa y la cancelación; la vista previa empieza con `⚠ DINERO REAL · cuenta ••••7890`. |
| Segunda confirmación | En real, el primer `Enter` sobre una vista previa aprobada no envía: pide escribir el símbolo (`PreviewOverlay::typed_symbol`). Solo el símbolo exacto envía; otro texto muestra el error. `Esc` vuelve a la vista previa y otro `Esc` al ticket. En paper sigue siendo un solo `Enter`. |
| Mercado fuera de horario | Regla del dominio: `RiskLimits::restrict_market_orders_to_open_market` (activa solo en real, no configurable) produce `RiskViolation::MarketOrderOutsideRegularHours` para órdenes a mercado si el mercado no está abierto **o no se pudo saber** (`RiskContext::market_open`). `PlaceOrderUseCase` consulta el reloj solo cuando la regla aplica. Las órdenes límite no se ven afectadas. Al estar en la política, también la cumplirá el futuro motor de ML. |
| Pérdida máxima del día | `RiskLimits::with_max_daily_loss`; la pérdida es `last_equity - equity` (el P&L del día de la barra de cuenta). Regla única `RiskLimits::daily_loss_breach`, usada por la política (`RiskViolation::DailyLossLimitReached`, bloquea toda orden nueva) y por `DailyLossGuard`, que con cada cuenta cargada (cada 30 s y tras cada ejecución) activa el kill switch con el motivo "pérdida del día de X (máximo Y)" y cancela las órdenes abiertas. Si ya está activo, no repite nada. Si se desactiva a mano con la pérdida aún superada, la siguiente revisión lo vuelve a activar. Por defecto: 50 USD en real y 1,000 USD en paper (`TICKFORGE_[LIVE_]MAX_DAILY_LOSS`). |
| Tope de órdenes | `RiskLimits::with_order_rate_limits`: ventanas **móviles** de 60 s y 24 h (no el día calendario), sobre las intenciones del diario (`OrderJournal::count_intents_since`), así que sobrevive a reinicios. Cuentan los intentos de envío (también los que el broker rechazó), no las órdenes que bloqueó el riesgo. Si el diario no responde, la orden no sale. Por defecto: 5/min y 30/día en real; 30/min y 500/día en paper (`TICKFORGE_[LIVE_]MAX_ORDERS_PER_MINUTE` y `..._PER_DAY`). Protegen contra errores en bucle y contra un futuro modelo de ML desbocado. |
