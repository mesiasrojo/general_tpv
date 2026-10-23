# Arquitectura implementada — General TPV

Este documento describe la primera rebanada vertical implementada del plan v2. El plan completo está en `docs/plan_version2.md`.

## 1. Principio rector

**Núcleo funcional, bordes imperativos.**

- `tpv-domain`: tipos, invariantes, cálculos y máquinas de estado. Sin I/O.
- `tpv-application`: casos de uso y puertos.
- `tpv-storage-sqlite`: adaptador de persistencia.
- `tpv-sync`: cliente HTTP de sincronización.
- `tpv-print`: renderer ESC/POS y adaptadores de impresora.
- `tpv-server`: central Axum.
- `apps/desktop`: shell Tauri y comandos IPC.
- `ui`: Svelte 5, sin lógica de negocio.
- `backoffice`: SvelteKit, solo lectura de la central.

## 2. Flujo de venta

```text
UI Svelte
  └─ invoke("add_line")
       └─ SaleService::add_line
            ├─ CatalogRepository::find_product_by_sku
            ├─ SaleLine::from_product (dominio puro)
            ├─ Sale::add_line (valida estado e importes)
            └─ SaleRepository::update_draft (SQLite)

UI Svelte
  └─ invoke("confirm_sale")
       └─ SaleService::confirm_sale
            ├─ SequenceRepository::next_number (TPV, año)
            ├─ Sale::assign_ticket
            ├─ Sale::confirm (valida pagos = total)
            ├─ SyncEvent::new (hash técnico del sobre)
            ├─ AuditEvent::new
            └─ SaleRepository::confirm_sale
                 └─ transacción SQLite:
                      venta + líneas + pagos + outbox + auditoría
```

La venta confirmada es inmutable. Cualquier corrección posterior debe ser devolución o rectificativa.

## 3. Dinero e IVA

- `Money(i64)` en céntimos.
- `rust_decimal` para precio unitario y cantidades.
- Precios con IVA incluido.
- IVA en puntos básicos (`2100` = 21 %).
- Redondeo half-up por línea y agregación por tipo de IVA.
- `TaxBreakdown` calcula base y cuota a partir del bruto.

## 4. Persistencia

Base local SQLite con WAL y migraciones embebidas:

- `stores`, `registers`, `users`, `sellers`, `customers`, `products`.
- `sales`, `sale_lines`, `payments`.
- `invoices`, `invoice_lines`.
- `document_sequences` para numeración atómica.
- `cash_sessions` con movimientos, cierres y reaperturas en JSON.
- `sync_outbox` con el sobre completo del evento.
- `audit_log` append-only.

SQLite guarda enums como texto, UUID como TEXT, fechas RFC3339 y dinero como INTEGER de céntimos.

## 4.1 Impresión

```text
confirm_sale
  └─ PrintService::enqueue_ticket
       ├─ TicketDocument::from_sale
       ├─ EscPosRenderer::render_ticket (Windows-1252, 42 columnas)
       └─ PrintQueueRepository::enqueue (SQLite print_jobs)

PrintService::process_pending
  ├─ PrintQueueRepository::pending
  ├─ PrinterPort::print
  │    ├─ FilePrinter
  │    ├─ DirectoryPrinter
  │    └─ NetworkPrinter (TCP 9100)
  └─ mark_printed / mark_failed
```

El ticket incluye cabecera de tienda, vendedor, cliente, líneas, desglose de IVA, total, entregado, cambio, pie legal, corte y pulso de cajón. La reimpresión se marca como "COPIA". La cola es persistente: si la impresora falla, el trabajo queda en estado `failed` y puede reintentarse.

## 5. Sincronización

```text
outbox local → SyncClient::push → POST /api/v1/sync/push
                                      ├─ valida hash del sobre
                                      ├─ deduplica por event_id
                                      └─ asigna seq y persiste en ledger

central → GET /api/v1/sync/pull?cursor=N → eventos posteriores al cursor
```

- Lotes máximos de 100 eventos.
- `event_id` UUIDv7 y deduplicación idempotente.
- El hash del sobre es integridad técnica, **no** una cadena fiscal.
- La central actual usa un ledger en memoria; el contrato HTTP está preparado para PostgreSQL.

## 6. Central y backoffice

- `tpv-server` expone salud, push, pull, eventos y ventas.
- Sirve el backoffice estático si existe `backoffice/build` o `TPV_BACKOFFICE_DIR`.
- El backoffice muestra salud, cursor, ventas, facturas y últimos eventos.
- No reescribe ventas ni documentos.

## 7. Seguridad y límites actuales

Implementado:
- `#![forbid(unsafe_code)]` en las crates de lógica.
- Renderer ESC/POS probado con vectores de bytes y cola persistente.
- Validación de NIF/NIE/CIF.
- Transacciones atómicas para venta + outbox + auditoría.
- Token bearer opcional en la central.
- CORS y trazas HTTP.
- Sin cálculo de negocio en la UI.

Pendiente para producción:
- SQLCipher y gestión de clave en keychain.
- Autenticación Argon2id y RBAC completo.
- Adaptadores USB/serie nativos y perfiles de impresora certificados.
- PostgreSQL persistente y migraciones centrales.
- Reintentos con backoff, cuarentena y resolución de conflictos de maestros.
- Firma de actualizaciones, SBOM y pipeline de release.
- Pruebas hardware-in-the-loop y de carga.

## 8. Verifactu

No implementado por decisión expresa: el destinatario está exento. No hay modos fiscales, hash chain, QR, certificados ni envío a AEAT. Se mantiene facturación ordinaria con IVA, series, numeración y rectificativas.

## 9. Siguiente iteración recomendada

1. Autenticación y RBAC con Argon2id.
2. PostgreSQL central y migraciones.
3. Worker de sync con backoff y estado visible en UI.
4. SQLCipher y backups cifrados.
5. Adaptadores USB/serie y hardware-in-the-loop.
6. Pruebas E2E Tauri.
