# Plan de desarrollo v2 — General TPV

**Versión 2. Sustituye por completo a la v1.**

## 1. Resumen ejecutivo y decisión

**Situación actual.** El repositorio solo contiene documentación; no hay código ni esquema de datos reutilizable. La aplicación antigua es un TPV/ERP de escritorio Windows ("Micro Server", Nova Decision Systems) con los módulos Mostrador, Consultas, Mercancía, Mantenimientos, Control Presencia y Utilidades.

**Cambio principal de la v2.** Se elimina todo el diseño asociado a Verifactu: sin modos fiscales, sin cadena de hash, sin QR, sin envío a AEAT, sin certificados, sin tablas `fiscal_records`/`fiscal_chain` y sin endpoints `/fiscal/*`. Se mantiene la facturación ordinaria: tickets, facturas, series, numeración, desglose de IVA, rectificativas, reimpresiones, PDF y auditoría. La exención del destinatario se acredita fuera del software.

**Decisión arquitectónica.** TPV de escritorio local-first:

- Cliente TPV: Tauri 2 + Rust para dominio, persistencia, impresión y sincronización; Svelte 5 + TypeScript estricto como capa visual.
- Datos locales: SQLite WAL (SQLCipher en producción), una base por instalación/caja.
- Central: Axum + PostgreSQL, API REST/JSON versionada y backoffice web SvelteKit.
- Sincronización event-driven, bidireccional y diferida; la caja funciona sin internet.
- Facturación ordinaria con series y numeración locales, facturas y rectificativas inmutables, IVA desglosado y PDF, sin QR ni envío telemático.

**Criterios de éxito.**

- 100 % de ventas, cobros, tickets y cierres funciona sin internet.
- Cero pérdida de datos en pruebas de caída, corte de red, disco lleno y apagado abrupto.
- Venta completa solo con teclado; p95 de confirmación ≤ 100 ms sin impresión.
- Ticket impreso ≤ 1,5 s p95 con impresora certificada.
- Sincronización de 1.000 eventos ≤ 5 s p95, sin duplicados.
- Facturas y tickets con numeración correcta, IVA desglosado y reimpresión fiel.
- Migración de artículos conciliada al 100 %.
- Ninguna venta confirmada puede borrarse o alterarse; las correcciones se hacen con devolución o rectificativa.

**Alcance v1.** Acceso/RBAC; tiendas, cajas y dispositivos; venta por teclado, devoluciones, pagos mixtos, tickets y reimpresiones; cierre de caja; gastos de caja; reservas; facturación y rectificativas; artículos, familias, impuestos, precios y stock básico; clientes, vendedores, tiendas, notas y fichaje; consultas básicas; importación CSV; backup/restore; actualizaciones; sincronización; central y backoffice; series documentales, PDF y auditoría.

**Fuera de v1.** Datáfono integrado, visor de cliente, báscula, etiquetadora, proveedores/compras, e-commerce, fidelización, multi-divisa, móvil, TicketBAI, SII, contabilidad completa, migración de tickets históricos y cualquier funcionalidad Verifactu.

## 2. Stack y disciplina de diseño

### 2.1 Comparativa

| Requisito | Web pura | Electron | Tauri + Rust | Rust puro | Nativo |
|---|---|---|---|---|---|
| Offline real | No | Sí | Sí | Sí | Sí |
| Hardware local | Limitado | Sí | Sí | Sí | Sí |
| Rendimiento | Medio | Medio | Alto | Alto | Alto |
| Productividad UI | Alta | Alta | Alta | Media/baja | Media |
| Seguridad/huella | Media | Baja | Alta | Alta | Alta |
| Multiplataforma | Alta | Alta | Alta | Alta | Baja |

Se elige Tauri 2 + Rust. La web pura se reserva para backoffice. Electron se descarta por huella y superficie de ataque. Rust puro con Slint/Iced se revisará si el equipo prescinde de TypeScript.

### 2.2 Reglas funcionales y clean code

- `tpv-domain`: sin `async`, red, ficheros, SQL ni Tauri; solo tipos, invariantes y funciones puras.
- `tpv-application`: casos de uso y puertos; sin infraestructura.
- Adaptadores: SQLite, impresora, reloj, IDs, keychain y HTTP.
- UI delgada: valida DTO, invoca comandos y presenta; no calcula precios, IVA ni cambios.
- Tipos algebraicos, `match` exhaustivo, prohibido `unwrap`/`expect`/`panic` en dominio y aplicación.
- Newtypes: `StoreId`, `SaleId`, `Sku`, `Money`, `Quantity`, `TaxRate`.
- Inmutabilidad por defecto, sin estado global mutable, puertos inyectados para reloj e IDs.
- Errores tipados con `thiserror`.
- Lints: `rustfmt`, Clippy pedantic, `cargo-deny`, `cargo-audit`; `#![forbid(unsafe_code)]` en dominio y aplicación.
- Código e identificadores en inglés; UI y documentos en español.
- Dinero: `Money(i64)` en céntimos; `rust_decimal` para precio unitario y cantidades; IVA en puntos básicos; precios con IVA incluido; redondeo half-up por línea.

