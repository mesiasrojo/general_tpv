# Arquitectura implementada — General TPV

Este documento describe la primera rebanada vertical implementada del plan v2. El plan completo está en `docs/plan_version2.md`.

## 1. Principio rector

**Núcleo funcional, bordes imperativos.**

- `tpv-domain`: tipos, invariantes, cálculos y máquinas de estado. Sin I/O.
- `tpv-application`: casos de uso y puertos.
- `tpv-storage-sqlite`: adaptador de persistencia.
- `tpv-sync`: cliente HTTP de sincronización.
- `tpv-print`: renderer ESC/POS y adaptadores de impresora.
- `tpv-auth`: hashing Argon2id.
- `tpv-server`: central Axum.
- `apps/desktop`: shell Tauri y comandos IPC.
- `ui`: Svelte 5, sin lógica de negocio.
- `backoffice`: SvelteKit, consola de lectura y gestión de dispositivos.

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

Base local **SQLCipher** (SQLite + AES-256) con WAL y migraciones embebidas:

- `stores`, `registers`, `users`, `sellers`, `customers`, `products`.
- `sales`, `sale_lines`, `payments`.
- `invoices`, `invoice_lines`.
- `document_sequences` para numeración atómica.
- `cash_sessions` con movimientos, cierres y reaperturas en JSON.
- `sync_outbox` con el sobre completo del evento.
- `local_identity` con el tenant y dispositivo estables del terminal.
- `audit_log` append-only.

SQLite guarda enums como texto, UUID como TEXT, fechas RFC3339 y dinero como INTEGER de céntimos.

La clave aleatoria de 256 bits se guarda en el keychain del sistema: Secret Service en Linux, Keychain en macOS y Credential Manager en Windows. El servicio es `general-tpv` y la cuenta se deriva del hash de la ruta absoluta de la base. Si existe un `<base>.key` antiguo, se importa al keychain y se elimina; si el keychain no está disponible (CI, servidor headless), se mantiene el fichero sidecar con permisos `0600` como fallback. Nunca se genera una clave nueva para una base existente sin clave localizable: el arranque falla con una instrucción para usar `--key-file` o `--import-key`.

El CLI permite exportar la clave (`--export-key`), abrir una base con una clave externa (`--key-file`) e importarla al keychain (`--import-key`). Al abrir una base antigua en texto plano se exporta a SQLCipher con `sqlcipher_export`, se verifica la copia cifrada y solo entonces se sustituye el original. Los backups se generan con `VACUUM INTO`, heredan la misma clave y pueden abrirse con `SqliteStore::connect` usando esa clave.

## 4.1 Impresión

```text
confirm_sale
  └─ PrintService::enqueue_ticket
       ├─ TicketDocument::from_sale
       ├─ EscPosRenderer::render_ticket (Windows-1252, ancho según perfil)
       └─ PrintQueueRepository::enqueue (SQLite print_jobs)

PrintService::process_pending
  ├─ PrintQueueRepository::pending
  ├─ PrinterPort::print
  │    ├─ FilePrinter
  │    ├─ DirectoryPrinter
  │    ├─ NetworkPrinter (TCP 9100)
  │    ├─ SerialPrinter (tokio-serial, 8N1 configurable)
  │    └─ UsbPrinter (nusb, bulk OUT autodetectado o explícito)
  └─ mark_printed / mark_failed
```

`build_printer` acepta especificaciones `file:`, `dir:`, `network:`, `serial:`, `usb:` y `null`. Los perfiles `generic-58`, `generic-80`, `epson-tm-t20` y `bixolon-srp-350` fijan ancho, corte, cajón y velocidad serie por defecto. La impresión USB abre el dispositivo por VID/PID, selecciona el primer endpoint bulk OUT compatible (o el indicado), reclama la interfaz y, en Linux, puede desacoplar el driver del kernel si es necesario.

