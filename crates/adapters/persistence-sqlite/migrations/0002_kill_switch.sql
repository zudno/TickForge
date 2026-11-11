-- Estado del kill switch: una sola fila (id = 1). Sin fila equivale a "desactivado".
CREATE TABLE kill_switch (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    engaged   INTEGER NOT NULL CHECK (engaged IN (0, 1)),
    -- RFC 3339 UTC; solo si está activo
    since     TEXT,
    reason    TEXT
);