### 2.3 Máquinas de estado

- `SaleState`: `Draft → Priced → AwaitingPayment → Paid → Closed`, con cancelación y devolución.
- `CashSessionState`: `Open → Closing → Closed → Reopened`.
- `InvoiceState`: `Draft → Issued → Rectified`.
- `SyncState`: `Pending → Sending → Acked/Failed/Quarantined`.

## 3. Arquitectura

### 3.1 Vista general

```text
┌────────────────────────────── PC de caja ──────────────────────────────┐
│  Svelte 5 UI  ──IPC──▶  Tauri 2 / Rust                                │
│                              │                                        │
│                              ├──▶ tpv-application ──▶ tpv-domain       │
│                              │        │                               │
│                              │        ├──▶ SQLite WAL/SQLCipher       │
│                              │        ├──▶ ESC/POS: USB/serie/red     │
│                              │        ├──▶ Cajón, lector HID          │
│                              │        └──▶ Outbox de sincronización   │
│                              │                     │                  │
└──────────────────────────────┼─────────────────────┼──────────────────┘
                               │                     │ HTTPS + TLS
                               │                     ▼
                    ┌──────────┴──────────┐  ┌─────────────────────────┐
                    │ Backoffice SvelteKit│  │ Central Rust + Axum     │
                    │  (navegador)        │◀─▶│ PostgreSQL + Object Store│
                    └─────────────────────┘  └─────────────────────────┘
```

Cada caja sincroniza directamente con la central. No hay servidor de tienda en v1. El backoffice no sustituye al TPV. Se reserva `tenant_id` para una futura versión SaaS.

### 3.2 Crates

| Crate | Responsabilidad |
|---|---|
| `tpv-domain` | Tipos, invariantes, cálculos, máquinas de estado, eventos. Sin I/O. |
| `tpv-application` | Casos de uso, puertos, transacciones, DTOs internos. |
| `tpv-storage-sqlite` | Repositorios SQLite, migraciones, seed. |
| `tpv-sync` | Outbox, push/pull, reintentos, conflictos, cuarentena. |
| `tpv-server` | Axum, PostgreSQL, autenticación, API, informes, OpenAPI. |
| `tpv-desktop` | Comandos Tauri, workers, updater, ciclo de vida. |
| `tpv-ui` | Svelte 5 + TypeScript estricto. |
| `tpv-backoffice` | SvelteKit + TypeScript estricto. |
| `tpv-testkit` | Fixtures, estrategias `proptest`, vectores de prueba. |
| `xtask` | Build, firma, empaquetado y tareas de release. |

### 3.3 Modelo de datos

Tablas principales:

- Identidad: `tenants`, `stores`, `registers`, `devices`, `users`, `roles`, `user_store_roles`.
- Maestros: `sellers`, `customers`, `products`, `product_barcodes`, `families`, `tax_rates`, `price_lists`.
- Ventas: `sales`, `sale_lines`, `payments`.
- Documentos: `invoices`, `invoice_lines`, `document_series`, `document_sequences`.
- Caja: `cash_sessions`, `cash_movements`.
- Operación: `reservations`, `reservation_lines`, `notes`, `note_recipients`, `time_entries`, `stock_movements`, `stock_levels`.
- Infraestructura: `sync_outbox`, `sync_inbox`, `audit_log`, `settings`, `legacy_imports`, `print_jobs`.

Invariantes:

- Venta confirmada inmutable; correcciones por devolución o rectificativa.
- `sum(payments) = total`; cambio solo en efectivo y nunca superior a lo entregado.
- Totales de IVA recalculados y persistidos.
- Número único por serie; sin reutilización; huecos auditados.
- Cierre de caja inmutable; reabrir crea nueva versión auditada.
- Maestros con `version` para concurrencia optimista; borrado lógico solo en maestros.
- `audit_log` append-only; sin `UPDATE` ni `DELETE`.
- `tenant_id` en todas las tablas; aislamiento por fila en PostgreSQL.
- UUIDv7 para entidades y eventos.
- Secuencias locales por `(store_id, register_id, document_type, series, year)`.

