# General TPV

TPV de escritorio **local-first** para comercio minorista, construido con Rust y Tauri. Sustituye la aplicación Windows "Micro Server" descrita en `docs/descripcion_apliciacion_antigua.md`.

> **Verifactu no forma parte de este producto.** El destinatario está exento. El sistema emite tickets y facturas ordinarias con IVA desglosado, series, numeración y rectificativas, sin QR, hash fiscal, certificados ni envío a AEAT. El plan completo está en [`docs/plan_version2.md`](docs/plan_version2.md).

## Estado actual

| Área | Estado |
|---|---|
| Dominio puro Rust (ventas, IVA, pagos, cambio, caja, facturas, NIF, secuencias, auditoría, sync) | Implementado |
| Persistencia SQLite cifrada con SQLCipher + migraciones + seed | Implementado (migra bases antiguas en texto plano) |
| Casos de uso y puertos de aplicación | Implementado |
| Servidor central Axum + PostgreSQL | Implementado (con fallback en memoria para desarrollo) |
| Cliente HTTP + worker de sync con backoff | Implementado (backoff, cuarentena y reintento manual) |
| CLI de demostración end-to-end | Implementado |
| Cliente Tauri + UI Svelte 5 | Implementado (requiere libs nativas de Tauri para compilar) |
| Impresión ESC/POS + cola persistente | Implementado (fichero, directorio, red TCP, serie y USB nativo) |
| Autenticación Argon2id + RBAC local | Implementado (login, bloqueo, roles, gestión de usuarios y auditoría) |
| Backoffice SvelteKit | Implementado (consume `/api/v1/events`) |
| Backups cifrados | Implementado (CLI y botón Backup de la UI) |
| Enrolamiento y revocación de dispositivos | Implementado (token por dispositivo, auto-enrolamiento del CLI) |
| Conflictos de datos maestros | Implementado (versiones, LWW determinista y registro de conflictos) |
| Clave SQLCipher en keychain | Implementado (migración automática y fallback `0600`) |
| Pruebas E2E de comandos Tauri y UI | Implementado (mock IPC + Playwright con Chrome) |
| Validación con hardware real | Pendiente de hardware-in-the-loop |

## Arranque rápido

### 1. Demo completa sin infraestructura

```bash
cargo run -p tpv-cli -- --memory
```

Ejecuta: login con `admin` / `admin123`, apertura de caja, venta de 2 líneas, cobro con cambio, ticket por consola, **ticket ESC/POS en `data/ticket-000001.escpos`**, factura, cierre de caja y listado de ventas.

### 2. Central + sincronización

Terminal A (PostgreSQL con Docker):

```bash
docker compose up -d postgres
DATABASE_URL=postgres://tpv:tpv@127.0.0.1:55432/tpv \
TPV_CENTRAL_ADDR=127.0.0.1:8080 \
cargo run -p tpv-server
```

Sin `DATABASE_URL`, la central usa un ledger en memoria para desarrollo.

Terminal B:

```bash
cargo run -p tpv-cli -- --memory --sync-url http://127.0.0.1:8080
```

La primera sincronización enrola el terminal automáticamente contra `/api/v1/devices/enroll` y guarda el token del dispositivo en la base cifrada. Si la central tiene configurado `TPV_CENTRAL_TOKEN`, expórtalo para autorizar el alta:

```bash
TPV_CENTRAL_TOKEN=secreto cargo run -p tpv-cli -- data/tpv.db --sync-url http://127.0.0.1:8080 --device-name "Caja 1"
```

Si el dispositivo ha sido revocado o se ha perdido el token, `--enroll` fuerza un nuevo alta y rota el token. `--retry-failed` devuelve antes a la cola los eventos en cuarentena:

```bash
cargo run -p tpv-cli -- data/tpv.db --sync-url http://127.0.0.1:8080 \
  --device-name "Caja 1" --enroll --retry-failed
```

Los cambios de datos maestros viajan como eventos versionados (`master_data.upsert` / `master_data.deleted`). La resolución es determinista: gana la versión mayor; las versiones antiguas se descartan; si dos dispositivos publican la misma versión con contenido distinto, gana el `occurred_at` más reciente y el conflicto queda registrado. Puede probarse el flujo con:

```bash
cargo run -p tpv-cli -- data/tpv.db --sync-url http://127.0.0.1:8080 \
  --master-data-demo
```

La base local se cifra con SQLCipher. La clave se guarda en el keychain del sistema (Secret Service en Linux, Keychain en macOS, Credential Manager en Windows). Si existía un `<base>.key` antiguo, se migra automáticamente y se elimina; si el keychain no está disponible, se usa ese fichero como fallback con permisos `0600`. Si la base ya existía en texto plano, también se migra automáticamente.

Para backups y recuperación:

```bash
# Backup cifrado + exportación de la clave
cargo run -p tpv-cli -- data/tpv.db --backup data/tpv-backup.db \
  --export-key data/tpv-key.export

# Abrir una copia o restaurar en otro equipo con la clave exportada
cargo run -p tpv-cli -- data/tpv-backup.db --key-file data/tpv-key.export

# Importar la clave en el keychain de este equipo
cargo run -p tpv-cli -- data/tpv.db --import-key data/tpv-key.export
```

El servidor expone:

- `GET /health`
- `POST /api/v1/sync/push`
- `GET /api/v1/sync/pull?cursor=0`
- `GET /api/v1/events`
- `GET /api/v1/master-data/conflicts`
- `GET /api/v1/sales`
- `POST /api/v1/devices/enroll`
- `GET /api/v1/devices`
- `POST /api/v1/devices/{device_id}/revoke`

### Impresión nativa

El CLI acepta `--printer` y `--printer-profile`; la app Tauri usa las variables `TPV_PRINTER` y `TPV_PRINTER_PROFILE`.

```bash
# Serie 8N1 a 9600 baudios
cargo run -p tpv-cli -- data/tpv.db \
  --printer 'serial:/dev/ttyUSB0;baud=9600;data=8;parity=none;stop=1;flow=none'

# USB directo por VID/PID, con detección automática del endpoint bulk OUT
cargo run -p tpv-cli -- data/tpv.db \
  --printer 'usb:0x04b8:0x0e15;interface=0;endpoint=0x02;packet=512'

# Perfil de 58 mm y salida a directorio para pruebas
cargo run -p tpv-cli -- data/tpv.db \
  --printer dir:data/tickets --printer-profile generic-58
```

Especificaciones disponibles: `file:...`, `dir:...`, `network:host:puerto`, `serial:...`, `usb:...` y `null`. Perfiles incluidos: `generic-58`, `generic-80`, `epson-tm-t20` y `bixolon-srp-350`. En Linux, el acceso USB directo requiere reglas udev adecuadas; en Windows puede requerir WinUSB. La validación con impresoras físicas queda pendiente de hardware-in-the-loop.

### 3. UI del TPV

```bash
cd ui
npm install
npm run dev
```

Para empaquetar con Tauri hacen falta las dependencias nativas de Tauri (en Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`). El contenedor de desarrollo actual no las tiene instaladas, por lo que el crate `apps/desktop` está excluido del workspace y no se compila en CI todavía.

### 4. Backoffice

```bash
cd backoffice
npm install
npm run dev
```

Por defecto apunta a `http://localhost:8080`. Se puede cambiar con `VITE_CENTRAL_URL` y `VITE_CENTRAL_TOKEN`.

Para servir el backoffice desde la central:

```bash
cd backoffice && npm run build
TPV_BACKOFFICE_DIR=backoffice/build cargo run -p tpv-server
```

## Estructura

```text
crates/tpv-domain             Dominio puro, sin I/O
crates/tpv-application        Casos de uso y puertos
crates/tpv-storage-sqlite     Adaptadores SQLite y migraciones
crates/tpv-sync               Cliente HTTP de sincronización
crates/tpv-print              Renderer ESC/POS y adaptadores de impresora
crates/tpv-auth               Hashing Argon2id
crates/tpv-server             Central Axum + PostgreSQL/MemoryStore
apps/cli                      Demo end-to-end
apps/desktop/src-tauri        Cliente Tauri
ui                            Interfaz Svelte 5 del TPV
backoffice                    Backoffice SvelteKit
docs/plan_version2.md         Plan aprobado v2
docs/architecture.md          Resumen de arquitectura implementada
```

## Principios

- Núcleo funcional: tipos algebraicos, inmutabilidad, `Result`, sin `unwrap`/`panic` en dominio.
- Efectos en los bordes: puertos para reloj, repositorios, impresión y transporte.
- Dinero en céntimos (`i64`) y precios/IVA con `rust_decimal`.
- Ventas confirmadas inmutables; correcciones por devolución o rectificativa.
- Outbox transaccional: venta y evento de sync se guardan juntos.
- Sin `unsafe` en dominio, aplicación, storage, servidor y sync.

## Calidad

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd apps/desktop/src-tauri && cargo test
cd ui && npm run check && npm run build && npm run test:e2e
cd backoffice && npm run check && npm run build
```

La prueba de comandos Tauri usa el runtime mock oficial y recorre login, caja, venta, cobro, ticket, factura, backup y RBAC. La prueba de UI usa Playwright con el Chrome instalado en el sistema y un puente IPC simulado; no descarga navegadores adicionales.

## Documentación

- [`docs/plan_version2.md`](docs/plan_version2.md): plan de producto, arquitectura, fases, pruebas y supuestos.
- [`docs/architecture.md`](docs/architecture.md): mapa de la implementación actual y siguientes pasos.
- [`docs/descripcion_apliciacion_antigua.md`](docs/descripcion_apliciacion_antigua.md): análisis de la aplicación legada.
