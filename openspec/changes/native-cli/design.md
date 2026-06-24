# DarkDM Native CLI — Spec

## Context

### Cómo funciona IDM realmente

IDM no tiene scrapers por sitio. Su modelo real es:

```
1. Browser extension monitorea TODAS las peticiones HTTP
2. Cuando detecta una descarga (por content-type, extensión, o tamaño):
   → Captura: URL + Method + Headers (User-Agent, Referer, Cookie, etc.)
   → Lo envía a IDM engine
3. IDM engine descarga con:
   → Multi-hilo (particiona el archivo)
   → Resume automático
   → Programación
4. El engine NO parsea HTML — la extensión ya le dio la URL directa
```

Los scrapers site-specific de IDM (para MediaFire, etc.) son solo un extra menor — NO son el core.

### El core de DarkDM debe ser igual

La **extensión de Chrome** es la que captura las URLs. El CLI/engine solo necesita:

1. **Descargar una URL directa** (con headers personalizados)
2. **Hacerlo bien**: multi-hilo, resume, progreso, timeouts
3. **Aceptar URLs desde cualquier fuente**: extensión, CLI manual, Tauri GUI

Los extractors site-specific (MediaFire, etc.) son **plugins opcionales** para cuando pegas una URL de página web — no al revés.

### Qué tenemos hoy

| Componente | Rol | Problema |
|---|---|---|
| `scripts/darkdm-mediafire` (bash) | Script específico para MediaFire | Bash frágil, sinute, engine no compartido |
| `native-host/src/bin/cli.rs` (Rust) | Descarga HLS (.m3u8) | Solo HLS, llama a ffmpeg/curl |
| `native-host/src/server.rs` (Rust) | HTTP server para extensión Chrome | Usa curl, no tiene progreso real |
| `native-host/src/downloader.rs` (Rust) | Engine HLS/DASH con fetch_url | Usa curl, no reqwest |

**Ninguno descarga genéricamente bien** — todos tienen limitaciones.

## Goals / Non-Goals

### Goals

1. **Engine de descarga universal** (Rust nativo) que:
   - Descarga cualquier URL directa (video, audio, binario, documento)
   - Multi-hilo (particionado de archivos)
   - Resume automático con Range headers
   - Headers personalizados (User-Agent, Referer, Cookie)
   - Timeout granular (por conexión + total)
   - Callbacks de progreso (para terminal y GUI)
   - Extracción automática de archives (ZIP/RAR/7z/tar)

2. **CLI unificado** `darkdm <url>` que:
   - Usa el engine directamente
   - Detecta automáticamente el tipo de contenido
   - Acepta headers custom (`--referer`, `--user-agent`, `--cookie`)
   - Barra de progreso en terminal
   - Output JSON para integraciones

3. **Site-extractors como plugins** (no como core):
   - Si la URL devuelve HTML (no un archivo), el CLI intenta extraer el link real
   - Estrategias de extracción genéricas:
     - Buscar `<video>` / `<audio>` / `<source>` en la página
     - Buscar enlaces con extensiones de archivo comunes
     - Detectar scripts con configuraciones de video (window.__NUXT__, etc.)
   - Extractores específicos (MediaFire, Mega, Google Drive) son plugins opcionales

4. **Integración con extensión Chrome**:
   - La extensión captura la URL + headers → engine descarga
   - Sin scraping — la extensión ya hizo el trabajo

5. **Engine compartido** entre CLI, Tauri app, y HTTP server

### Non-Goals

- GUI (lo maneja Svelte/Tauri)
- Captura de tráfico de red (SSLKEYLOGFILE)
- DRM (Netflix, Disney+, etc.)
- Torrents / magnet links
- Soporte Windows/macOS (por ahora)

## Architecture

