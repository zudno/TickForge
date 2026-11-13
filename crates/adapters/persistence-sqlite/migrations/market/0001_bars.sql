-- Histórico de mercado para entrenar modelos (fase 9). Base aparte (`market.db`), compartida
-- por paper y real: el mercado es el mismo. Se lee desde Python con `sqlite3`/pandas.
-- Precios y volumen como TEXT para conservar el valor exacto (nunca REAL/flotante).
-- `starts_at` en RFC 3339, UTC y precisión fija: el orden de texto es el cronológico.

CREATE TABLE bars (
    symbol     TEXT NOT NULL,
    -- 1m | 5m | 15m | 1h | 1d
    timeframe  TEXT NOT NULL,
    starts_at  TEXT NOT NULL,
    open       TEXT NOT NULL,
    high       TEXT NOT NULL,
    low        TEXT NOT NULL,
    close      TEXT NOT NULL,
    volume     TEXT NOT NULL,
    PRIMARY KEY (symbol, timeframe, starts_at)
) WITHOUT ROWID;
