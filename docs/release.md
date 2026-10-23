# Guía de release

Esta guía describe cómo preparar, verificar y publicar una versión de General TPV.

## 1. Preparación

Antes de etiquetar una versión:

1. Actualiza la versión en todos estos sitios:
   - `Cargo.toml` (`workspace.package.version`).
   - `apps/desktop/src-tauri/tauri.conf.json`.
   - `apps/desktop/package.json`.
   - `ui/package.json`.
   - `backoffice/package.json`.
2. Ejecuta el comprobador de versiones:
   ```bash
   ./scripts/check-versions.sh v0.2.0
   ```
3. Ejecuta el preflight completo:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo build --release -p tpv-cli

   cd apps/desktop/src-tauri
   cargo clippy --all-targets -- -D warnings
   cargo test

   cd ../../ui && npm ci && npm run check && npm run build && npm run test:e2e
   cd ../backoffice && npm ci && npm run check && npm run build && npm run test:e2e
   ```
4. Ejecuta las pruebas HIL con hardware real cuando el release incluya cambios de impresión:
   ```bash
   TPV_HIL_REQUIRED=1 \
   TPV_HIL_NETWORK_ADDR=192.168.1.50:9100 \
   cargo test -p tpv-print --test hil -- --ignored --nocapture
   ```
5. Revisa los avisos de seguridad y licencias:
   ```bash
   cargo deny check
   ```

## 2. Publicación

El workflow `.github/workflows/release.yml` se dispara al subir una etiqueta `v*`:

```bash
git tag v0.2.0
git push origin v0.2.0
```

El workflow:

- Valida que la etiqueta coincide con la versión del workspace.
- Compila el CLI en Linux, Windows y macOS.
- Compila y empaqueta la app Tauri en Linux, Windows y macOS.
- Genera un SBOM SPDX del repositorio.
- Crea un borrador de release con todos los artefactos.
- Publica `SHA256SUMS.txt`.

Los artefactos esperados son:

- `general-tpv-cli-*.tar.gz` o `.zip`.
- Instaladores Tauri para la plataforma correspondiente.
- `general-tpv-vX.Y.Z-sbom.spdx.json`.
- `SHA256SUMS.txt`.

## 3. Verificación de artefactos

Descarga el borrador y comprueba los checksums:

```bash
gh release download v0.2.0 --dir release-assets
cd release-assets
sha256sum -c SHA256SUMS.txt
```

En macOS:

```bash
shasum -a 256 -c SHA256SUMS.txt
```

Comprueba el SBOM:

```bash
jq '.spdxVersion, .name, (.packages | length)' general-tpv-v0.2.0-sbom.spdx.json
```

## 4. Secretos de firma

El workflow acepta los secretos estándar de Tauri para firma de artefactos de actualización:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Para firma nativa de plataforma, configura los certificados en el runner o en la configuración de Tauri:

- Windows: certificado Authenticode y `signtool`.
- macOS: certificado Developer ID, notarización y `notarytool`.
- Linux: firma de paquetes según la distribución objetivo.

Mientras no se configure la firma nativa, los instaladores deben tratarse como artefactos internos de piloto, no como distribución pública.

## 5. Publicación del borrador

Antes de publicar:

- Verifica que el SBOM y los checksums están adjuntos.
- Prueba el instalador en una máquina limpia.
- Confirma que el terminal arranca, crea la clave en el keychain y abre la base SQLCipher.
- Confirma que el enrolamiento y la sincronización funcionan contra la central de producción.
- Comprueba backup y restauración con la misma clave.
- Revisa las notas de release y los cambios incompatibles.

Después publica el borrador desde GitHub.

## 6. Rollback

Si un release presenta un problema:

1. No borres la etiqueta publicada; crea una nueva versión correctiva.
2. Mantén la central compatible con al menos la versión anterior del terminal.
3. Restaura backups cifrados solo con la clave original del terminal.
4. Si hay que revocar terminales, usa la consola de dispositivos del backoffice.
5. Documenta la incidencia y el rango de versiones afectado en las notas del release correctivo.