El ticket incluye cabecera de tienda, vendedor, cliente, líneas, desglose de IVA, total, entregado, cambio, pie legal, corte y pulso de cajón. La reimpresión se marca como "COPIA". La cola es persistente: si la impresora falla, el trabajo queda en estado `failed` y puede reintentarse. La validación con impresoras físicas queda pendiente de hardware-in-the-loop.

## 4.2 Autenticación y RBAC

```text
login(username, password)
  └─ AuthService::login
       ├─ UserRepository::find_by_username
       ├─ Argon2PasswordHasher::verify
       ├─ bloqueo tras 5 intentos durante 15 minutos
       ├─ UserRepository::record_login
       └─ AuditRepository::append (login / login_failed)

Gestión de usuarios (solo admin)
  ├─ AuthService::create_user  → UserCreated
  ├─ AuthService::update_user  → UserUpdated / UserDeactivated / UserReactivated
  ├─ AuthService::reset_password → PasswordReset + desbloqueo
  └─ AuthService::list_users

Comandos Tauri protegidos
  ├─ require_session       → lectura general
  ├─ require_sell          → venta, caja e impresión
  ├─ require_manager       → reintentos de sync y datos maestros
  └─ require_manage_users  → alta, edición y contraseñas de usuarios
       └─ AppState.session (RwLock<Option<SessionUser>>)
```

Roles: `admin`, `manager`, `cashier`, `auditor`. Los comandos de venta exigen `can_sell`; la gestión de usuarios exige `can_manage_users` (solo `admin`) y los reintentos de sincronización `can_manage_master_data` (`admin`/`manager`). No se permite desactivar la propia cuenta ni degradar o desactivar al último administrador activo. Las contraseñas se almacenan como hashes Argon2id con sal; nunca en texto plano.

## 5. Sincronización

```text
SyncWorker::push_pending
  ├─ SyncOutboxRepository::pending_events
  ├─ SyncClient::push
  │    └─ reintentos con backoff exponencial (máx. 5 intentos)
  ├─ mark_events_sent (accepted + duplicates)
  └─ mark_events_failed
       ├─ rechazos por evento devueltos por la central
       └─ errores no reintentables o reintentos agotados

SyncService (tpv-application)
  ├─ status → pendientes / enviados / en cuarentena
  ├─ quarantine → eventos fallidos con intentos y último error
  └─ retry_failed / retry_all_failed → vuelven a la cola

Central
  ├─ MemoryStore (desarrollo/tests)
  └─ PostgresStore (DATABASE_URL)
       ├─ migraciones embebidas
       ├─ deduplicación por event_id (ON CONFLICT)
       ├─ cursor global por seq BIGSERIAL
       └─ registro de dispositivos con token hasheado

Enrolamiento
  ├─ POST /api/v1/devices/enroll (admin) → token tpvdev_* (solo se devuelve una vez)
  ├─ token guardado localmente en local_identity (base SQLCipher)
  ├─ push/pull autentican el token del dispositivo
  └─ POST /api/v1/devices/{id}/revoke → siguientes sync devuelven 403

Datos maestros
  ├─ master_data_state local por tenant/tienda/tipo/entidad
  ├─ eventos master_data.upsert y master_data.deleted con versión
  ├─ central_master_data guarda la última versión aceptada
  ├─ política: versión mayor gana; versión antigua se descarta
  └─ misma versión divergente → desempate occurred_at/device_id y conflicto auditado
```

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
- La central usa PostgreSQL cuando existe `DATABASE_URL`; sin ella cae a un ledger en memoria para desarrollo y tests.
- El tenant y el dispositivo se crean una vez y se guardan en `local_identity`, de modo que los eventos pendientes siguen siendo válidos tras reiniciar el terminal.
- El CLI enrola automáticamente el terminal la primera vez y guarda el token en la base cifrada. `--enroll` rota el token y permite recuperar un dispositivo revocado.
- El token de dispositivo solo puede empujar y leer su propio tenant/tienda. La revocación invalida el token de inmediato.
- Los maestros se publican con versión monotónica por entidad y viajan en el outbox como `master_data.upsert` / `master_data.deleted`.
- La central mantiene `central_master_data` por tenant/tienda/tipo/entidad; las versiones antiguas se ignoran y las divergencias con la misma versión se registran en `central_master_conflicts`.
- La copia local aplica la misma política; `GET /api/v1/master-data/conflicts` expone los conflictos auditados.
- Un evento rechazado o agotado se marca en `sync_outbox` con `failed_at`, `attempts` y `last_error`; deja de bloquear la cola y aparece en la consola de cuarentena.
- La UI del TPV y el CLI muestran el estado local y permiten reintentar la cuarentena sin reiniciar el terminal.

