# DarkDM Native CLI — Spec

## Context

### Referencia: IDM (Internet Download Manager)

IDM es el estándar de facto en Windows. Su modelo es:

1. El usuario copia un link o hace clic en "Descargar con IDM"
2. IDM intercepta el link y abre su UI
3. IDM analiza la página/sitio para obtener el link directo
4. Descarga con **multi-hilo** (hasta 32 conexiones), **resume** automático
5. Puede programar descargas, capturar videos (HLS/DASH), etc.

### DarkDM hoy

DarkDM tiene **3 componentes separados** con el mismo propósito:

| Componente | Rol | Problema |
|---|---|---|
| `scripts/darkdm-mediafire` (bash) | Descarga desde MediaFire | Bash frágil, sin progreso nativo, depende de curl |
| `native-host/src/bin/cli.rs` (Rust) | Descarga HLS (.m3u8) | Solo HLS, llama a ffmpeg/curl, no integrado con Tauri |
| `native-host/src/downloader.rs` (Rust) | Engine HLS/DASH | No se usa desde CLI, solo desde el HTTP server |

**No hay un CLI unificado** que haga todo. El usuario tiene que saber qué script usar según el tipo de contenido.

### Problemas del bash actual (`darkdm-mediafire`)

1. **curl sin --compressed** → página MediaFire gzip no se parseaba (ya fixeado)
2. **grep frágil** → el HTML de MediaFire cambia, regex se rompe
3. **Sin resume real** → curl -C - funciona a veces, pero no maneja errores bien
4. **Timeout hardcodeado** → 3min, 10min, ahora 1h... parche sobre parche
5. **Sin streaming de progreso** → la barra de curl no se puede capturar desde Tauri
6. **Dependencia de unrar** → hay que tenerlo instalado, versión específica
7. **Sin cola de descargas** → no se pueden encolar varias URLs
8. **Sin notificaciones** → cuando termina, no avisa

### ¿Por qué Rust nativo?

- **Tauri ya es Rust** → el CLI comparte el mismo engine que la app desktop
- **reqwest** → HTTP con streaming, resume, timeout granulado, redirects automáticos
- **scraper** → parseo de HTML robusto (CSS selectors, no regex)
- **indicatif** → barra de progreso nativa en terminal
- **clap** → CLI con autocomplete, flags, subcomandos
- **Compilado estáticamente** → sin dependencias externas (curl, unrar, etc.)
- **IPC con Tauri** → el mismo binario puede reportar progreso a la GUI

## Goals / Non-Goals

### Goals

- **CLI unificado** `darkdm descargar <url>` que funcione para cualquier tipo de contenido
- **MediaFire** → extraer link + descargar + extraer RAR/ZIP/7z (reemplazar `darkdm-mediafire`)
- **HLS** → descargar streams .m3u8 (reemplazar `darkdm-cli`)
- **Direct** → descarga directa de cualquier archivo (mp4, pdf, etc.)
- **Resume** automático si se interrumpe la descarga
- **--get-link** flag para solo extraer el link directo
- **--password** para archives protegidos
- **Multi-hilo** (opcional, configurable)
- **Barra de progreso** en terminal
- **Que funcione como CLI independiente y como backend de la app Tauri**

### Non-Goals

- GUI (eso lo maneja Tauri/Svelte)
- Captura de tráfico de red (SSLKEYLOGFILE, tcpdump)
- DRM (Netflix, Disney+, etc.)
- Soporte Windows/macOS por ahora (solo Linux)
- Parseo de torrent/magnet links

## Architecture

