# General TPV

TPV de escritorio **local-first** para comercio minorista, construido con Rust y Tauri. Sustituye la aplicación Windows "Micro Server" descrita en `docs/descripcion_apliciacion_antigua.md`.

> **Verifactu no forma parte de este producto.** El destinatario está exento. El sistema emite tickets y facturas ordinarias con IVA desglosado, series, numeración y rectificativas, sin QR, hash fiscal, certificados ni envío a AEAT. El plan completo está en [`docs/plan_version2.md`](docs/plan_version2.md).

## Estado actual

| Área | Estado |
|---|---|
| Dominio puro Rust (ventas, IVA, pagos, cambio, caja, facturas, NIF, secuencias, auditoría, sync) | Implementado |
| Persistencia SQLite + migraciones + seed | Implementado |
| Casos de uso y puertos de aplicación | Implementado |
| Servidor central Axum + sync push/pull idempotente | Implementado (ledger en memoria en esta primera rebanada) |
| Cliente HTTP de sincronización | Implementado |
| CLI de demostración end-to-end | Implementado |
| Cliente Tauri + UI Svelte 5 | Implementado (requiere libs nativas de Tauri para compilar) |
| Impresión ESC/POS + cola persistente | Implementado (renderer, fichero, directorio y red TCP) |
| Backoffice SvelteKit | Implementado (consume `/api/v1/events`) |
| SQLCipher, auth Argon2, PostgreSQL persistente, USB/serie nativo | Pendiente de siguiente iteración |

## Arranque rápido

### 1. Demo completa sin infraestructura

```bash
cargo run -p tpv-cli -- --memory
```

Ejecuta: apertura de caja, venta de 2 líneas, cobro con cambio, ticket por consola, **ticket ESC/POS en `data/ticket-000001.escpos`**, factura, cierre de caja y listado de ventas.

### 2. Central + sincronización

Terminal A:

```bash
TPV_CENTRAL_ADDR=127.0.0.1:8080 cargo run -p tpv-server
```

Terminal B:

```bash
cargo run -p tpv-cli -- --memory --sync-url http://127.0.0.1:8080
```

El CLI empuja los eventos del outbox y hace un pull de verificación. El servidor expone:

- `GET /health`
- `POST /api/v1/sync/push`
- `GET /api/v1/sync/pull?cursor=0`
- `GET /api/v1/events`
- `GET /api/v1/sales`

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
crates/tpv-server             Central Axum
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

cd ui && npm run check && npm run build
cd backoffice && npm run check && npm run build
```

## Documentación

- [`docs/plan_version2.md`](docs/plan_version2.md): plan de producto, arquitectura, fases, pruebas y supuestos.
- [`docs/architecture.md`](docs/architecture.md): mapa de la implementación actual y siguientes pasos.
- [`docs/descripcion_apliciacion_antigua.md`](docs/descripcion_apliciacion_antigua.md): análisis de la aplicación legada.
