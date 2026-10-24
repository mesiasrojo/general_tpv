-- Índices de apoyo para las consultas del histórico de ventas.
-- Las consultas filtran por tienda, estado y número de ticket y ordenan por fecha.

CREATE INDEX IF NOT EXISTS idx_sales_store_state_created
    ON sales(store_id, state, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_sales_store_ticket
    ON sales(store_id, ticket_number);