```
                    ┌─────────────────────────────────┐
                    │         darkdm CLI (Rust)        │
                    │                                  │
                    │  $ darkdm descargar <url>        │
                    │  $ darkdm --get-link <url>       │
                    │  $ darkdm --password "x" <url>   │
                    └──────────┬──────────────────────┘
                               │
                    ┌──────────▼──────────────────────┐
                    │     Download Engine (Rust)       │
                    │                                  │
                    │  ┌────────────────────────┐      │
                    │  │  Link Extractor         │      │
                    │  │  • MediaFire scraper    │      │
                    │  │  • HLS parser (.m3u8)   │      │
                    │  │  • Direct URL (passthru)│      │
                    │  └────────┬───────────────┘      │
                    │           │                       │
                    │  ┌────────▼───────────────┐      │
                    │  │  HTTP Downloader        │      │
                    │  │  • reqwest streaming    │      │
                    │  │  • Resume automático    │      │
                    │  │  • Multi-hilo (aria2)   │      │
                    │  │  • Progress callback    │      │
                    │  └────────┬───────────────┘      │
                    │           │                       │
                    │  ┌────────▼───────────────┐      │
                    │  │  Extractor              │      │
                    │  │  • RAR (unrar crate)    │      │
                    │  │  • ZIP (zip crate)      │      │
                    │  │  • 7z (externo o crate) │      │
                    │  │  • Video detection      │      │
                    │  └────────────────────────┘      │
                    └──────────────────────────────────┘
                               │
          ┌────────────────────┼────────────────────┐
          │                    │                     │
          ▼                    ▼                     ▼
   ┌────────────┐    ┌──────────────┐    ┌────────────────┐
   │ Terminal   │    │ Tauri App    │    │ Systemd        │
   │ (indicatif)│    │ (Svelte GUI) │    │ (daemon mode)  │
   └────────────┘    └──────────────┘    └────────────────┘
```

## CLI Commands

### `darkdm descargar <url> [destino]` (default)

Descarga el contenido de la URL. Detecta automáticamente el tipo.

```bash
# MediaFire → extrae link, descarga, extrae RAR
darkdm descargar "https://www.mediafire.com/file/XXXX/archivo.rar/file"
darkdm descargar "https://www.mediafire.com/file/XXXX/archivo.rar/file" ~/Pelis

# Con contraseña
darkdm descargar "https://www.mediafire.com/file/XXXX/archivo.rar/file" --password "mipass"

# Solo extraer link directo (sin descargar)
darkdm descargar "https://www.mediafire.com/file/XXXX/archivo.rar/file" --get-link
# Output: https://download1350.mediafire.com/.../archivo.rar

# HLS stream
darkdm descargar "https://cdn.example.com/stream.m3u8" video.mp4

# Download directo
darkdm descargar "https://example.com/video.mp4" ~/Descargas/
```

### `darkdm info <url>`

Muestra información del archivo sin descargar.

```bash
darkdm info "https://www.mediafire.com/file/XXXX/archivo.rar/file"
# Output:
#   Nombre: archivo.rar
#   Tamaño: 2.7 GB
#   Link directo: https://download1350.mediafire.com/...
```

### `darkdm lista`

Muestra el estado de las descargas activas (progreso, ETA, velocidad).

### `darkdm cancelar <id>`

Cancela una descarga en curso.

## Flags globales

| Flag | Descripción | Default |
|---|---|---|
| `--dir <path>` | Directorio de descarga | `~/Descargas/DarkDM` |
| `--password <p>` | Contraseña para archivos protegidos | — |
| `--get-link` | Solo extrae y muestra el link directo | false |
| `--keep-archive` | Conserva el .rar/.zip después de extraer | true |
| `--video-only` | Solo extrae videos, ignora otros archivos | false |
| `--threads <n>` | Número de hilos de descarga | 4 |
| `--resume` | Intentar reanudar descarga interrumpida | true |
| `--timeout <secs>` | Timeout por conexión | 30 |
| `--max-time <secs>` | Timeout total | 3600 |
| `--quiet` | Sin output | false |
| `--json` | Output en JSON (para integraciones) | false |

## Source detection (auto-detect)

El CLI debe detectar automáticamente el tipo de contenido:

1. **MediaFire** → URL contiene `mediafire.com/file/`
   - Scrapea HTML con `scraper` crate (CSS selectors)
   - Encuentra `#downloadButton` href
   - Extrae nombre del archivo del título/page

2. **HLS** → URL contiene `.m3u8`
   - Parses manifest, descarga segmentos, concatena
   - Si hay master playlist, selecciona mejor calidad

3. **Direct** → cualquier URL con extensión de archivo o content-type
   - Descarga directa con reqwest streaming

## Comportamiento de descarga

### Download Engine

