## Context

DarkDM es un gestor de descargas de video para Linux (estilo IDM). La arquitectura actual tiene dos componentes:

1. **Extensión Chrome/Vivaldi (MV3)**: Detecta streams HLS (.m3u8) via `webRequest.onSendHeaders`, muestra popup con streams detectados, envía solicitudes de descarga al native host via `chrome.runtime.sendNativeMessage`
2. **Native host (Rust)**: Binario que Chrome lanza bajo demanda, recibe mensajes via stdin/stdout (native messaging protocol), ejecuta ffmpeg para descargar streams HLS

**Problema actual**: En MV3, el service worker de la extensión se duerme después de ~30s de inactividad. Cuando el usuario hace clic en "Descargar", el worker se despierta pero `sendNativeMessage` falla silenciosamente — el native host nunca se ejecuta. El native host funciona perfectamente cuando se ejecuta directamente (test de bash produce 303 MB correctos con ffmpeg `-c copy`).

**Restricciones**:
- Vivaldi 7.9 (basado en Chromium)
- MV3 service workers con lifecycle de ~30s
- ffmpeg debe ejecutarse con `-user_agent` y `-referer` como flags separados (no `-headers`)
- El stream HLS ya viene multiplexado (video+audio), ffmpeg con `-c copy` produce 303 MB correctos

## Goals / Non-Goals

**Goals:**
- Descarga de streams HLS funciona confiablemente desde el popup de la extensión
- El native host ejecuta ffmpeg exactamente como el test de bash (mismos flags, mismo resultado)
- El popup muestra feedback al usuario (descarga iniciada, errores)
- El native host corre como servicio persistente (no depende de Chrome para arrancar)

**Non-Goals:**
- Soporte para DASH streams (futuro)
- Descarga de sitios con DRM (Netflix, Disney+)
- Progreso de descarga en tiempo real en el popup (solo confirmación de inicio)
- Multi-plataforma (solo Linux por ahora)

## Decisions

### 1. Servidor HTTP con `tiny_http` (no `hyper` ni `axum`)

**Decisión**: Usar el crate `tiny_http` para el servidor HTTP del native host.

**Rationale**: 
- `tiny_http` es minimalista (~500 líneas), sin dependencias async, fácil de integrar en el loop existente del native host
- `hyper`/`axum` requieren tokio runtime, lo cual cambia toda la arquitectura del native host
- El servidor solo necesita un endpoint (POST /download), no necesita routing complejo
- El native host actual es síncrono (lee stdin, procesa, escribe stdout). `tiny_http` permite mantener este patrón

**Alternativas consideradas**:
- `hyper` + `tokio`: Overkill para un solo endpoint, requiere refactor async completo
- `actix-web`: Demasiado pesado, muchas dependencias
- Unix socket: Más rápido pero `fetch()` no soporta Unix sockets desde extensiones

### 2. Puerto fijo `localhost:8765`

**Decisión**: El servidor escucha en `127.0.0.1:8765` (hardcoded, configurable via env var `DARKDM_PORT`).

**Rationale**:
- `fetch()` desde la extensión necesita una URL fija
- `localhost` garantiza que solo la máquina local puede conectarse
- Puerto 8765 no conflictúa con servicios comunes
- Env var permite override si hay conflicto

**Alternativas consideradas**:
- Puerto dinámico + archivo de discovery: Complejo, la extensión necesitaría leer el archivo
- Unix socket: No soportado por `fetch()` en extensiones

### 3. CORS headers para la extensión

**Decisión**: El servidor responde con `Access-Control-Allow-Origin: chrome-extension://*` y maneja preflight OPTIONS.

**Rationale**:
- `fetch()` desde la extensión es cross-origin (extensión → localhost)
- Chrome requiere CORS headers para permitir la petición
- Wildcard `chrome-extension://*` permite cualquier extensión (suficiente para uso local)

### 4. Systemd user service para el native host

**Decisión**: El native host se instala como systemd user service (`~/.config/systemd/user/darkdm-host.service`).

**Rationale**:
- Arranca automáticamente al login
- Reinicia si crashea (`Restart=on-failure`)
- Logs via `journalctl --user -u darkdm-host`
- Estándar en Linux, no requiere dependencias adicionales

**Alternativas consideradas**:
- Script en `.xprofile`: No maneja reinicios, no tiene logs
- Autostart XDG: Similar a systemd pero menos robusto
- Chrome `onInstalled` event: No puede lanzar procesos persistentes

### 5. API REST simple (POST /download)

**Decisión**: Un solo endpoint `POST /download` que recibe JSON con los datos del stream y responde inmediatamente.

**Request body**:
```json
{
  "manifest_url": "https://...m3u8?...",
  "title": "Video Title",
  "page_url": "https://pelisjuanita.com/...",
  "headers": {"user-agent": "...", "referer": "..."},
  "cookies": ""
}
```

**Response**:
```json
{
  "success": true,
  "message": "Download started",
  "output_path": "/home/dark/Descargas/DarkDM/Video Title.mp4"
}
```

**Rationale**:
- ffmpeg se lanza en background (`.spawn()`) y el servidor responde inmediatamente
- La extensión no necesita esperar a que termine la descarga
- Simple, fácil de debuggear con `curl`

### 6. Eliminar native messaging completamente

**Decisión**: Eliminar el protocolo de native messaging (stdin/stdout framing) del native host. El binario ya no lee de stdin.

**Rationale**:
- Ya no se necesita — HTTP reemplaza toda la comunicación
- Simplifica el código (eliminar `read_message`/`write_message`)
- El manifest JSON en `~/.config/vivaldi/NativeMessagingHosts/` ya no es necesario
- La extensión ya no necesita el permiso `nativeMessaging`

## Risks / Trade-offs

- **[Riesgo] Puerto 8765 ocupado** → Mitigación: Env var `DARKDM_PORT` para override, mensaje de error claro si falla el bind
- **[Riesgo] Servicio no arrancado** → Mitigación: La extensión muestra "DarkDM no está corriendo. Ejecuta: systemctl --user start darkdm-host" si fetch falla
- **[Riesgo] Firewall bloquea localhost** → Mitigación: `127.0.0.1` no pasa por el firewall, solo iptables OUTPUT podría bloquearlo (poco común)
- **[Trade-off] Proceso persistente consume recursos** → Mitigación: El servidor idle usa ~2MB RAM, ffmpeg solo corre durante descargas
- **[Trade-off] Instalación más compleja** → Mitigación: Script `install.sh` actualizado que configura systemd service automáticamente
