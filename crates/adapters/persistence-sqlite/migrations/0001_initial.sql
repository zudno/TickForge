-- Esquema inicial de TickForge.
-- Los decimales se guardan como TEXT para conservar el valor exacto (nunca REAL/flotante).
-- Las fechas se guardan como TEXT en RFC 3339 y UTC.

-- Diario de auditoría de órdenes: solo se agregan filas, nunca se modifican ni se borran.
CREATE TABLE order_journal (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    recorded_at      TEXT    NOT NULL,
    -- intent | risk_rejected | submitted | submission_failed | cancel_requested | reconciled
    kind             TEXT    NOT NULL,
    client_order_id  TEXT,
    broker_order_id  TEXT,
    symbol           TEXT,
    side             TEXT,
    order_type       TEXT,
    limit_price      TEXT,
    quantity         TEXT,
    time_in_force    TEXT,
    status           TEXT,
    -- Motivo de un rechazo, violaciones de riesgo o resultado de la reconciliación
    detail           TEXT
);

CREATE INDEX idx_order_journal_client_order_id ON order_journal (client_order_id);
CREATE INDEX idx_order_journal_kind ON order_journal (kind);

-- Watchlist del usuario, en el orden en que se muestra.
CREATE TABLE watchlist (
    position  INTEGER PRIMARY KEY,
    symbol    TEXT    NOT NULL UNIQUE
);
