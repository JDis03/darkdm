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
pub trait SitePlugin: Send + Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, url: &Url) -> bool;
    fn extract(&self, page_html: &str, page_url: &Url) -> Result<Vec<DetectedLink>, String>;
}

// Plugins built-in (registrados en orden de prioridad):
// 1. YoutubePlugin → llama a yt-dlp
// 2. MediaFirePlugin → busca #downloadButton href
// 3. MegaPlugin → API key + decrypt (futuro)
// 4. GoogleDrivePlugin → confirma descarga (futuro)
```

---

### Plugin YouTube (vía yt-dlp)

YouTube **no se puede scrapear**. No hay `<video src>` en el HTML, no hay manifests públicos sin autenticación, el contenido está cifrado en segmentos. La única forma realista es usar **yt-dlp**, el estándar de facto.

#### Cómo funciona yt-dlp

yt-dlp (fork de youtube-dl) hace todo el trabajo pesado:
1. Descifra la página de YouTube (JS, cifrado, signature decoding)
2. Obtiene los manifests DASH/HLS reales
3. Selecciona el mejor formato (video + audio separados)
4. Los descarga y los multiplexa en un solo archivo

Ejemplo de comandos yt-dlp:

```bash
# Obtener lista de formatos disponibles
yt-dlp -F "https://youtube.com/watch?v=xxxx"

# Obtener URL directa del mejor formato (sin descargar)
yt-dlp -g --format "bestvideo+bestaudio/best" "https://youtube.com/watch?v=xxxx"

# Descargar y convertir a mp4
yt-dlp --format "bestvideo+bestaudio/best" --merge-output-format mp4 \
  -o "~/Descargas/DarkDM/%(title)s.%(ext)s" "https://youtube.com/watch?v=xxxx"
```

#### Plugin: casos de uso

```bash
# ─── Descargar video (best quality automático) ───
darkdm descargar "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
# 1. Plugin detecta youtube.com
# 2. Llama a yt-dlp con format best
# 3. yt-dlp descarga video+audio, merge a mp4
# 4. → ~/Descargas/DarkDM/Rick Astley - Never Gonna Give You Up.mp4

# ─── YouTube + playlist ───
darkdm descargar "https://youtube.com/playlist?list=PL..."
# → yt-dlp descarga toda la playlist

# ─── YouTube + formato específico ───
darkdm descargar "https://youtube.com/watch?v=xxxx" --yt-format "bestvideo[height<=1080]+bestaudio/best[height<=1080]"

# ─── YouTube + solo audio ───
darkdm descargar "https://youtube.com/watch?v=xxxx" --yt-audio-only

# ─── YouTube + subtítulos ───
darkdm descargar "https://youtube.com/watch?v=xxxx" --yt-subs

# ─── YouTube + info (sin descargar) ───
darkdm info "https://youtube.com/watch?v=xxxx"
# → Título: Rick Astley - Never Gonna Give You Up
# → Duración: 3:32
# → Formatos: 18 (360p), 22 (720p), 137+140 (1080p DASH)
# → Mejor: 1080p DASH (video+audio)

# ─── YouTube + búsqueda ───
darkdm descargar "ytsearch:rick astley never gonna"
# → yt-dlp busca en YouTube y descarga el primer resultado

# ─── YouTube + lista de URLs ───
darkdm descargar "https://youtube.com/watch?v=xxxx" "https://youtube.com/watch?v=yyyy"
# → Cola de descargas

# ─── Otros sitios soportados por yt-dlp ───
darkdm descargar "https://vimeo.com/123456789"
darkdm descargar "https://www.tiktok.com/@user/video/123456789"
darkdm descargar "https://twitter.com/user/status/123456789"
darkdm descargar "https://www.twitch.tv/clips/..."
darkdm descargar "https://www.instagram.com/p/..."
```

#### Flags específicos de YouTube

| Flag | Descripción | Default |
|---|---|---|
| `--yt-format <fmt>` | Formato específico de yt-dlp | `bestvideo+bestaudio/best` |
| `--yt-audio-only` | Solo descargar audio (mp3) | false |
| `--yt-subs` | Descargar subtítulos | false |
| `--yt-playlist-start <n>` | Empezar desde el item N de la playlist | 1 |
| `--yt-playlist-end <n>` | Terminar en el item N de la playlist | — |
| `--yt-output-template <t>` | Template de nombre de archivo | `%(title)s.%(ext)s` |

#### Plugin: implementación

```rust
use std::process::Command;
use std::path::PathBuf;

