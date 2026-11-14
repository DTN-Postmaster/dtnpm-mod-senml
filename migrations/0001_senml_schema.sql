CREATE TABLE IF NOT EXISTS metric_dictionary (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    unit TEXT,
    custom_label TEXT,
    display_order INTEGER,
    is_visible INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS telemetry_history (
    metric_id INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    value REAL,
    str_value TEXT,
    ingest_time INTEGER NOT NULL,
    PRIMARY KEY (metric_id, timestamp) ON CONFLICT REPLACE,
    FOREIGN KEY (metric_id) REFERENCES metric_dictionary (id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_telemetry_history
ON telemetry_history (metric_id, timestamp ASC);
