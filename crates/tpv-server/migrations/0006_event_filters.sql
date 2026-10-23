-- Índices de apoyo para la consola de trazabilidad del backoffice.
-- La paginación ordena por seq descendente y filtra por tipo, fecha y dispositivo.

CREATE INDEX IF NOT EXISTS idx_central_events_event_type_seq
    ON central_events(event_type, seq DESC);

CREATE INDEX IF NOT EXISTS idx_central_events_occurred_seq
    ON central_events(occurred_at DESC, seq DESC);

CREATE INDEX IF NOT EXISTS idx_central_events_device_seq
    ON central_events(device_id, seq DESC);