pub struct YoutubePlugin;

impl SitePlugin for YoutubePlugin {
    fn name(&self) -> &'static str {
        "youtube-dlp"
    }

    fn matches(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("");
        // YouTube
        host.contains("youtube.com") || host == "youtu.be" ||
        // yt-dlp search
        url.as_str().starts_with("ytsearch") ||
        // Otros sitios populares que yt-dlp soporta
        host.contains("vimeo.com") ||
        host.contains("twitch.tv") ||
        host.contains("tiktok.com") ||
        host.contains("twitter.com") ||
        host.contains("x.com") ||
        host.contains("instagram.com") ||
        host.contains("facebook.com") ||
        host.contains("dailymotion.com")
    }

    fn extract(&self, page_html: &str, page_url: &Url) -> Result<Vec<DetectedLink>, String> {
        // Para yt-dlp no necesitamos el HTML del todo —
        // yt-dlp obtiene la info por su cuenta
        Ok(vec![DetectedLink {
            url: page_url.as_str().to_string(),
            source: LinkSource::SitePlugin("youtube-dlp"),
            filename: None,
            quality: None,
        }])
    }
}

pub struct YoutubeOptions {
    pub format: Option<String>,         // --yt-format
    pub audio_only: bool,               // --yt-audio-only
    pub subs: bool,                     // --yt-subs
    pub playlist_start: Option<u32>,    // --yt-playlist-start
    pub playlist_end: Option<u32>,      // --yt-playlist-end
    pub output_template: Option<String>,// --yt-output-template
}

impl Default for YoutubeOptions {
    fn default() -> Self {
        Self {
            format: Some("bestvideo+bestaudio/best".into()),
            audio_only: false,
            subs: false,
            playlist_start: None,
            playlist_end: None,
            output_template: Some("%(title)s.%(ext)s".into()),
        }
    }
}

/// Descarga con yt-dlp y reporta progreso
pub fn download_with_ytdlp(
    url: &str,
    options: &YoutubeOptions,
    output_dir: &Path,
    progress: impl Fn(ProgressInfo),
) -> Result<DownloadResult, String> {
    // Verificar que yt-dlp existe
    let which = Command::new("which")
        .arg("yt-dlp")
        .output()
        .map_err(|_| "yt-dlp no está instalado. Instálalo con:\n  pip install yt-dlp\n  # o\n  sudo pacman -S yt-dlp".to_string())?;

    if !which.status.success() {
        return Err("yt-dlp no está instalado".to_string());
    }

    let output_template = options.output_template
        .clone()
        .unwrap_or_else(|| "%(title)s.%(ext)s".into());
    let output_path = output_dir.join(&output_template);

    let mut cmd = Command::new("yt-dlp");
    cmd.args([
        "--format", options.format.as_deref().unwrap_or("bestvideo+bestaudio/best"),
        "--merge-output-format", "mp4",
        "--output", output_path.to_str().unwrap_or(""),
        "--progress-template", "console:%(progress.eta)s %(progress.speed)s",
        "--no-mtime",                        // no modificar timestamp
        "--embed-thumbnail",                 // thumbnail en el archivo
        "--embed-metadata",                  // metadatos en el archivo
    ]);

    if options.audio_only {
        cmd.args(["--extract-audio", "--audio-format", "mp3"]);
    }

    if options.subs {
        cmd.args(["--write-subs", "--sub-langs", "all"]);
    }

    if let Some(start) = options.playlist_start {
        cmd.args(["--playlist-start", &start.to_string()]);
    }
    if let Some(end) = options.playlist_end {
        cmd.args(["--playlist-end", &end.to_string()]);
    }

    // Cookies del navegador (para contenido restringido por edad)
    cmd.args(["--cookies-from-browser", "vivaldi"]);

    cmd.arg(url);

    // Ejecutar y reportar progreso
    progress(ProgressInfo {
        bytes_downloaded: 0,
        total_bytes: None,
        speed: 0.0,
        eta: None,
        phase: DownloadPhase::Downloading,
    });

    let status = cmd.status()
        .map_err(|e| format!("Error ejecutando yt-dlp: {}", e))?;

    if !status.success() {
        return Err(format!("yt-dlp falló con código: {}", status));
    }

    // Encontrar el archivo descargado
    let files = find_files_in_dir(output_dir);
    let total_size: u64 = files.iter()
        .filter_map(|f| std::fs::metadata(f).ok().map(|m| m.len()))
        .sum();

    Ok(DownloadResult {
        output_path: output_dir.to_path_buf(),
        files,
        total_bytes: total_size,
        duration: Duration::from_secs(0),
        extracted: false,
    })
}