```rust
pub struct DownloadOptions {
    pub url: String,
    pub output_dir: PathBuf,
    pub filename: Option<String>,
    pub password: Option<String>,
    pub keep_archive: bool,
    pub video_only: bool,
    pub num_threads: u32,
    pub resume: bool,
    pub timeout: u64,
    pub max_time: u64,
}

pub enum DownloadResult {
    Success {
        output_path: PathBuf,
        files: Vec<PathBuf>,  // todos los archivos generados
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

### MediaFire scraper

```rust
pub struct MediaFireLink {
    pub direct_url: String,
    pub filename: String,
    pub filesize: Option<u64>,
}

pub fn extract_mediafire_link(page_url: &str) -> Result<MediaFireLink, String> {
    // 1. Fetch page HTML con reqwest + User-Agent
    // 2. Parse con scraper crate
    // 3. Buscar #downloadButton → href
    // 4. Fallback: buscar en cualquier href que contenga download*.mediafire.com
    // 5. Extraer filename desde el nombre del archivo en el título
}
```

### Resume

Usar `reqwest` con headers `Range` y `Accept-Ranges`:

```rust
// Si el archivo parcial existe y el servidor soporta Range:
//   1. GET con Range: bytes=X-  (X = bytes descargados)
//   2. Append al archivo existente
//   3. Verificar integridad al final
```

### Extracción de archives

```rust
pub fn extract_archive(path: &Path, password: Option<&str>) -> Result<Vec<PathBuf>, String> {
    // Detectar tipo por extensión:
    // .rar → `unrar` crate o comando externo
    // .zip → `zip` crate
    // .7z  → crate externo o 7z CLI
    // .tar.gz, .tar.xz → `tar` crate
    // Retorna lista de archivos extraídos
}
```

## Integración con Tauri

El CLI debe ser un binario separado pero usar el mismo engine que la app Tauri:

```
src-tauri/
  src/
    lib.rs          ← Tauri app (frontend Svelte)
    cli.rs          ← punto de entrada para darkdm CLI binario
    downloader/     
      mod.rs        ← Download engine compartido
      mediafire.rs  ← MediaFire scraper
      hls.rs        ← HLS parser/descargador
      direct.rs     ← Descarga directa
      extract.rs    ← Extracción de archives
      progress.rs   ← Callbacks de progreso
```

### Binarios en Cargo.toml

```toml
[[bin]]
name = "darkdm"
path = "src/bin/cli.rs"       # CLI entry point

[[bin]]
name = "darkdm-host"          # HTTP server (ya existe)
path = "src/main.rs"

# El lib.rs sigue siendo la app Tauri
```

El engine vive en `lib.rs` y lo usan tanto el CLI como la app Tauri:

```rust
// lib.rs — engine compartido
pub mod downloader;

// src/bin/cli.rs — CLI
use darkdm::downloader;

fn main() {
    let args = clap::parse();
    downloader::descargar(args);
}

// src-tauri/src/lib.rs — Tauri app
use darkdm::downloader;

#[tauri::command]
fn download(url: String, password: Option<String>) {
    downloader::descargar(DownloadOptions { url, password });
}
```

## Implementation Plan

### Fase 1: Engine básico en Rust
- [ ] Migrar `downloader.rs` actual a `src-tauri/src/downloader/`
- [ ] Implementar descarga directa con `reqwest`
- [ ] Implementar MediaFire scraper con `scraper` crate
- [ ] Implementar extracción de archives (unrar, zip)
- [ ] Implementar barra de progreso con `indicatif`

### Fase 2: CLI
- [ ] Crear binario `darkdm` con `clap`
- [ ] Comando `descargar` completo
- [ ] Comando `info`
- [ ] Flag `--get-link`
- [ ] Flag `--json` para integraciones

### Fase 3: Resume y multi-hilo
- [ ] Implementar resume con Range requests
- [ ] Multi-hilo con segmentación de archivos
- [ ] Timeout granular (por conexión y total)

### Fase 4: Integración Tauri
- [ ] Eventos de progreso → frontend Svelte
- [ ] El CLI comparte engine con `lib.rs`

## Dependencies (crates)

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", features = ["stream"] }
scraper = "0.20"                                 # HTML parsing (MediaFire)
indicatif = "0.17"                               # Progress bar
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
zip = "2"                                        # ZIP extraction
tar = "0.4"                                      # tar.gz extraction
flate2 = "1"                                     # gzip decompression
xz = "0.2"                                       # xz decompression
# Nota: RAR requiere crate externo (unrust) o llamar a unrar CLI
```

