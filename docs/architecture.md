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

La UI permite rectificar una factura completa desde el histórico de ventas. La operación exige rol responsable o administrador, solicita un motivo y genera una nota de crédito con serie `REC`, importes negativos, evento `invoice.rectified` y auditoría.

La devolución parcial selecciona líneas y cantidades de la factura original, prorratea importe e IVA por cantidad y emite otra rectificativa `REC`. Si el reembolso es en efectivo, registra automáticamente un gasto de caja con la referencia de la nota de crédito. Si es con tarjeta, exige la referencia del justificante del terminal y la guarda en la rectificativa para permitir la conciliación manual; la integración automática con el TPV bancario queda pendiente.

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
- Los movimientos de caja admiten gasto, retirada e ingreso; cada uno exige importe positivo y motivo.
- `sync_outbox` con el sobre completo del evento.
- `product_import_batches` y `product_import_sources` para lotes CSV idempotentes y el mapeo `source_id` → producto local.
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
- El cursor de pull se persiste en `local_identity.pull_cursor`; el pull pagina de 1000 en 1000 y no vuelve a descargar el histórico tras reiniciar.
- Con `TPV_SYNC_URL` definida, la app Tauri sincroniza al arrancar y cada `TPV_SYNC_INTERVAL_SECS` (60 segundos por defecto, mínimo 5), serializando el trabajo con el botón manual.
- Si no existe token local, la app Tauri se enrola automáticamente usando `TPV_DEVICE_NAME` y, cuando la central lo exige, `TPV_CENTRAL_TOKEN`.
- Los maestros se publican con versión monotónica por entidad y viajan en el outbox como `master_data.upsert` / `master_data.deleted`.
- La central mantiene `central_master_data` por tenant/tienda/tipo/entidad; las versiones antiguas se ignoran y las divergencias con la misma versión se registran en `central_master_conflicts`.
- La copia local aplica la misma política; `GET /api/v1/master-data/conflicts` expone los conflictos auditados.
- Los maestros de producto y cliente se persisten en el catálogo local al publicar o aplicar un cambio; las bajas se propagan como entidad inactiva.
- La importación CSV de artículos valida en la capa de aplicación (UTF-8/BOM, separador, alias de cabeceras, coma decimal e IVA en porcentaje o puntos básicos) y concentra el commit en una transacción SQLite. El lote se identifica por `(source_system, batch_id)` derivado del SHA-256 del contenido si no se indica; `(source_system, source_id)` conserva la identidad del producto aunque cambie el SKU. Solo las filas nuevas o modificadas emiten `master_data.upsert` y eventos de outbox; una fila inválida impide toda escritura y se devuelve con su línea en el informe.
- Los usuarios centrales generan `user.provisioned` con el hash Argon2id y una revisión monotónica. El terminal los aplica con `upsert_provisioned`, ignora revisiones antiguas y limpia bloqueos al cambiar la contraseña.
- Una cuenta desactivada centralmente se propaga al terminal y el login local devuelve `usuario desactivado`.
- Un evento rechazado o agotado se marca en `sync_outbox` con `failed_at`, `attempts` y `last_error`; deja de bloquear la cola y aparece en la consola de cuarentena.
- La UI del TPV y el CLI muestran el estado local y permiten reintentar la cuarentena sin reiniciar el terminal.

## 6. Central y backoffice

