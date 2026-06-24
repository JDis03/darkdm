# DarkDM — Gestor de Descargas para Linux (como IDM)

DarkDM es un gestor de descargas para Linux inspirado en **IDM (Internet Download Manager)**.
Detecta y descarga archivos, streams de video y contenido desde cualquier sitio web.

## Estado actual

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│  🟢 Bash scripts (funcional)   →   🟡 Spec completado           │
│  ↓                                 ↓                            │
│  darkdm-mediafire                openspec/changes/native-cli/   │
│  darkdm-capture                  (2000+ líneas)                 │
│  darkdm-cli                                                      │
│  (Rust, solo HLS)                                                │
│                                                                  │
│  🔵 FUTURO: darkdm CLI nativo en Rust                           │
│  → Engine compartido con Tauri                                   │
│  → Multi-hilo, resume, plugins, pipeline                         │
│  → Reemplaza todos los scripts                                   │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

| Componente | Estado | Descripción |
|---|---|---|
| `darkdm-mediafire` (bash) | ✅ **Funcional** | Descarga desde MediaFire con `--get-link`, `--password`, resume, extracción RAR/ZIP/7z |
| `darkdm-cli` (Rust) | ⚠️ Solo HLS | Descarga streams .m3u8 vía ffmpeg |
| `darkdm-host` (Rust) | ✅ **Funcional** | Servidor HTTP para la extensión de Chrome |
| Extension Chrome (MV3) | ✅ **Funcional** | Detecta streams HLS automáticamente |
| Tauri App (Svelte + Rust) | ✅ **Funcional** | Lista archivos descargados |
| **Spec CLI nativo** (openspec) | ✅ **Completado** | 2000+ líneas con patrones: Pipeline, State Machine, Plugin Registry, Event System |
| CLI nativo en Rust | 🔜 **Próximo** | Engine compartido con Tauri, multi-hilo, resume, plugins |

## Scripts disponibles hoy

### `darkdm-mediafire` — Descarga desde MediaFire

```bash
# Descargar y extraer automáticamente
darkdm-mediafire "https://www.mediafire.com/file/XXXX/archivo.rar/file"

# Con contraseña para RAR protegido
darkdm-mediafire "https://www.mediafire.com/file/XXXX/archivo.rar/file" --password "mipass"

# Solo obtener el enlace directo (sin descargar)
darkdm-mediafire "https://www.mediafire.com/file/XXXX/archivo.rar/file" --get-link

# Usar enlace directo (si ya lo tienes)
darkdm-mediafire --direct "https://download1350.mediafire.com/.../archivo.rar"

# Destino personalizado
darkdm-mediafire "https://..." ~/Videos --password "mipass"
```

**Comportamiento:**
- Extrae automáticamente el link directo de la página de MediaFire
- Descarga con curl (resume con `-C -`, timeout 1h, barra de progreso)
- Extrae automáticamente: RAR (con/sin contraseña), ZIP, 7z, tar.gz, tar.xz
- Conserva siempre el archivo original (nunca se borra)
- Los videos extraídos van a `~/Descargas/DarkDM/` por defecto

### `darkdm-cli` — Descarga streams HLS

```bash
darkdm-cli "https://cdn.ejemplo.com/stream.m3u8" video.mp4
darkdm-cli "https://cdn.ejemplo.com/stream.m3u8" --referer "https://sitio.com"
```

### `darkdm-capture` — Captura nativa de tráfico TLS

```bash
darkdm-capture start   # Inicia tcpdump + SSLKEYLOGFILE
darkdm-vivaldi         # Abre Vivaldi con claves TLS
darkdm-capture stop    # Detiene y extrae segmentos .ts del PCAP
```

### `darkdm-host` — Servidor HTTP para la extensión

```bash
# Iniciar manualmente
darkdm-host

# O como servicio systemd
systemctl --user start darkdm-host
```

## Extensión de Chrome

```
extension/
├── manifest.json        ← MV3
├── background.js        ← Detecta streams .m3u8 via webRequest
├── content.js           ← Overlay flotante sobre videos
├── hook.js              ← Intercepta fetch/XHR en la página
├── popup/               ← Lista de streams detectados + botón descargar
└── icons/
```

La extensión:
1. Detecta automáticamente streams HLS via `webRequest.onSendHeaders`
2. Muestra un overlay "⬇️ Descargar" sobre los videos
3. Envía la descarga al `darkdm-host` via HTTP POST
4. Soporta MV3 service workers (usa HTTP, no native messaging)

## Spec del CLI nativo (próximo)

El spec en `openspec/changes/native-cli/` define el futuro CLI `darkdm` en Rust:

```bash
# Ejemplo de cómo funcionará (no implementado aún)
darkdm descargar "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
darkdm descargar "https://mediafire.com/file/XXXX/archivo.rar/file" --password "x"
darkdm descargar "https://cdn.ejemplo.com/video.mp4" --threads 8
darkdm info "https://www.youtube.com/watch?v=..."
```

### Patrones de diseño (2000+ líneas de spec)

| Patrón | Descripción |
|--------|-------------|
| **Pipeline** | Stages independientes: Resolver → Extraer → Descargar → Post-procesar |
| **State Machine** | 12 estados con transiciones explícitas (QUEUED → RESOLVING → DOWNLOADING → ...) |
| **Plugin Registry** | Extractores por sitio: YouTube, MediaFire, Mega, Google Drive + fallback genérico |
| **Event System** | `broadcast::channel` para terminal (indicatif) y GUI (Tauri events) |
| **Retry Backoff** | 5 intentos con backoff exponencial 1s→16s + jitter |
| **Segmented Download** | Multi-hilo tipo IDM con Range headers |
| **Queue Manager** | Cola FIFO con límite de concurrentes |

### Dependencias planeadas

```toml
clap = "4"          # CLI argument parser
reqwest = "0.12"    # HTTP client nativo
scraper = "0.20"    # HTML parsing (CSS selectors)
indicatif = "0.17"  # Barra de progreso en terminal
tokio = "1"         # Async runtime
zip = "2"           # Extracción ZIP
tar = "0.4"         # Extracción tar
```

## Estructura del proyecto

```
├── src-tauri/              ← Tauri Desktop App (Svelte + Rust)
│   ├── src/lib.rs          ← Lógica Tauri (listar descargas)
│   └── src-tauri/src/      ← Binarios y engine (futuro)
│
├── native-host/            ← Rust HTTP server + CLI
│   ├── src/server.rs       ← HTTP server en :8765
│   ├── src/downloader.rs   ← Engine HLS/DASH
│   ├── src/bin/cli.rs      ← CLI para HLS
│   └── src/bin/proxy*.rs   ← Proxies de captura
│
├── extension/              ← Chrome MV3 Extension
│   ├── background.js       ← Detección de streams
│   ├── content.js          ← Overlay de descarga
│   └── popup/             ← UI de streams detectados
│
├── scripts/                ← Bash scripts funcionales
│   ├── darkdm-mediafire    ← Descarga desde MediaFire
│   ├── darkdm-capture      ← Captura TLS nativa
│   ├── darkdm-debug        ← Logs en tiempo real
│   ├── darkdm-vivaldi      ← Vivaldi con SSLKEYLOGFILE
│   └── watch-downloads.sh  ← Monitor de descargas
│
├── openspec/               ← Spec-driven development
│   └── changes/native-cli/ ← Spec del CLI nativo (2000+ líneas)
│
├── docs/                   ← Documentación
├── dist/                   ← Build output
├── init.sh                 ← Verificación del proyecto
├── install.sh              ← Instalación de extensión + host
├── feature_list.json       ← Estado de features
└── progress.md             ← Log de sesiones
```

## Instalación

```bash
# 1. Clonar
git clone https://github.com/JDis/darkdm && cd darkdm

# 2. Verificar entorno
./init.sh

# 3. Scripts (disponibles en ~/.local/bin/)
export PATH="$PATH:$PWD/scripts"
cp scripts/darkdm-mediafire ~/.local/bin/

# 4. Native host
cd native-host && cargo build --release
cp target/release/darkdm-host ~/.local/bin/

# 5. Extensión de Chrome
# vivaldi://extensions → Load unpacked → seleccionar extension/
```

## Dependencias

```bash
# Esenciales (scripts bash)
sudo pacman -S curl unrar p7zip

# Native host + CLI Rust
sudo pacman -S ffmpeg           # Para HLS
sudo pacman -S rust cargo       # Para compilar native-host

# YouTube (opcional, para el plugin futuro)
pip install yt-dlp
```

## Roadmap

```
Fase 1  [✅] Bash scripts funcionales (darkdm-mediafire con --get-link, --password, fix gzip, resume)
Fase 2  [✅] Spec completo del CLI nativo (2000+ líneas, 8 patrones)
Fase 3  [🔜] Engine de descarga universal en Rust (reqwest, multi-hilo, resume)
Fase 4  [  ] CLI con clap (descargar, info, --json, --interactive)
Fase 5  [  ] Page Analyzer genérico + plugins (MediaFire, YouTube, Mega)
Fase 6  [  ] Integración con Tauri (engine compartido, eventos de progreso)
Fase 7  [  ] Reemplazar todos los scripts bash por el CLI nativo
```

## Licencia

MIT