## Decisions

### 1. `reqwest` sobre `curl`

**Decisión**: Usar `reqwest` (Rust nativo) en lugar de llamar a curl.

**Rationale**:
- Control total del HTTP (headers, timeouts, redirects, streaming)
- Resume con Range headers programático
- Sin dependencia externa (curl no necesita estar instalado)
- Compartir tipos con la app Tauri (mismo engine)

### 2. `scraper` para MediaFire (no regex)

**Decisión**: Usar el crate `scraper` con CSS selectors para parsear el HTML de MediaFire.

**Rationale**:
- CSS selectors son más robustos que regex para HTML
- `#downloadButton` es un selector estable
- Si MediaFire cambia, es fácil de actualizar
- El regex actual se rompe si cambia el orden de los atributos

### 3. `clap` para el CLI

**Decisión**: Usar `clap` con derive macros para el parseo de argumentos.

**Rationale**:
- Autocomplete para shells
- Mensajes de error claros
- Subcomandos (`descargar`, `info`, `lista`, `cancelar`)
- Es el estándar en Rust

### 4. Binario separado, engine compartido

**Decisión**: El CLI es un binario separado que usa el engine de `lib.rs`.

**Rationale**:
- El engine se compila una vez, lo usan CLI y Tauri
- El CLI no necesita toda la dependencia de Tauri
- Se puede probar el CLI sin la GUI
- La GUI puede llamar al mismo engine via IPC

### 5. RAR: llamar a `unrar` CLI

**Decisión**: Para RAR, llamar al binario `unrar` del sistema (en lugar de crate Rust).

**Rationale**:
- Los crates de RAR en Rust (`unrust`, `unrar-rs`) tienen soporte limitado
- `unrar` CLI maneja RAR5, cifrado, volúmenes, multi-part
- `unrar` ya está instalado en el sistema del usuario
- Alternativa: `unar` (The Unarchiver) que maneja más formatos

### 6. Salida JSON para integración

**Decisión**: El CLI puede output en JSON con `--json`.

**Rationale**:
- Permite que otros programas (scripts, GUIs) consuman el output
- La app Tauri puede parsear JSON para mostrar resultados
- Útil para notificaciones del sistema

## Risks / Trade-offs

- **[Riesgo] Tamaño del binario** → reqwest + tokio = ~10MB compilado. Pero es estático, no necesita nada más.
- **[Riesgo] MediaFire cambia su HTML** → Mitigación: el scraper usa selectores CSS, fáciles de actualizar. Añadir tests de integración que verifiquen el scraper periódicamente.
- **[Riesgo] RAR5 no soportado por crate** → Mitigación: llamar a `unrar` CLI. Si no está instalado, error claro con instrucciones.
- **[Trade-off] Sin multi-hilo nativo en v1** → Empezar con single-thread (reqwest), añadir multi-hilo con segmentación después. La mayoría de los archivos de MediaFire son single-file.
- **[Trade-off] Rust es más lento de compilar que bash** → Pero más rápido en ejecución, menos bugs, mejor mantenimiento.

## Definition of Done

- [ ] `darkdm descargar <mediafire_url>` funciona sin errores
- [ ] `darkdm descargar <mediafire_url> --password x` extrae RAR correctamente
- [ ] `darkdm descargar <mediafire_url> --get-link` solo imprime el link
- [ ] `darkdm descargar <hls_url>` descarga stream HLS
- [ ] `darkdm descargar <direct_url>` descarga archivo directo
- [ ] Resume funciona (interrumpir y reanudar)
- [ ] Barra de progreso visible en terminal
- [ ] `--json` output es parseable
- [ ] Engine funciona desde Tauri (comando `download`)
- [ ] `./init.sh` pasa (build)
- [ ] Tests unitarios para MediaFire scraper