### 3.4 Sincronización

- `POST /api/v1/sync/push`: lote de eventos; respuesta con aceptados, rechazados y cursor.
- `GET /api/v1/sync/pull?cursor=...&store_id=...`: maestros y cambios.
- Sobre: `event_id`, `tenant_id`, `store_id`, `device_id`, `device_seq`, `aggregate_type`, `aggregate_id`, `event_type`, `schema_version`, `occurred_at`, `payload`, `hash`.
- Deduplicación por `event_id`; idempotencia.
- Reintentos con backoff y jitter; lote de 100; cola de cuarentena.
- Enrolamiento de dispositivo con código de un solo uso; revocación; token + TLS.
- Ventas/pagos/facturas inmutables; maestros con autoridad central; stock eventual; reservas locales; cierres locales consolidados.
- UTC en almacenamiento; `Europe/Madrid` para fecha de negocio; `device_seq` monótono; detección de skew.

### 3.5 Documentos y facturación ordinaria

- Ticket: cabecera de tienda, líneas, unidades, precio, total, IVA desglosado, pagos, entregado, cambio y pie legal. Sin QR.
- Factura: datos del cliente, número, fecha, líneas, IVA, total, forma de pago y PDF.
- Rectificativa: referencia a la original, motivo e importes corregidos.
- Reimpresión: marca "COPIA", fecha y número original; sin renumerar.
- Numeración local e inmutable por serie; una venta se factura una sola vez; correcciones por rectificativa.
- Validación de NIF/NIE/CIF antes de facturar.
- Sin modos, QR, hash, certificados ni envío a AEAT.

### 3.6 Hardware

- `PrinterPort`: USB, serie, red TCP 9100 y spooler RAW de Windows; ESC/POS, CP437/CP850/CP1252, logo raster, corte y pulso de cajón.
- Cajón conectado a impresora por pulso ESC/POS.
- Lector HID/teclado; foco, prefijo/sufijo y debounce.
- Fallback PDF y spooler del sistema.
- Snapshot de bytes ESC/POS y matriz de impresoras certificadas.

### 3.7 Seguridad, robustez y operación

- STRIDE; RBAC `Admin`, `Manager`, `Cashier`, `Auditor`; Argon2id; bloqueo por intentos; PIN rápido.
- SQLCipher en producción; clave en keychain; TLS 1.3 y pinning.
- Actualizaciones firmadas Ed25519/minisign; instaladores Authenticode; builds reproducibles.
- SBOM CycloneDX, `cargo-audit`, `cargo-deny`; sin datos de tarjeta.
- Auditoría append-only; PII redactada en logs; copias cifradas.
- SQLite WAL, transacciones, `busy_timeout`, recuperación tras caída.
- Cola de impresión persistente e idempotente.
- Outbox transaccional: venta y evento en la misma transacción.
- Recuperación de borrador tras cierre inesperado.
- Backups locales diarios cifrados (30 días); central PITR (90 días); simulacro trimestral.
- RPO local 0; RPO central 5 min; RTO local 1 h; RTO central 4 h.
- `tracing` + OpenTelemetry; logs JSON con correlation ID; métricas y alertas de sync, backup e impresora.

### 3.8 Presupuestos de rendimiento

| Operación | Objetivo p95 |
|---|---|
| Arranque en frío | ≤ 2 s |
| Búsqueda de artículo (50k SKU) | ≤ 100 ms |
| Añadir línea / recalcular | ≤ 50 ms |
| Confirmar venta (sin imprimir) | ≤ 100 ms |
| Cierre de caja | ≤ 2 s |
| Imprimir ticket | ≤ 1,5 s |
| Sincronizar 1.000 eventos | ≤ 5 s |
| Memoria en reposo | ≤ 300 MB |
| Instalador | ≤ 30 MB |

### 3.9 Interfaces públicas

Tipos principales:

```rust
Money(i64)
Quantity(Decimal)
TaxRate { code, percent_bp }
Sale { id, store_id, register_id, seller_id, lines, payments, state, totals, invoice_id }
SaleLine { product_id, code, description, unit_price, quantity, discount, tax_rate, kind }
Payment { method, amount, received, change, reference }
Invoice { id, sale_id, customer_id, series, number, issued_at, kind, totals }
DocumentSeries { id, store_id, document_type, code, year, next_number }
SyncEvent { event_id, device_id, device_seq, aggregate_id, event_type, schema_version, occurred_at, payload }
```

