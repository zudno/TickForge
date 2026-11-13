# ml/ — Machine Learning de TickForge

Código de Python para explorar datos y entrenar modelos (fase 9). Queda **fuera** del workspace de
Rust: los modelos se exportan a ONNX y TickForge los ejecuta en Rust, sin Python en tiempo de
ejecución.

> **Principio**: el modelo **propone** y la política de riesgo **decide**. Ningún modelo envía
> órdenes directamente; toda señal pasa por `PlaceOrderUseCase` y `RiskPolicy`.

## Datos: `data/market.db`

Histórico de velas de 1 minuto en SQLite. Se llena de dos formas:

- **En vivo**: la TUI guarda cada vela que llega por el flujo de mercado mientras está abierta.
- **Histórico**: `cargo run -p tickforge -- data backfill --days 30` descarga lo que falte de la
  watchlist guardada (o de los símbolos que indiques: `... data backfill AAPL SPY`).

Tabla `bars`:

| Columna | Contenido |
|---|---|
| `symbol` | Símbolo, por ejemplo `AAPL` |
| `timeframe` | Resolución: `1m` |
| `starts_at` | Inicio de la vela, RFC 3339 en UTC (`2026-11-13T16:05:00.000000Z`) |
| `open`, `high`, `low`, `close` | Precios como texto (valor exacto) |
| `volume` | Volumen como texto |

Los precios no están ajustados por splits ni dividendos, y el feed IEX gratuito cubre solo una
fracción del volumen total del mercado.

## Leer las velas con pandas

```python
import sqlite3

import pandas as pd

# Solo lectura: no bloquea a la TUI mientras graba
connection = sqlite3.connect("file:data/market.db?mode=ro", uri=True)
bars = pd.read_sql_query(
    "SELECT * FROM bars WHERE symbol = ? AND timeframe = '1m' ORDER BY starts_at",
    connection,
    params=("AAPL",),
    parse_dates=["starts_at"],
)
# Los precios se guardan como texto para no perder exactitud; para entrenar basta float
for column in ["open", "high", "low", "close", "volume"]:
    bars[column] = bars[column].astype(float)
bars = bars.set_index("starts_at")
print(bars.tail())
```

## Próximos pasos

1. **9b**: crate `ml-features` en Rust con las mismas features para entrenar y para inferir.
2. **9c**: entrenamiento aquí (scikit-learn o LightGBM) y exportación a ONNX.
3. **9d**: inferencia en Rust en **modo sombra**: se registran las señales para medir si aciertan,
   sin enviar órdenes.