```
                    ┌─────────────────────────────────────────┐
                    │         darkdm CLI / Tauri App          │
                    │                                         │
                    │  $ darkdm "https://ejemplo.com/video"   │
                    │  $ darkdm "https://mediafire.com/..."   │
                    └──────────┬──────────────────────────────┘
                               │
                    ┌──────────▼──────────────────────────────┐
                    │        Download Orchestrator             │
                    │                                         │
                    │  1. ¿Ya es URL directa?                  │
                    │     → saltar a paso 3                    │
                    │                                         │
                    │  2. ¿Es página web? Intentar extraer:    │
                    │     ┌─────────────────────────────┐      │
                    │     │ Page Analyzer (genérico)     │      │
                    │     │ • Busca <video>/<audio> src  │      │
                    │     │ • Busca .mp4/.mkv/.avi href  │      │
                    │     │ • Busca .m3u8/.mpd manifest  │      │
                    │     │ • Busca window.__data__ etc  │      │
                    │     └─────────────────────────────┘      │
                    │     │                                    │
                    │     ┌─────────────────────────────┐      │
                    │     │ Site Plugins (opcional)      │      │
                    │     │ • MediaFire: download link   │      │
                    │     │ • Mega: api key + decrypt    │      │
                    │     │ • Google Drive: confirm dl   │      │
                    │     └─────────────────────────────┘      │
                    │                                         │
                    │  3. Descargar URL directa:               │
                    │     ┌─────────────────────────────┐      │
                    │     │ Download Engine              │      │
                    │     │ • reqwest streaming          │      │
                    │     │ • Multi-hilo (particionado)  │      │
                    │     │ • Resume (Range headers)     │      │
                    │     │ • Headers custom             │      │
                    │     └─────────────────────────────┘      │
                    │                                         │
                    │  4. ¿Es archive? Extraer:               │
                    │     ┌─────────────────────────────┐      │
                    │     │ Extractor                    │      │
                    │     │ • ZIP (zip crate)            │      │
                    │     │ • RAR (unrar CLI)            │      │
                    │     │ • 7z (7z CLI)                │      │
                    │     │ • tar.gz/xz (tar crate)      │      │
                    │     └─────────────────────────────┘      │
                    └──────────────────────────────────────────┘
```

## Comportamiento del CLI

### `darkdm descargar <url> [destino]`

```bash
# ─── Modo IDM: URL directa ───
darkdm descargar "https://cdn.ejemplo.com/video.mp4"
darkdm descargar "https://cdn.ejemplo.com/video.mp4" ~/Videos
darkdm descargar "https://cdn.ejemplo.com/video.mp4" --referer "https://sitio.com" --user-agent "Mozilla/5.0"
darkdm descargar "https://cdn.ejemplo.com/video.mp4" --cookie "session=abc123"
darkdm descargar "https://cdn.ejemplo.com/video.mp4" --threads 8

# ─── Modo página web (extracción automática) ───
darkdm descargar "https://mediafire.com/file/XXXX/archivo.rar/file"
darkdm descargar "https://mediafire.com/file/XXXX/archivo.rar/file" --password "mipass"

# ─── HLS / DASH ───
darkdm descargar "https://cdn.ejemplo.com/stream.m3u8"
darkdm descargar "https://cdn.ejemplo.com/manifest.mpd"

# ─── Solo info ───
darkdm info "https://cdn.ejemplo.com/video.mp4"
# → Nombre: video.mp4 | Tamaño: 1.2 GB | Tipo: video/mp4 | Resumen: yes

# ─── Output para scripting ───
darkdm descargar "https://..." --json
```

### `darkdm info <url>`

Muestra información del archivo sin descargar:
- Nombre del archivo
- Tamaño
- Content-Type
- ¿Soporta Range? (para resume)
- Si es página web: qué enlaces detecta

### Flags

| Flag | Descripción | Default |
|---|---|---|
| `--dir <path>` | Directorio de descarga | `~/Descargas/DarkDM` |
| `--password <p>` | Contraseña para archives | — |
| `--referer <url>` | Header Referer | — |
| `--user-agent <ua>` | Header User-Agent | Chrome 124 |
| `--cookie <str>` | Header Cookie | — |
| `--header <k:v>` | Header custom (multi) | — |
| `--threads <n>` | Número de hilos | 4 |
| `--resume` | Reanudar si hay archivo parcial | true |
| `--keep-archive` | Conservar .rar/.zip tras extraer | true |
| `--video-only` | Solo extraer videos del archive | false |
| `--timeout <secs>` | Timeout por conexión | 30 |
| `--max-time <secs>` | Timeout total | 3600 |
| `--get-link` | Solo mostrar link directo | false |
| `--quiet` | Sin output a terminal | false |
| `--json` | Output en JSON | false |

## Download Engine

```rust
pub struct DownloadOptions {
    pub url: String,
    pub output_dir: PathBuf,
    pub filename: Option<String>,
    pub password: Option<String>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
    pub cookies: Option<String>,
    pub extra_headers: Vec<(String, String)>,
    pub num_threads: u32,
    pub resume: bool,
    pub keep_archive: bool,
    pub video_only: bool,
    pub connection_timeout: u64,
    pub max_time: u64,
}

pub struct ProgressInfo {
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,  // None si no se sabe
    pub speed: f64,                 // bytes/sec
    pub eta: Option<Duration>,
    pub phase: DownloadPhase,       // Connecting | Downloading | Extracting | Done
}

pub enum DownloadPhase {
    Resolving,
    Connecting,
    Downloading,
    Extracting,
    Done,
}

pub enum DownloadResult {
    Success {
        output_path: PathBuf,
        files: Vec<PathBuf>,
        total_bytes: u64,
        duration: Duration,
        extracted: bool,
    },
    Cancelled {
        partial_path: PathBuf,
        downloaded_bytes: u64,
    },
    Error {
        error: String,
        partial_path: Option<PathBuf>,
    },
}
```