Comandos Tauri: `auth_login`, `auth_logout`, `auth_lock`, `get_session`, `catalog_search`, `catalog_get`, `catalog_upsert`, `sale_new`, `sale_add_line`, `sale_update_line`, `sale_remove_line`, `sale_apply_payment`, `sale_confirm`, `sale_cancel`, `sale_reprint`, `cash_open`, `cash_expense`, `cash_close`, `cash_reopen`, `cash_reprint`, `invoice_issue`, `invoice_reprint`, `invoice_rectify`, `reservation_create`, `reservation_get`, `reservation_cancel`, `customer_search`, `customer_upsert`, `seller_upsert`, `store_upsert`, `note_list`, `note_read`, `note_create`, `time_clock_in`, `time_clock_out`, `sync_status`, `sync_now`, `backup_now`, `restore_backup`, `printer_list`, `printer_test`, `printer_config`.

API central: `/api/v1/auth/*`, `/stores`, `/registers`, `/users`, `/products`, `/customers`, `/sellers`, `/sales`, `/invoices`, `/document-series`, `/cash-sessions`, `/reports/*`, `/audit`, `/sync/push`, `/sync/pull`, `/devices/enroll`, `/health`, `/metrics`. OpenAPI 3.1; RFC 7807; paginación por cursor; `Idempotency-Key`.

UX: F10 aceptar, Esc cancelar, F2 eliminar, F5 modificar, F3 leer, F9 salir; atajos configurables; escáner HID; indicadores de offline/impresora/sync; recuperación de borrador; mensajes en español; contraste alto; modo compatible con la aplicación antigua.

## 4. Plan de desarrollo por fases

| Fase | Duración | Entregables | Criterio de salida |
|---|---:|---|---|
| 0. Descubrimiento | 3 semanas | Especificación de módulos pendientes, formas de pago, devoluciones, cierre, facturación; hardware y CSV; prototipo UX; ADRs; modelo de amenazas | Especificación firmada, matriz de hardware y backlog priorizado |
| 1. Fundaciones | 4 semanas | Workspace Rust, CI, dominio, migraciones SQLite, auth/RBAC, shell Tauri, design system, logging, configuración, backup básico | Login, menú principal y venta en memoria; CI verde |
| 2. Núcleo de venta | 6 semanas | Catálogo local, borrador, líneas, precios/IVA, pagos mixtos, cambio, confirmación, ticket ESC/POS, reimpresión, cierre, offline, recuperación | Venta completa solo con teclado, ticket y cierre sin red; pruebas de caída |
| 3. Maestros y consultas | 4 semanas | Artículos, familias, impuestos, precios, clientes, vendedores, tiendas, notas, fichaje, consultas, importador CSV | Flujos maestros completos; importación conciliada |
| 4. Facturación | 2 semanas | Series, numeración, facturas, rectificativas, PDF, reimpresión, NIF, auditoría | Facturación completa sin QR ni envío; numeración y totales verificados |
| 5. Central y sync | 5 semanas | Axum, PostgreSQL, enrolamiento, push/pull, backoffice, informes, consolidación, auditoría, métricas | Multi-tienda sin duplicados; conflictos resueltos; backoffice operativo |
| 6. Endurecimiento y piloto | 4 semanas | Carga, seguridad, penetración, backups, UAT, piloto 1 tienda/2 cajas, formación, migración, release | Release gates; piloto firmado; rollback probado |

Total: 28 semanas; con 20 % de contingencia, ~34 semanas (8 meses) con 2-4 desarrolladores.

Equipo mínimo: 1 tech lead/arquitecto Rust; 2 desarrolladores Rust; 1 frontend Svelte/TS; 0,5 QA/DevOps; 0,2 product owner y asesoría legal.

Hitos: M1 venta offline con ticket; M2 cierre y reimpresiones; M3 maestros, consultas e importación; M4 facturación y rectificativas; M5 central, sync y backoffice; M6 piloto y release.

Migración: importador CSV con staging, mapeo, validación, dry-run, informe de errores, commit transaccional y conciliación. Idempotencia por `source_system + source_id + import_batch_id`. v1 migra artículos y precios; el resto queda preparado. Tickets históricos como `legacy_sales` de solo lectura y reimpresión "COPIA HISTÓRICA".

Despliegue: MSI/NSIS Windows; AppImage/deb Linux; dmg macOS best-effort. Auto-update firmado y escalonado. Central Docker + PostgreSQL gestionado, TLS, backups e IaC. Piloto en paralelo 2-4 semanas, formación y runbooks.

## 5. Calidad, pruebas, riesgos y supuestos

### 5.1 Pruebas