## 6. Central y backoffice

- `tpv-server` expone salud, push, pull, eventos, ventas y gestión de dispositivos.
- `POST /api/v1/devices/enroll` emite un token por dispositivo; `GET /api/v1/devices` y `POST /api/v1/devices/{id}/revoke` requieren el token de administración.
- `GET /api/v1/master-data/conflicts` lista los conflictos de maestros auditados.
- Sirve el backoffice estático si existe `backoffice/build` o `TPV_BACKOFFICE_DIR`.
- El backoffice muestra salud, cursor, ventas, facturas, últimos eventos, dispositivos registrados y conflictos de datos maestros.
- Desde la consola de dispositivos se puede revocar un terminal; la central rechaza su token inmediatamente.
- La consola no reescribe ventas ni documentos.
- Las llamadas de gestión usan el token de administración configurado en la central.

## 7. Seguridad y límites actuales

Implementado:
- `#![forbid(unsafe_code)]` en las crates de lógica.
- Renderer ESC/POS probado con vectores de bytes y cola persistente.
- Login local con Argon2id, bloqueo por intentos, roles y auditoría.
- RBAC por comando: venta, caja, impresión, sync y gestión de usuarios.
- Alta, edición y restablecimiento de contraseñas de usuarios desde la UI de escritorio.
- Protección del último administrador activo y de la propia cuenta.
- Validación de NIF/NIE/CIF.
- Transacciones atómicas para venta + outbox + auditoría.
- Token bearer opcional en la central.
- CORS y trazas HTTP.
- Reintentos con backoff, cuarentena persistente y reintento manual.
- SQLCipher con clave local de 256 bits y migración automática desde texto plano.
- Backups cifrados consistentes desde CLI y UI, con la misma clave del terminal.
- Enrolamiento de dispositivos con token aleatorio hasheado en la central.
- Revocación inmediata y sync push/pull autenticados y limitados a tenant/tienda.
- Datos maestros versionados con resolución determinista y registro de conflictos.
- Clave SQLCipher en el keychain del sistema con migración desde fichero y fallback headless.
- Exportación, importación y uso de claves externas para backups y recuperación.
- Adaptadores serie y USB nativos, con detección de endpoint y perfiles de impresora.
- Pruebas E2E de la capa de comandos Tauri con runtime mock.
- Prueba Playwright de la UI con puente IPC simulado y Chrome del sistema.
- Arnés HIL para impresoras de red, serie y USB, activable por variables de entorno.
- Prueba de carga de ventas confirmadas y procesamiento de la cola de impresión.
- Consola backoffice de dispositivos y conflictos, con revocación y prueba Playwright.
- Sin cálculo de negocio en la UI.

Pendiente para producción:
- Gestión de usuarios desde backoffice central y aprovisionamiento a terminales.
- Ejecución HIL en tienda piloto, certificación de impresoras y reglas udev/WinUSB.
- Firma de actualizaciones, SBOM y pipeline de release.

## 8. Verifactu

No implementado por decisión expresa: el destinatario está exento. No hay modos fiscales, hash chain, QR, certificados ni envío a AEAT. Se mantiene facturación ordinaria con IVA, series, numeración y rectificativas.

## 9. Siguiente iteración recomendada

1. Ejecución HIL en tienda piloto y ajuste de umbrales de carga.
2. Gestión de usuarios desde el backoffice central y aprovisionamiento a terminales.
3. Edición de maestros desde la UI del TPV.
4. Firma de actualizaciones, SBOM y pipeline de release.