/// Función info para YouTube (sin descargar)
pub fn get_ytdlp_info(url: &str) -> Result<serde_json::Value, String> {
    let output = Command::new("yt-dlp")
        .args(["--dump-json", "--no-download", url])
        .output()
        .map_err(|e| format!("yt-dlp falló: {}", e))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Error parseando JSON de yt-dlp: {}", e))
}
```

#### Output de ejemplo

```bash
$ darkdm descargar "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

🔍 Detectado: YouTube (vía yt-dlp)
📹 Rick Astley - Never Gonna Give You Up
   Duración: 3:32 | Mejor calidad: 1080p DASH

⬇️  Descargando...
[youtube] dQw4w9WgXcQ: Downloading webpage
[youtube] dQw4w9WgXcQ: Downloading android player API JSON
[download] Destination: Rick Astley - Never Gonna Give You Up.f137.mp4
[download] ━━━━━━━━━━━━━━━━━━━╸━━━━━ 65% • 8.2 MB/s • ETA: 0:45
[download] Destination: Rick Astley - Never Gonna Give You Up.f140.m4a
[download] ━━━━━━━━━━━━━━━━━━━━━━━━━ 100% • 3.4 MB/s
[Merger] Merging video + audio into Rick Astley - Never Gonna Give You Up.mp4

✅ Completo: ~/Descargas/DarkDM/Rick Astley - Never Gonna Give You Up.mp4
   Tamaño: 42 MB | Duración: 3:32 | 1080p
```

```bash
$ darkdm info "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

📹 Rick Astley - Never Gonna Give You Up
   Canal: Rick Astley
   Duración: 3:32
   Publicado: 2009-10-25
   Vistas: 1.5B
   
Formatos disponibles:
  18  360p  mp4      video+audio  22 MB
  22  720p  mp4      video+audio  45 MB
  137 1080p mp4      video only   38 MB  ← mejor video
  140       m4a      audio only    3 MB  ← mejor audio
  💡 Recomendado: 137+140 (1080p DASH, 42 MB)
```

#### yt-dlp: dependencia opcional

A diferencia del resto del engine que es Rust nativo, YouTube **requiere yt-dlp**.

```bash
# Instalación
pip install yt-dlp
# o
sudo pacman -S yt-dlp
# o
sudo apt install yt-dlp
```

Si no está instalado:

```bash
darkdm descargar "https://youtube.com/watch?v=xxxx"
# ⚠️ yt-dlp no está instalado
# → Para descargar de YouTube necesitas yt-dlp:
#   pip install yt-dlp
# → O usa la extensión de Chrome (no requiere yt-dlp)
```

#### Compatibilidad: más que YouTube

yt-dlp soporta **más de 1000 sitios**. El plugin YouTube también funcionará para:

| Sitio | URL de ejemplo |
|-------|----------------|
| YouTube | `youtube.com/watch?v=...` |
| Vimeo | `vimeo.com/123456789` |
| TikTok | `tiktok.com/@user/video/...` |
| Twitter/X | `twitter.com/user/status/...` |
| Twitch | `twitch.tv/videos/...` |
| Instagram | `instagram.com/p/...` |
| Facebook | `facebook.com/watch/...` |
| Dailymotion | `dailymotion.com/video/...` |
| Bilibili | `bilibili.com/video/...` |
| Reddit | `reddit.com/r/.../comments/...` |

Cualquier URL que yt-dlp soporte, DarkDM también.

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