- Unitarias de dominio, precios/IVA, pagos, cambio, cierres y máquinas de estado.
- Property-based con `proptest` para invariantes monetarios.
- Golden tests de ESC/POS, PDF y plantillas.
- Integración SQLite/PostgreSQL, migraciones y transacciones.
- Contrato de sync, idempotencia y compatibilidad N-1.
- E2E Playwright sobre Tauri.
- Hardware-in-the-loop.
- Caos: red, latencia, disco lleno, apagado, reloj, impresora.
- Carga: 5.000 tickets/día, 50.000 artículos, 1M históricos.
- Seguridad OWASP ASVS, dependencias, fuzzing y penetración.
- Migración con CSV real, duplicados, NIF inválidos y codificaciones.
- Accesibilidad y teclado.
- Cobertura: dominio ≥ 90 %, global ≥ 80 %.

### 5.2 Escenarios de aceptación

1. Venta offline de 3 líneas con pago mixto: confirma, imprime y persiste tras reiniciar.
2. Corte de red 24 h con 500 ventas: sincroniza una vez al reconectar.
3. Impresora desconectada: venta confirmada, trabajo pendiente y reimpresión al reconectar.
4. Cierre con efectivo, tarjeta y vale: expected, contado y diferencia correctos.
5. Reabrir último cierre: solo manager, motivo, nueva versión y auditoría.
6. Devolución posterior: documento de devolución o rectificativa sin alterar la venta.
7. Factura desde venta: número, IVA, PDF y venta marcada como facturada.
8. Rectificativa: referencia, importes correctos y original inmutable.
9. Reimpresión: "COPIA", fecha y número original.
10. Migración de 50.000 artículos: dry-run, errores, reejecución y conciliación.
11. `kill -9` en venta: sin venta parcial; WAL recupera; outbox intacto.
12. Reloj desfasado: aviso, secuencia monótona y documentos auditados.
13. Multi-tienda: aislamiento y consolidación correcta.
14. RBAC: cajero no reabre cierres, no rectifica facturas ni modifica maestros.

### 5.3 Puertas de release

Tests verdes, sin warnings de Clippy, sin vulnerabilidades altas/críticas; rendimiento; revisión legal de facturación ordinaria; backups/restauración; hardware certificado; UAT y piloto; SBOM, instaladores firmados y rollback.

### 5.4 Riesgos

| Riesgo | Impacto | Mitigación |
|---|---|---|
| Variabilidad de impresoras | Alto | Abstracción, lista certificada, HIL, fallback |
| Conflictos de sync | Alto | Eventos inmutables, idempotencia, versión, cuarentena |
| Pérdida de datos | Crítico | WAL, transacciones, outbox, backups, simulacros |
| Módulos no documentados | Alto | Fase 0 de descubrimiento |
| Error de numeración/factura | Alto | Series inmutables, NIF, pruebas y conciliación |
| Falta de experiencia Rust | Medio | Tech lead, formación, revisiones |
| Webview distinto por OS | Medio | Matriz Windows/Linux, Playwright |
| Desfase de reloj | Medio | NTP, skew, secuencia |
| Scope creep | Alto | Alcance v1 y control de cambios |

### 5.5 Supuestos

- Destinatario exento de Verifactu; sin modos, hash, QR, certificados ni AEAT.
- Se emiten tickets y facturas ordinarias con IVA, series, numeración y rectificativas.
- La exención se acredita fuera del software; no hay declaración responsable de SIF.
- España, EUR, español, IVA 21/10/4/0, precios con IVA incluido.
- 5-30 tiendas, 50.000 artículos, 5.000 tickets/día, 1-10 cajas por tienda.
- Windows 10/11 principal; Linux soportado; macOS best-effort.
- Impresora ESC/POS 80 mm con cajón; lector HID.
- Central Docker/PostgreSQL; SQLite local.
- Sin tiempo real entre cajas; stock eventual y no bloqueante offline.
- v1 migra artículos; el resto se pospone.
- Sin datáfono, visor, báscula ni etiquetadora en v1.
- `tenant_id` reservado para futura versión SaaS.
- Si se comercializa a terceros no exentos, requerirá un módulo adicional fuera de este plan.

### 5.6 Incógnitas de Fase 0

- Significado del número superior derecho del TPV antiguo (`243250` frente a `2`).
- Lista completa de formas de pago.
- Flujo completo de devolución y facturación.
- Pantalla y cálculo del cierre de caja.
- Comportamiento de gastos de caja, rehacer y reabrir cierre.
- Especificación de Consultas, Mercancía y Utilidades.
- Formato de factura, series, NIF, IVA y retención documental.
- Copia de seguridad y restauración de la aplicación antigua.
- Muestras CSV y volúmenes.
- Hardware del piloto.