### Flujo del engine

```
recibir URL
  │
  ├─ ¿Content-Type es application/x-mpegURL o .m3u8?
  │    → download_hls()
  │
  ├─ ¿Content-Type es application/dash+xml o .mpd?
  │    → download_dash()
  │
  ├─ ¿Content-Type empieza con video/ audio/ application/octet?
  │  ¿O tiene extensión de archivo?
  │    → download_direct()
  │
  ├─ ¿Content-Type es text/html?
  │    → page_analyzer() busca enlaces
  │    → si no encuentra → error "no se pudo extraer link"
  │    → si encuentra → download_direct()
  │
  └─ Si no se puede determinar:
       → download_direct() (intenta igual)
```

### Page Analyzer (genérico, no específico)

```rust
pub fn analyze_page(html: &str, page_url: &Url) -> Vec<DetectedLink> {
    // Estrategias de extracción, en orden:
    
    // 1. Buscar <video> <source> src
    // 2. Buscar <a href="*.mp4">, <a href="*.mkv">, etc
    // 3. Buscar <a href="*.m3u8">, <a href="*.mpd">
    // 4. Buscar scripts con config: window.__NUXT__, window.__INITIAL_STATE__
    // 5. Buscar meta[property="og:video"]
    // 6. Buscar iframe[src] con videos
    // 7. Intentar site-plugins (MediaFire, etc.)
}

pub struct DetectedLink {
    pub url: String,
    pub source: LinkSource,  // VideoTag | Anchor | Script | SitePlugin
    pub filename: Option<String>,
    pub quality: Option<String>,
}
```

### Site Plugins (opcionales, registrables)

```rust
pub trait SitePlugin {
    fn name(&self) -> &'static str;
    fn matches(&self, url: &Url) -> bool;
    fn extract(&self, page_html: &str, page_url: &Url) -> Result<Vec<DetectedLink>, String>;
}

// Plugins built-in:
// - MediaFirePlugin: busca #downloadButton href
// - MegaPlugin: API key + decrypt
// - GoogleDrivePlugin: confirma descarga
// - YoutubePlugin: yt-dlp integration
```

## Multi-hilo (como IDM)

IDM descarga archivos en **particiones** con múltiples conexiones:

```
Archivo: 100 MB
Thread 1: bytes 0-25M
Thread 2: bytes 25M-50M
Thread 3: bytes 50M-75M
Thread 4: bytes 75M-100M
```

El engine debe:
1. Hacer HEAD request para obtener tamaño total
2. Verificar que el servidor soporta Range headers
3. Dividir en N partes iguales
4. Descargar cada parte en paralelo
5. Ensamblar al final

```rust
pub fn download_with_segments(
    url: &str,
    total_size: u64,
    num_threads: u32,
    output: &Path,
    progress: impl Fn(ProgressInfo),
) -> Result<(), String> {
    let segment_size = total_size / num_threads as u64;
    
    // Lanzar N threads, cada uno con su Range
    for i in 0..num_threads {
        let start = i as u64 * segment_size;
        let end = if i == num_threads - 1 { total_size - 1 } else { start + segment_size - 1 };
        // GET con Range: bytes={start}-{end}
        // Escribir en archivo temporal
        // Reportar progreso
    }
    
    // Ensamblar archivos temporales
}
```

## Resume automático

```
1. Verificar si ya existe archivo parcial en output_dir
2. Comparar tamaño con Content-Length del servidor
3. Si es menor → GET con Range: bytes={existing_size}-
4. Append al archivo existente
```

## Extracción de archives

```rust
pub fn extract(path: &Path, password: Option<&str>) -> Result<Vec<PathBuf>, String> {
    match detect_archive_type(path) {
        ArchiveType::Zip => extract_zip(path),
        ArchiveType::Rar => extract_rar(path, password),
        ArchiveType::SevenZ => extract_7z(path, password),
        ArchiveType::TarGz => extract_targz(path),
        ArchiveType::TarXz => extract_tarxz(path),
        ArchiveType::None => Ok(vec![]),  // no es archive
    }
}
```

## Integración con Tauri

```
src-tauri/
  src/
    lib.rs                    ← Tauri app + engine público
    bin/cli.rs               ← darkdm CLI
    downloader/
      mod.rs                  ← Orchestrator principal
      engine.rs               ← reqwest download core
      segmented.rs            ← Multi-hilo con Range
      resume.rs               ← Resume logic
      hls.rs                  ← HLS parser/downloader
      dash.rs                 ← DASH parser/downloader
      page_analyzer.rs        ← Generic page scraper
      plugins/
        mod.rs                ← Plugin trait
        mediafire.rs          ← MediaFire plugin
        mega.rs               ← Mega plugin (futuro)
      extract.rs              ← Archive extraction
      progress.rs             ← Progress types + callbacks
```

