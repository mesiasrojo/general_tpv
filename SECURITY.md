# Seguridad

## Reportar una vulnerabilidad

No publiques vulnerabilidades en issues públicos. Envía un correo a `security@neotecnia.net` con:

- Versión afectada.
- Descripción y pasos de reproducción.
- Impacto esperado.
- Posible mitigación.

Acusaremos recibo en un plazo orientativo de 72 horas y coordinaremos la divulgación cuando exista una corrección disponible.

## Versiones soportadas

Durante la fase piloto solo se da soporte de seguridad a la última versión estable publicada en GitHub Releases.

## Prácticas del proyecto

- Dependencias Rust revisadas con `cargo deny`.
- Dependencias npm de producción auditadas en CI.
- SBOM SPDX adjunto a cada release.
- Checksums SHA-256 de los artefactos de release.
- SQLCipher con clave de 256 bits almacenada en el keychain del sistema.
- Contraseñas locales y centrales con Argon2id.
- Tokens de dispositivo almacenados como hash SHA-256 en la central.
- Sincronización autenticada y limitada a tenant/tienda.