- `tpv-server` expone salud, autenticación, push, pull, eventos, ventas, dispositivos y usuarios centrales.
- `POST /api/v1/auth/bootstrap` crea el primer administrador solo cuando no existen usuarios; `POST /api/v1/auth/login` valida Argon2id y emite una sesión opaca de 12 horas; `GET /api/v1/auth/me` y `POST /api/v1/auth/logout` completan el ciclo.
- Las sesiones se guardan como hash SHA-256 en `central_sessions`; desactivar un usuario o restablecer su contraseña invalida sus sesiones.
- `POST /api/v1/devices/enroll` emite un token por dispositivo; `GET /api/v1/devices` y `POST /api/v1/devices/{id}/revoke` aceptan token de administración estático o sesión de administrador.
- `GET/POST /api/v1/users`, `PUT /api/v1/users/{id}` y `POST /api/v1/users/{id}/password` gestionan cuentas centrales; las escrituras requieren rol `admin`, las lecturas admiten `manager` y `auditor`. Las contraseñas se guardan como Argon2id y nunca se devuelven por la API.
- Cada alta, cambio o reset de usuario emite `user.provisioned` firmado para cada tienda.
- `GET /api/v1/master-data/conflicts` lista los conflictos de maestros auditados.
- `GET /api/v1/events` y `GET /api/v1/sales` paginan por `limit`/`offset` y filtran por `store_id`, `device_id`, `event_type` y rango `from`/`to`; el conteo y la página comparten filtros y se ordenan por `seq` descendente. Los índices de `event_type`, `occurred_at` y `device_id` acompañan a la consulta.
- Sirve el backoffice estático si existe `backoffice/build` o `TPV_BACKOFFICE_DIR`.
- El backoffice muestra salud, cursor, ventas, facturas, usuarios centrales, eventos, dispositivos registrados y conflictos de datos maestros, con filtros, paginación, detalle de payload y exportación CSV de la página de ventas.
- Desde la consola de usuarios se pueden crear cuentas, activarlas o desactivarlas y restablecer contraseñas; desde la de dispositivos se revoca un terminal.
- La consola no reescribe ventas ni documentos.
- El backoffice exige inicio de sesión; `manager` y `auditor` ven la consola en modo solo lectura y `cashier` no puede acceder.
- `TPV_CENTRAL_TOKEN` sigue disponible para automatización y enrolamiento; `TPV_REQUIRE_BACKOFFICE_AUTH=1` cierra la API al modo local.

## 7. Seguridad y límites actuales

Implementado:
- `#![forbid(unsafe_code)]` en las crates de lógica.
- Renderer ESC/POS probado con vectores de bytes y cola persistente.
- Login local con Argon2id, bloqueo por intentos, roles y auditoría.
- RBAC por comando: venta, caja, impresión, sync y gestión de usuarios.
- Movimientos de caja de gasto, retirada e ingreso con importe, motivo, evento `cash.movement` y auditoría.
- Devoluciones parciales por línea con prorrateo de importe/IVA, nota de crédito `REC`, gasto de caja automático para efectivo y referencia obligatoria del justificante para tarjeta.
- Alta, edición y restablecimiento de contraseñas de usuarios desde la UI de escritorio.
- Catálogo de productos editable desde la UI con versión maestra, evento en outbox y visualización de conflictos.
- Importación CSV de artículos con previsualización, informe de errores, commit transaccional e idempotencia por lote y por identificador de origen.
- Clientes editables desde la UI, selector de cliente en la venta, validación de NIF y sincronización maestra.
- Temas compartidos `light_theme` (por defecto, blanco y azul) y `dark_theme`, con selector persistido en `localStorage`.
- Protección del último administrador activo y de la propia cuenta.
- Validación de NIF/NIE/CIF.
- Transacciones atómicas para venta + outbox + auditoría.
- Token bearer opcional en la central y sesiones de backoffice con expiración, hash de token y RBAC.
- Bootstrap del primer administrador, invalidación de sesiones y CORS/trazas HTTP.
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
- Usuarios centrales con Argon2id, API de gestión, consola backoffice y provisioning `user.provisioned`.
- Aplicación del provisioning en el pull del CLI y en el botón de sincronización de Tauri.
- Rectificación total de facturas desde el histórico, con nota de crédito `REC`, motivo, evento y auditoría.
- CI con tests Rust, PostgreSQL, comandos Tauri, E2E de UI y auditoría de dependencias.
- Release multi-plataforma con SBOM SPDX, checksums SHA-256 y borrador de GitHub Release.
- Dependabot semanal para Cargo, npm y GitHub Actions.
- Sin cálculo de negocio en la UI.

Pendiente para producción:
- Integración automática con el terminal de pago y conciliación bancaria; hoy la referencia del justificante se registra manualmente.
- Ejecución HIL en tienda piloto, certificación de impresoras y reglas udev/WinUSB.
- Firma nativa de instaladores (Authenticode y notarización Apple) y publicación pública del release.

## 8. Verifactu

No implementado por decisión expresa: el destinatario está exento. No hay modos fiscales, hash chain, QR, certificados ni envío a AEAT. Se mantiene facturación ordinaria con IVA, series, numeración y rectificativas.

## 9. Siguiente iteración recomendada

1. Ejecución HIL en tienda piloto y ajuste de umbrales de carga.
2. Firma nativa de instaladores y publicación de la primera release pública.
3. Integración automática con el terminal de pago y conciliación de liquidaciones bancarias.
