## Why

Chrome MV3 service workers se duermen después de ~30s de inactividad. Cuando el usuario hace clic en "Descargar" en el popup, el service worker se despierta pero `chrome.runtime.sendNativeMessage` falla silenciosamente — el native host nunca se ejecuta. Esto hace que la descarga no arranque sin ningún error visible. El native host funciona perfectamente cuando se ejecuta directamente (test de bash produce 303 MB correctos), pero el puente de native messaging de Chrome es poco confiable en MV3.

## What Changes

- **Native host se convierte en servidor HTTP local** — escucha en `localhost:8765` como proceso persistente (systemd user service o script de arranque)
- **Extensión usa `fetch()` en lugar de `sendNativeMessage`** — el popup envía la solicitud de descarga via HTTP POST al servidor local
- **Se elimina la dependencia de native messaging de Chrome** — ya no se necesita el manifest JSON en `~/.config/vivaldi/NativeMessagingHosts/`
- **Se elimina el service worker keepalive** — ya no se necesita `setInterval` ni puertos de keepalive porque `fetch()` funciona incluso con workers dormidos
- **BREAKING**: El native host ya no se lanza automáticamente por Chrome — debe estar corriendo como servicio antes de usar la extensión

## Capabilities

### New Capabilities
- `http-server`: Servidor HTTP local en el native host que recibe solicitudes de descarga via REST API (POST /download) y ejecuta ffmpeg en background

### Modified Capabilities
- `extension-download`: El popup cambia de `chrome.runtime.sendNativeMessage` a `fetch('http://localhost:8765/download')` para enviar solicitudes de descarga

## Impact

- **Native host (Rust)**: Añadir módulo HTTP server (usando `tiny_http` o `hyper`), endpoint POST /download, mantener la lógica de ffmpeg existente
- **Extensión (JS)**: Cambiar popup.js para usar fetch(), eliminar background.js native messaging code, simplificar arquitectura
- **Instalación**: Añadir systemd user service o script de arranque para el native host, eliminar manifest de native messaging
- **Dependencias**: Añadir crate HTTP al Cargo.toml del native host
- **Permisos de extensión**: Ya no necesita `nativeMessaging`, solo `host_permissions` para `http://localhost:8765/*`