```rust
// lib.rs
pub mod downloader;

use downloader::{descargar, DownloadOptions};

#[tauri::command]
fn download_url(url: String, dir: Option<String>, password: Option<String>) {
    let options = DownloadOptions {
        url,
        output_dir: dir.map(PathBuf::from).unwrap_or_default(),
        password,
        ..Default::default()
    };
    
    // Lanzar en thread separado, reportar progreso via eventos Tauri
    tauri::async_runtime::spawn(async move {
        let result = descargar(options).await;
        // Emitir evento "download-complete"
    });
}
```

## Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", features = ["stream"] }
scraper = "0.20"           # HTML parsing (page analyzer)
indicatif = "0.17"         # Progress bar terminal
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
zip = "2"                  # ZIP extraction
tar = "0.4"                # tar.gz/xz
flate2 = "1"               # gzip
xz2 = "0.1"               # xz
url = "2"                  # URL parsing
mime_guess = "2"           # Content-Type detection
chrono = "0.4"             # Timestamps
```

## Decisions

### 1. Core = download engine, no scrapers

**Decisión**: El core del CLI es un **download engine universal** que acepta URLs directas. La extracción de páginas web es una feature secundaria.

**Rationale**: Como IDM, la extensión de Chrome es la que captura las URLs con todos sus headers. El engine solo necesita descargar bien.

### 2. reqwest nativo, no curl

**Decisión**: Usar `reqwest` para todo el HTTP.

**Rationale**: Control total (headers, streaming, Range), sin dependencias externas, comparte tipos con Tauri.

### 3. Multi-hilo con Range (como IDM)

**Decisión**: Implementar particionado de archivos con Range headers.

**Rationale**: IDM descarga archivos grandes ~3x más rápido con 4-8 hilos. reqwest soporta Range nativamente.

### 4. Page Analyzer genérico, plugins site-specific

**Decisión**: El page analyzer busca enlaces genéricamente (video tags, anchors, scripts). Los site-plugins (MediaFire, Mega) son solo un extra.

**Rationale**: No podemos tener scrapers para cada sitio del mundo. Lo genérico cubre el 90%. Los plugins cubren sitios populares.

### 5. Resumen de sesión de descarga

**Output del CLI**:
```bash
$ darkdm descargar "https://mediafire.com/file/XXXX/archivo.rar/file"

Resolviendo... → MediaFire detectado
Extrayendo enlace directo...
  → https://download1350.mediafire.com/.../archivo.rar

Descargando archivo.rar (2.7 GB)
  ━━━━━━━━━━━━━━━━━━━━━━╸━━━━━━ 78% • 12.3 MB/s • ETA: 1:23
  Threads: 4/4 activos

✅ Descarga completa (2.7 GB en 2:34)
📦 Extrayendo... (contraseña: sí)
  → video.mp4 (1.1 GB)
  → archivo.rar conservado en ~/Descargas/DarkDM/
```

### 6. Output JSON para todo

Con `--json`, cualquier comando debe output JSON parseable:

```bash
darkdm descargar "https://..." --json
# {"status":"success","files":[{"path":"...","size":123}],"duration":154}
```

## Risks / Trade-offs

- **[Riesgo] Servidores sin Range** → Muchos CDNs no soportan Range (o solo en ciertos casos). Mitigación: detectar Accept-Ranges, fallback a single-thread.
- **[Riesgo] Rate limiting con multi-hilo** → Algunos servidores limitan conexiones simultáneas. Mitigación: --threads configurable, default conservador (4).
- **[Riesgo] Content-Length dinámico** → Algunos streams cambian de tamaño. Mitigación: si Content-Length cambia, reiniciar sin Range.
- **[Trade-off] Rust compile time** → Más lento que bash. Pero se compila una vez y corre sin dependencias.

## Definition of Done

- [ ] `darkdm descargar <direct_url>` descarga con progreso
- [ ] `darkdm descargar <direct_url> --threads 8` multi-hilo
- [ ] `darkdm descargar <direct_url> --resume` reanuda si se cortó
- [ ] `darkdm descargar <mediafire_url>` extrae link + descarga
- [ ] `darkdm descargar <mediafire_url> --password x` extrae RAR
- [ ] `darkdm descargar <hls_url>` descarga stream HLS
- [ ] `darkdm info <url>` muestra info sin descargar
- [ ] `darkdm descargar <url> --json` output JSON
- [ ] `./init.sh` pasa
- [ ] Engine funciona desde Tauri
