CREATE TABLE IF NOT EXISTS {table}(
    id      INTEGER PRIMARY KEY,
    name    TEXT NOT NULL,
    math    INTEGER NOT NULL,
    chinese INTEGER NOT NULL,
    english INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS name_key ON {table} (name);
