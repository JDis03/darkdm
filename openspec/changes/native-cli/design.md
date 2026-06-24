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

---

## Patrones de diseño estándar

### 1. Pipeline Pattern (Chain of Responsibility)

Cada descarga pasa por una cadena de stages independientes. Cada stage hace una cosa y solo una.

```
     ┌──────────┐    ┌───────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
     │ 1. URL   │    │ 2. Link   │    │ 3. Queue │    │ 4. Down- │    │ 5. Post  │
     │ Resolver │───►│ Extractor │───►│ Manager  │───►│ load     │───►│ Processor│
     └──────────┘    └───────────┘    └──────────┘    │ Engine   │    └──────────┘
                                                      └──────────┘
```

```rust
/// Cada stage implementa este trait
#[async_trait]
pub trait Stage: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: &mut DownloadContext) -> Result<(), StageError>;
}

/// Contexto compartido entre stages
#[derive(Debug)]
pub struct DownloadContext {
    pub id: Uuid,
    pub url: String,
    pub resolved_url: Option<String>,    // después de redirects
    pub detected_links: Vec<DetectedLink>,
    pub selected_link: Option<DetectedLink>,
    pub download_path: Option<PathBuf>,
    pub extracted_files: Vec<PathBuf>,
    pub state: DownloadState,
    pub options: DownloadOptions,
    pub attempt: u8,
    pub error: Option<DownloadError>,
}

/// Pipeline ejecuta stages en orden
pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        Self { stages }
    }

    pub async fn execute(&self, ctx: &mut DownloadContext) -> PipelineResult {
        for stage in &self.stages {
            ctx.state = state_for_stage(stage.name());
            
            match stage.execute(ctx).await {
                Ok(()) => continue,
                Err(e) if e.is_retryable() && ctx.attempt < MAX_RETRIES => {
                    ctx.attempt += 1;
                    let delay = backoff_duration(ctx.attempt); // 2s, 4s, 8s...
                    tokio::time::sleep(delay).await;
                    return self.execute(ctx).await; // retry entire pipeline
                }
                Err(e) => return PipelineResult::Error(ctx.id, e),
            }
        }
        PipelineResult::Success(ctx.clone())
    }
}
```

#### Beneficios del Pipeline

| Beneficio | Explicación |
|-----------|-------------|
| **Aislamiento** | Cada stage es independiente. Si falla el extractor, el downloader no se entera |
| **Testeable** | Puedes testear cada stage por separado con mocks |
| **Extensible** | Quieres añadir un stage de "desencriptar"? Solo agregas un Stage |
| **Observable** | Cada stage emite eventos → progreso detallado en terminal/GUI |
| **Recuperable** | Sabes exactamente en qué stage falló, puedes reintentar desde ahí |

---

### 2. State Machine Pattern

Cada descarga tiene un ciclo de vida con estados y transiciones **explícitas**.

#### Diagrama de estados

```
                         ┌─────────────────────────────────────────┐
                         │                                         │
                         ▼                                         │
                    ┌─────────┐                                    │
                    │ QUEUED  │                                    │
                    └────┬────┘                                    │
                         │                                         │
                    ┌────▼────┐                                    │
              ┌─────│RESOLVING│─────┐                              │
              │     └─────────┘     │                              │
              │                     │                              │
         ┌────▼───┐          ┌──────▼─────┐                        │
         │EXTRACT │          │DIRECT_DOWN │                        │
         └────┬───┘          └──────┬─────┘                        │
              │                     │                              │
              │               ┌─────▼──────┐                       │
              │               │DOWNLOADING │                       │
              │               └─────┬──────┘                       │
              │                     │                              │
              │          ┌──────────┼──────────┐                   │
              │          │          │          │                   │
              │     ┌────▼───┐ ┌────▼────┐ ┌───▼────┐             │
              │     │ PAUSED │ │VERIFYING│ │ ERROR  │──► RETRY ───┘
              │     └────┬───┘ └────┬────┘ └────────┘
              │          │          │
              │     ┌────▼──────────▼────┐
              │     │    EXTRACTING      │
              │     └────┬───────────────┘
              │          │
              │     ┌────▼────┐
              └────►│COMPLETED│
                    └─────────┘
```

```rust
/// Estados de una descarga
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum DownloadState {
    // ─── Espera ───
    Queued,
    Waiting { position: usize },

    // ─── Resolución ───
    Resolving,                              // HEAD request, detectar tipo
    ExtractingLink,                         // Page analyzer / plugins
    
    // ─── Descarga ───
    DirectDownload { url: String },
    Downloading {
        progress: ProgressInfo,
        segments_completed: u32,
        segments_total: u32,
    },
    Paused,
    Verifying { checksum: Option<String> },
    
    // ─── Post-procesamiento ───
    ExtractingArchive { file: String },
    
    // ─── Final ───
    Completed(DownloadResult),
    Error(DownloadError),
    Retrying { attempt: u8, next_retry_at: Instant },
    Cancelled,
}

/// Transiciones válidas definidas en la máquina de estados
impl DownloadState {
    /// Retorna los estados SIGUIENTES válidos desde este estado
    pub fn valid_transitions(&self) -> Vec<DownloadState> {
        match self {
            Queued => vec![Resolving, Cancelled],
            Waiting { .. } => vec![Resolving, Cancelled],
            Resolving => vec![
                ExtractingLink,
                DirectDownload { url: String::new() },
                Error(DownloadError::unknown()),
                Cancelled,
            ],
            ExtractingLink => vec![
                DirectDownload { url: String::new() },
                Error(DownloadError::unknown()),
                Cancelled,
            ],
            DirectDownload { .. } => vec![
                Downloading {
                    progress: ProgressInfo::default(),
                    segments_completed: 0,
                    segments_total: 0,
                },
                Error(DownloadError::unknown()),
                Cancelled,
            ],
            Downloading { .. } => vec![
                Paused,
                Verifying { checksum: None },
                Error(DownloadError::unknown()),
                Cancelled,
            ],
            Paused => vec![
                Downloading {
                    progress: ProgressInfo::default(),
                    segments_completed: 0,
                    segments_total: 0,
                },
                Cancelled,
            ],
            Verifying { .. } => vec![
                ExtractingArchive { file: String::new() },
                Completed(DownloadResult::default()),
                Error(DownloadError::unknown()),
                Cancelled,
            ],
            ExtractingArchive { .. } => vec![
                Completed(DownloadResult::default()),
                Error(DownloadError::unknown()),
                Cancelled,
            ],
            Error(e) => {
                let mut next = vec![Cancelled];
                if e.is_retryable() {
                    next.push(Retrying {
                        attempt: 0,
                        next_retry_at: Instant::now(),
                    });
                }
                next
            }
            Retrying { attempt, .. } => {
                if *attempt < MAX_RETRIES {
                    vec![Resolving, Cancelled]
                } else {
                    vec![Error(DownloadError::max_retries()), Cancelled]
                }
            }
            Completed(_) | Cancelled => vec![], // terminal
        }
    }

    /// Verifica si una transición es válida
    pub fn can_transition_to(&self, next: &DownloadState) -> bool {
        self.valid_transitions()
            .iter()
            .any(|t| std::mem::discriminant(t) == std::mem::discriminant(next))
    }
}
```

#### Por qué State Machine

| Razón | Problema que resuelve |
|-------|----------------------|
| **Sin estados inválidos** | No puedes pasar de QUEUED a COMPLETED sin descargar |
| **Recuperación** | Sabes exactamente dónde falló y qué reintentar |
| **Persistencia** | Puedes guardar/restaurar el estado (resume entre sesiones) |
| **UI reactiva** | La GUI/Terminal escucha cambios de estado y se actualiza |
| **Testing** | Cada transición es testeable |

---

### 3. Plugin Registry Pattern (Strategy)

Los extractores de sitios específicos se registran en un registry que los resuelve automáticamente por URL.

```rust
/// Cualquier extractor de sitios implementa esto
#[async_trait]
pub trait SiteExtractor: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn priority(&self) -> u8;           // más alto = primero en probar
    
    /// ¿Este extractor puede manejar esta URL?
    fn can_handle(&self, url: &Url) -> bool;
    
    /// Extrae links de descarga de la página
    async fn extract(&self, url: &Url, ctx: &mut DownloadContext) -> Result<Vec<DetectedLink>, ExtractError>;
}

/// Registry de extractores
pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn SiteExtractor>>,
}

impl ExtractorRegistry {
    pub fn default() -> Self {
        let mut reg = Self { extractors: vec![] };
        
        // Ordenado por prioridad:
        // 1. Plugins específicos (alta prioridad)
        reg.register(MediaFireExtractor::new());
        reg.register(YouTubeExtractor::new());    // vía yt-dlp
        reg.register(MegaExtractor::new());
        reg.register(GoogleDriveExtractor::new());
        
        // 2. Detectores de formato (media prioridad)
        reg.register(HlsExtractor::new());        // .m3u8
        reg.register(DashExtractor::new());       // .mpd
        
        // 3. Page Analyzer genérico (baja prioridad, fallback)
        reg.register(GenericPageAnalyzer::new());
        
        reg
    }

    /// Encuentra TODOS los extractores que pueden manejar esta URL
    pub fn find_for_url(&self, url: &Url) -> Vec<&dyn SiteExtractor> {
        let mut matched: Vec<&dyn SiteExtractor> = self.extractors
            .iter()
            .filter(|e| e.can_handle(url))
            .map(|e| e.as_ref())
            .collect();
        
        // Ordenar por prioridad descendente
        matched.sort_by(|a, b| b.priority().cmp(&a.priority()));
        matched
    }
}

/// Ejemplo: Extractor de MediaFire
pub struct MediaFireExtractor;

#[async_trait]
impl SiteExtractor for MediaFireExtractor {
    fn id(&self) -> &'static str { "mediafire" }
    fn name(&self) -> &'static str { "MediaFire" }
    fn priority(&self) -> u8 { 100 }

    fn can_handle(&self, url: &Url) -> bool {
        url.host_str()
            .map(|h| h.contains("mediafire.com"))
            .unwrap_or(false)
    }

    async fn extract(&self, url: &Url, ctx: &mut DownloadContext) -> Result<Vec<DetectedLink>, ExtractError> {
        let html = reqwest::get(url.as_str())
            .await
            .map_err(|e| ExtractError::network(e))?
            .text()
            .await
            .map_err(|e| ExtractError::network(e))?;

        let doc = scraper::Html::parse_document(&html);
        
        // Pattern 1: #downloadButton
        let selector = scraper::Selector::parse("#downloadButton").unwrap();
        if let Some(btn) = doc.select(&selector).next() {
            if let Some(href) = btn.value().attr("href") {
                return Ok(vec![DetectedLink::direct(href)]);
            }
        }
        
        // Pattern 2: download*.mediafire.com
        let selector = scraper::Selector::parse("a[href*='download.mediafire.com']").unwrap();
        for link in doc.select(&selector) {
            if let Some(href) = link.value().attr("href") {
                return Ok(vec![DetectedLink::direct(href)]);
            }
        }

        Err(ExtractError::not_found("No download button found on MediaFire page"))
    }
}
```

#### Plugins built-in (orden de prioridad)

| Prioridad | Plugin | Detecta |
|-----------|--------|---------|
| 100 | **MediaFire** | `mediafire.com/file/...` |
| 95 | **YouTube** | `youtube.com`, `youtu.be`, vimeo, tiktok, twitch, twitter... |
| 90 | **Mega** | `mega.nz/file/...` |
| 85 | **Google Drive** | `drive.google.com/file/...` |
| 50 | **HLS** | `.m3u8` en URL o content-type |
| 45 | **DASH** | `.mpd` en URL o content-type |
| 10 | **Page Analyzer** | Cualquier página HTML (fallback genérico) |

---

### 4. Observer / Event System

El pipeline emite eventos. El CLI y la GUI son observers.

```rust
/// Tipos de eventos que emite el pipeline
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event")]
pub enum DownloadEvent {
    /// Cambio de estado (QUEUED → RESOLVING → ...)
    StateChanged {
        download_id: Uuid,
        old_state: DownloadState,
        new_state: DownloadState,
    },
    
    /// Progreso de descarga (se emite ~cada 500ms)
    Progress {
        download_id: Uuid,
        bytes_downloaded: u64,
        total_bytes: Option<u64>,
        speed_bytes_per_sec: f64,
        eta_secs: Option<f64>,
        percent: Option<f64>,
        active_segments: u32,
    },
    
    /// Stage del pipeline completado
    StageCompleted {
        download_id: Uuid,
        stage: String,
        duration_ms: u64,
    },
    
    /// Descarga completada
    Completed {
        download_id: Uuid,
        result: DownloadResult,
    },
    
    /// Error recuperable (se va a reintentar)
    RetryableError {
        download_id: Uuid,
        error: DownloadError,
        attempt: u8,
        next_retry_in_secs: u64,
    },
    
    /// Error fatal
    FatalError {
        download_id: Uuid,
        error: DownloadError,
    },
    
    /// Descarga cancelada por el usuario
    Cancelled {
        download_id: Uuid,
        partial_path: Option<PathBuf>,
        downloaded_bytes: u64,
    },
}

/// Canal de eventos: el pipeline produce, los observers consumen
pub type EventChannel = broadcast::Sender<DownloadEvent>;

/// El Pipeline Manager expone un canal de eventos
pub struct DownloadManager {
    event_tx: broadcast::Sender<DownloadEvent>,
    pipeline: Pipeline,
    active_downloads: HashMap<Uuid, JoinHandle<()>>,
}

impl DownloadManager {
    pub fn event_receiver(&self) -> broadcast::Receiver<DownloadEvent> {
        self.event_tx.subscribe()
    }
}
```

#### Consumidores de eventos

```rust
// ─── Terminal (CLI) ───
pub struct TerminalUi {
    rx: broadcast::Receiver<DownloadEvent>,
    progress_bars: HashMap<Uuid, ProgressBar>,
}

impl TerminalUi {
    pub async fn run(&mut self) {
        while let Ok(event) = self.rx.recv().await {
            match event {
                DownloadEvent::Progress { download_id, percent, speed, eta_secs, .. } => {
                    if let Some(pb) = self.progress_bars.get_mut(&download_id) {
                        if let Some(pct) = percent {
                            pb.set_position(pct as u64);
                        }
                        pb.set_message(format!("{:.1} MB/s | ETA: {:?}s", speed / 1_000_000.0, eta_secs));
                    }
                }
                DownloadEvent::Completed { download_id, result } => {
                    if let Some(pb) = self.progress_bars.remove(&download_id) {
                        pb.finish_with_message("✅ Completado");
                    }
                }
                DownloadEvent::FatalError { download_id, error } => {
                    eprintln!("❌ Error: {}", error);
                }
                _ => {}
            }
        }
    }
}

// ─── Tauri App (GUI) ───
#[tauri::command]
async fn start_download(app: tauri::AppHandle, url: String) -> Result<Uuid, String> {
    let manager = app.state::<DownloadManager>();
    let id = manager.submit(url, DownloadOptions::default()).await;
    
    // Escuchar eventos desde la GUI
    let mut rx = manager.event_receiver();
    let window = app.get_webview_window("main").unwrap();
    
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Enviar evento a la UI de Svelte
            let _ = window.emit("download-event", &event);
        }
    });
    
    Ok(id)
}

// En Svelte:
// import { listen } from '@tauri-apps/api/event';
// listen('download-event', (event) => {
//   // actualizar barra de progreso
// });
```

---

### 5. Retry with Exponential Backoff

Los errores recuperables (timeout, 503, rate limit) se reintentan con espera exponencial.

```rust
const MAX_RETRIES: u8 = 5;
const BASE_DELAY_MS: u64 = 1000;

/// Calcula el delay para el intento N usando exponential backoff + jitter
pub fn backoff_duration(attempt: u8) -> Duration {
    let base = BASE_DELAY_MS * 2u64.pow(attempt as u32); // 1s, 2s, 4s, 8s, 16s
    
    // Añadir jitter: ±25% aleatorio
    use rand::Rng;
    let jitter = rand::thread_rng().gen_range(0..=base / 4);
    
    Duration::from_millis(base + jitter)
}

/// ¿Este error se puede reintentar?
impl DownloadError {
    pub fn is_retryable(&self) -> bool {
        matches!(self,
            Self::Timeout { .. } |
            Self::RateLimited { .. } |
            Self::ServerError { code: 500..=599, .. } |
            Self::ConnectionReset |
            Self::DnsFailure { .. }
        )
    }
}

// Uso en el pipeline:
if let Err(e) = stage.execute(ctx).await {
    if e.is_retryable() && ctx.attempt < MAX_RETRIES {
        ctx.attempt += 1;
        let delay = backoff_duration(ctx.attempt);
        event_tx.send(DownloadEvent::RetryableError {
            download_id: ctx.id,
            error: e,
            attempt: ctx.attempt,
            next_retry_in_secs: delay.as_secs(),
        });
        tokio::time::sleep(delay).await;
        // Reintentar desde RESOLVING (no desde el principio del todo)
        continue;
    }
}
```

#### Errores retryables vs no-retryables

| Error | Retryable? |
|-------|-----------|
| Timeout de conexión | ✅ Sí (el servidor puede recuperarse) |
| 503 Service Unavailable | ✅ Sí |
| 429 Rate Limited | ✅ Sí (con backoff más largo) |
| Connection reset | ✅ Sí |
| DNS timeout | ✅ Sí |
| 404 Not Found | ❌ No (el archivo no existe) |
| 403 Forbidden | ❌ No (necesitas credenciales) |
| SSL certificate error | ❌ No |
| Contraseña incorrecta | ❌ No |
| Sin espacio en disco | ❌ No |
| URL inválida | ❌ No |

---

### 6. Segmented Download — Dynamic Piece-Splitting (algoritmo de XDM)

> **Referencia directa de XDM** (`PieceGrabber.cs`, `HTTPDownloaderBase.cs`).
> XDM **no** divide el archivo en N partes iguales. Empieza con 1 pieza y la divide **dinámicamente** conforme los workers terminan. Esto redistribuye trabajo automáticamente si un servidor es lento en ciertos rangos.

#### ¿Por qué dinámico sobre estático?

```
─── Estático (DarkDM spec original) ──────────────────────────────
Inicio:  [──────────────────────────────────────────────────────]
         T1(0-25%)  T2(25-50%)  T3(50-75%)  T4(75-100%)
Mitad:   T1 ✅      T2 descarga lento...   T3 ✅  T4 ✅
Fin:     T1 ✅      T2 sigue...  ← pierde tiempo, los demás esperan

─── Dinámico (algoritmo XDM / DarkDM futuro) ─────────────────────
Inicio:  [══════════════════════════════════════════════════════]
         T1 (100% del archivo)
25%:     [══════]T1✅  [══════════════════════════════]T2 (split)
50%:     [══════]T1✅  [══════════]T2✅  [════════════]T3 (split)
75%:     T1✅  T2✅  [══════]T3✅  [══════]T4 (split)
Fin:     T1✅  T2✅  T3✅  T4✅   ← todos terminan casi juntos
```

#### Modelo de datos: `Piece`

```rust
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PieceState {
    NotStarted,
    Downloading,
    Finished,
    Failed,
}

#[derive(Debug)]
pub struct Piece {
    pub id: PieceId,
    pub offset: u64,       // byte de inicio en el archivo completo
    pub length: u64,       // bytes asignados a esta pieza (-1 si desconocido)
    pub downloaded: AtomicU64, // bytes descargados hasta ahora (hot path)
    pub state: Mutex<PieceState>,
    pub stream_type: StreamType,  // Primary | Secondary (para dual-source)
}

impl Piece {
    /// Bytes que faltan descargar
    pub fn remaining(&self) -> u64 {
        self.length.saturating_sub(self.downloaded.load(Ordering::Relaxed))
    }

    /// Byte final de esta pieza en el archivo total
    pub fn end_offset(&self) -> u64 {
        self.offset + self.length - 1
    }

    /// ¿Esta pieza puede dividirse? (mínimo 256 KB restantes)
    pub fn is_splittable(&self) -> bool {
        self.remaining() > 256 * 1024
    }
}
```

#### Algoritmo: Dynamic Piece-Splitting

```rust
pub struct PieceManager {
    pieces: HashMap<PieceId, Piece>,
    max_active: usize,   // máximo de workers activos simultáneos (default: 8)
    active: HashSet<PieceId>,
}

impl PieceManager {
    /// Llamado cuando un worker termina su pieza o al probar el servidor.
    /// Busca la pieza más grande y la divide — el nuevo fragmento
    /// se asigna a un nuevo worker.
    pub fn try_create_piece(&mut self) -> Option<PieceId> {
        if self.active.len() >= self.max_active {
            return None;
        }

        // 1. Reintentar piezas fallidas sin worker activo
        if let Some(id) = self.retry_failed() {
            self.active.insert(id);
            return Some(id);
        }

        // 2. Encontrar la pieza con más bytes restantes (la más grande)
        let split_target = self.pieces.iter()
            .filter(|(id, p)| {
                self.active.contains(id)     // debe estar siendo descargada
                && p.is_splittable()         // mínimo 256KB restantes
            })
            .max_by_key(|(_, p)| p.remaining())
            .map(|(id, _)| *id)?;

        // 3. Dividir la pieza por la mitad
        let new_id = self.split_piece(split_target)?;
        self.active.insert(new_id);
        Some(new_id)
    }

    fn split_piece(&mut self, id: PieceId) -> Option<PieceId> {
        let piece = self.pieces.get_mut(&id)?;
        let remaining = piece.remaining();
        if remaining < 256 * 1024 { return None; }

        let new_length = remaining / 2;
        let new_offset = piece.offset + piece.length - new_length;

        // Acortar la pieza original (el final lo toma la nueva pieza)
        piece.length -= new_length;

        // Crear nueva pieza con la segunda mitad
        let new_id = PieceId::new_v4();
        self.pieces.insert(new_id, Piece {
            id: new_id,
            offset: new_offset,
            length: new_length,
            downloaded: AtomicU64::new(0),
            state: Mutex::new(PieceState::NotStarted),
            stream_type: StreamType::Primary,
        });

        Some(new_id)
    }

    fn retry_failed(&mut self) -> Option<PieceId> {
        self.pieces.iter()
            .filter(|(id, p)| {
                !self.active.contains(id)
                && *p.state.lock().unwrap() == PieceState::Failed
            })
            .map(|(id, _)| *id)
            .next()
    }
}
```

#### Worker: `PieceWorker` (porta `PieceGrabber` de XDM)

```rust
pub struct PieceWorker {
    piece_id: PieceId,
    callback: Arc<dyn PieceCallback>,
    client: reqwest::Client,
    max_retries: u8,
}

impl PieceWorker {
    pub async fn run(&self) -> Result<(), PieceError> {
        let mut retries = 0u8;

        loop {
            match self.download_piece().await {
                Ok(()) => {
                    self.callback.piece_finished(self.piece_id);
                    return Ok(());
                }
                Err(e) if e.is_retryable() && retries < self.max_retries => {
                    retries += 1;
                    let delay = Duration::from_secs(2u64.pow(retries as u32)); // 2s, 4s, 8s
                    tokio::time::sleep(delay).await;
                    continue;
                }
                Err(e) => {
                    self.callback.piece_failed(self.piece_id, e.into());
                    return Err(e);
                }
            }
        }
    }

    async fn download_piece(&self) -> Result<(), PieceError> {
        let piece = self.callback.get_piece(self.piece_id);
        let is_first = self.callback.is_first_request(piece.stream_type);

        // Construir Range header
        let range = if is_first {
            "bytes=0-".to_string()   // probe: open-ended
        } else {
            let start = piece.offset + piece.downloaded.load(Ordering::Relaxed);
            let end = piece.offset + piece.length - 1;
            format!("bytes={}-{}", start, end)
        };

        let headers = self.callback.get_headers(self.piece_id);

        let response = self.client
            .get(&piece.url)
            .header("Range", &range)
            .header("Accept-Encoding", "identity")  // ← CRÍTICO: no comprimir
            .headers(headers.unwrap_or_default())
            .send()
            .await
            .map_err(PieceError::Network)?;

        // En el primer request: detectar Text Redirect
        if is_first {
            if let Some(ct) = response.headers().get("content-type") {
                if ct == "text/plain" {
                    // El body es la URL real (algunos CDNs hacen esto)
                    let new_url = response.text().await?.trim().to_string();
                    return Err(PieceError::TextRedirect(new_url));
                }
            }

            // Extraer ProbeResult y notificar al orchestrator
            let probe = ProbeResult::from_response(&response);
            self.callback.piece_connected(self.piece_id, Some(probe));
        } else {
            // En requests posteriores: exigir 206 Partial Content
            if response.status() != 206 {
                return Err(PieceError::RangeNotSupported);
            }
            self.callback.piece_connected(self.piece_id, None);
        }

        // Stream body → archivo temporal
        let temp_path = self.callback.piece_file(self.piece_id);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&temp_path)
            .await?;

        // Seek al offset ya descargado (resume de pieza parcial)
        let already = piece.downloaded.load(Ordering::Relaxed);
        if already > 0 {
            file.seek(SeekFrom::Start(already)).await?;
        }

        const BUF: usize = 32 * 1024; // 32 KB buffer
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(PieceError::Network)?;
            file.write_all(&bytes).await?;
            piece.downloaded.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            self.callback.update_bytes(self.piece_id, bytes.len() as u64);

            // Intentar adoptar pieza adyacente (reutilizar conexión TCP)
            let max_range = piece.offset + piece.length;
            if self.callback.continue_adjacent(self.piece_id, max_range) {
                // El orchestrator nos dio más trabajo dentro de esta conexión
                continue;
            }
        }

        Ok(())
    }
}
```

#### `PieceCallback` trait (Inversion of Control)

El worker nunca conoce al orchestrator — solo habla con este trait. Permite testing con mocks:

```rust
#[async_trait]
pub trait PieceCallback: Send + Sync {
    /// ¿Es el primer request para este tipo de stream?
    fn is_first_request(&self, stream: StreamType) -> bool;

    /// ¿El archivo cambió en el servidor? (Content-Range length diferente)
    fn file_changed_on_server(&self, id: PieceId, new_size: u64) -> bool;

    /// Obtener datos de la pieza
    fn get_piece(&self, id: PieceId) -> Arc<Piece>;

    /// Obtener headers HTTP para esta pieza (User-Agent, Cookie, Referer)
    fn get_headers(&self, id: PieceId) -> Option<HeaderMap>;

    /// El worker se conectó — ProbeResult es Some() solo en el primer request
    fn piece_connected(&self, id: PieceId, probe: Option<ProbeResult>);

    /// Ruta del archivo temporal para escribir esta pieza
    fn piece_file(&self, id: PieceId) -> PathBuf;

    /// Worker descargó N bytes — actualizar contadores globales
    fn update_bytes(&self, id: PieceId, bytes: u64);

    /// ¿Puede el worker adoptar la pieza adyacente sin cerrar la conexión?
    fn continue_adjacent(&self, id: PieceId, current_range_end: u64) -> bool;

    /// Pieza completada
    fn piece_finished(&self, id: PieceId);

    /// Pieza falló
    fn piece_failed(&self, id: PieceId, error: ErrorCode);

    /// Aplicar speed limit si hay uno configurado (bloquea el worker)
    fn throttle_if_needed(&self);
}
```

#### ContinueAdjacentPiece (reutilizar conexión TCP)

Cuando un worker termina su pieza pero el servidor ya envió datos más allá de ese rango, puede adoptar la siguiente pieza sin cerrar la conexión TCP:

```rust
// En el orchestrator (implementa PieceCallback)
fn continue_adjacent(&self, id: PieceId, current_range_end: u64) -> bool {
    let pieces = self.pieces.read().unwrap();
    let active = self.active.read().unwrap();

    // Buscar pieza que empiece exactamente donde termina la actual
    let adjacent = pieces.iter().find(|(adj_id, adj_piece)| {
        adj_piece.offset == current_range_end + 1   // contigua
        && adj_piece.downloaded.load(Ordering::Relaxed) == 0   // no iniciada
        && !active.contains(adj_id)                // sin worker activo
        && adj_piece.stream_type == pieces[&id].stream_type
    });

    if let Some((adj_id, _)) = adjacent {
        // Adoptar: el worker continúa leyendo en la misma conexión HTTP
        // La pieza adyacente ahora es "propiedad" de este worker
        self.active.write().unwrap().insert(*adj_id);

        // Liberar un slot → intentar crear otra pieza con splitting
        drop(pieces);
        drop(active);
        self.try_create_piece();

        true  // el worker continúa sin cerrar la conexión
    } else {
        false // el worker cierra la conexión y termina
    }
}
```

#### ProbeResult — struct del primer HEAD/GET

```rust
/// Información obtenida del primer request a la URL
#[derive(Debug, Clone)]
pub struct ProbeResult {
    /// Tamaño total del archivo (de Content-Length o Content-Range)
    pub resource_size: Option<u64>,

    /// ¿El servidor soporta Range requests? (respuesta 206 = sí, 200 = no)
    pub resumable: bool,

    /// URL final después de todos los redirects
    pub final_uri: String,

    /// Nombre de archivo sugerido (de Content-Disposition: attachment; filename=...)
    pub filename: Option<String>,

    /// MIME type del contenido
    pub content_type: Option<String>,

    /// Última modificación (para detectar si el archivo cambió entre sesiones)
    pub last_modified: Option<SystemTime>,
}

impl ProbeResult {
    pub fn from_response(response: &reqwest::Response) -> Self {
        let headers = response.headers();

        // Tamaño desde Content-Length o Content-Range
        let resource_size = headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .or_else(|| {
                // Content-Range: bytes 0-1023/1234567
                headers.get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.split('/').last())
                    .and_then(|v| v.parse::<u64>().ok())
            });

        let resumable = response.status() == 206;

        // Content-Disposition: attachment; filename="archivo.mp4"
        let filename = headers
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.split(';')
                    .find(|s| s.trim().starts_with("filename="))
                    .map(|s| s.trim().trim_start_matches("filename=").trim_matches('"').to_string())
            });

        let content_type = headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_string());

        let last_modified = headers
            .get("last-modified")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| httpdate::parse_http_date(v).ok());

        ProbeResult {
            resource_size,
            resumable,
            final_uri: response.url().to_string(),
            filename,
            content_type,
            last_modified,
        }
    }
}
```

#### Flujo completo del Progressive Downloader

```
1. HEAD request → ProbeResult
   ├── ¿resumable? ¿tamaño conocido?
   ├── ¿Content-Length < 10MB? → single thread
   └── ¿disk space suficiente? → error temprano

2. Crear Piece(offset=0, length=total_size)
   └── Lanzar PieceWorker(piece_0)

3. piece_connected() recibido con ProbeResult:
   └── try_create_piece() → split piece_0 en piece_0 y piece_1
       └── Lanzar PieceWorker(piece_1)
       └── try_create_piece() → split más si hay slots libres

4. Cada vez que un worker termina:
   └── piece_finished(id)
       └── try_create_piece() → split la pieza más grande restante
           └── Lanzar nuevo PieceWorker(new_piece)

5. Progreso (cada 500ms):
   └── Sumar downloaded de todos los Pieces
   └── Calcular speed = Δbytes / Δtime
   └── ETA = (total - downloaded) / speed
   └── Emitir DownloadEvent::Progress

6. Estado persistido (cada 2s via TransactedIO):
   └── Serializar todos los Pieces a {id}.pieces

7. Todos los workers terminan:
   └── Ordenar piezas por offset
   └── Concatenar archivos temporales → archivo final
   └── Emitir DownloadEvent::Completed
```

#### Cuándo usar multi-hilo

| Tamaño archivo | Workers recomendados | Ganancia vs 1 worker | Observaciones |
|---|---|---|---|
| < 10 MB | 1 | Ninguna | Overhead no vale |
| 10-100 MB | 2 | ~1.5x | Split 1 vez |
| 100 MB - 1 GB | 4 | ~2.5x | Split 3 veces |
| 1 GB - 10 GB | 8 | ~4x | Split 7 veces |
| > 10 GB | 8-16 | ~5x | Limitado por ancho de banda |

---

### 6b. TransactedIO — Estado crash-safe

> **Referencia directa de XDM** (`TransactedIO.cs`).
> Si el proceso muere mientras escribe el estado de las piezas, el archivo anterior queda intacto. Usa `rename(2)` que es **atómico en Linux**.

```
Archivos de estado:
  {id}.pieces.1   ← estado actual (el válido)
  {id}.pieces.2   ← backup del estado anterior
  {id}.pieces.tmp ← escritura en curso (inválido si existe solo)

Rotación al escribir:
  1. Escribir nuevo estado en .tmp (con marcador END al final)
  2. Si .1 existe: rename .1 → .2  (backup atómico)
  3. rename .tmp → .1              (atómico: ahora .1 es el válido)

Al leer (resume después de crash):
  1. Intentar leer .1, validar marcador END
  2. Si .1 inválido: intentar leer .2
  3. Si ambos inválidos: descarga desde cero
```

```rust
const END_MARKER: &[u8] = b"END.";

pub struct TransactedIO;

impl TransactedIO {
    pub fn write(path: &Path, data: &[u8]) -> io::Result<()> {
        let p1 = path.with_extension("1");
        let p2 = path.with_extension("2");
        let tmp = path.with_extension("tmp");

        // Escribir a .tmp con marcador END
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.write_all(END_MARKER)?;
        f.flush()?;
        drop(f);

        // Rotar: .1 → .2 (backup), .tmp → .1 (nuevo válido)
        if p1.exists() {
            fs::rename(&p1, &p2)?;  // atómico en Linux
        }
        fs::rename(&tmp, &p1)?;     // atómico en Linux

        Ok(())
    }

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        let p1 = path.with_extension("1");
        let p2 = path.with_extension("2");

        for candidate in [&p1, &p2] {
            if let Ok(data) = fs::read(candidate) {
                if data.ends_with(END_MARKER) {
                    // Válido: quitar el marcador y retornar
                    return Ok(data[..data.len() - END_MARKER.len()].to_vec());
                }
            }
        }

        Err(io::Error::new(io::ErrorKind::NotFound, "No valid state file found"))
    }
}

/// Serialización del estado de piezas para persistencia
fn serialize_pieces(pieces: &HashMap<PieceId, Piece>) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(pieces.len() as u32).to_le_bytes());
    for (id, piece) in pieces {
        buf.extend_from_slice(id.as_bytes());
        buf.extend_from_slice(&piece.offset.to_le_bytes());
        buf.extend_from_slice(&piece.length.to_le_bytes());
        buf.extend_from_slice(&piece.downloaded.load(Ordering::Relaxed).to_le_bytes());
        buf.push(*piece.state.lock().unwrap() as u8);
        buf.push(piece.stream_type as u8);
    }
    buf
}
```

---

### 6c. Resume automático

El resume ocurre en dos niveles:

**Nivel 1 — Resume entre sesiones** (el proceso fue cerrado):

```rust
pub async fn resume_or_start(
    url: &str,
    output: &Path,
    state_path: &Path,
) -> DownloadTask {
    // 1. ¿Existe estado guardado?
    if let Ok(data) = TransactedIO::read(state_path) {
        if let Ok(pieces) = deserialize_pieces(&data) {
            // Verificar que el archivo no cambió en el servidor
            let probe = probe_url(url).await?;
            let saved_size = pieces.values().map(|p| p.length).sum::<u64>();

            if probe.resource_size == Some(saved_size)
                && probe.resumable {
                // Reanudar desde donde se quedó
                return DownloadTask::resume(pieces, probe);
            }
        }
    }
    // 2. No hay estado válido → empezar desde cero
    DownloadTask::start_fresh(url, output)
}
```

**Nivel 2 — Resume dentro de una pieza** (conexión TCP cortada):

```rust
// En PieceWorker.download_piece():
// Seek al offset ya descargado antes de escribir
let already = piece.downloaded.load(Ordering::Relaxed);
if already > 0 {
    file.seek(SeekFrom::Start(already)).await?;
    // El Range header incluye el offset:
    // Range: bytes={offset + already}-{offset + length - 1}
}
```

---

### 6d. Accept-Encoding: identity (regla crítica)

> **Aprendido de XDM**: Si el servidor comprime la respuesta (`gzip`, `br`, etc.), el `Content-Length` reportado corresponde al tamaño **comprimido**, no al real. Esto rompe los cálculos de Range porque:
> - El archivo real es más grande que `Content-Length`
> - Los byte ranges no corresponden a offsets reales en el archivo

```rust
// SIEMPRE en todos los requests del download engine:
client
    .get(url)
    .header("Accept-Encoding", "identity")  // ← forzar sin compresión
    .header("Range", range)
    .send()
    .await?;
```

Regla: `Accept-Encoding: identity` va en **todos** los requests del engine (probe, pieces, segments). No hay excepción.

---

### 6e. Text Redirect Detection

Algunos CDNs no responden con el archivo directamente — responden con `Content-Type: text/plain` y el body es la URL real:

```rust
// En PieceWorker, primer request únicamente:
if is_first_request {
    let ct = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if ct.starts_with("text/plain") {
        let new_url = response.text().await?.trim().to_string();
        if new_url.starts_with("http") {
            return Err(PieceError::TextRedirect(new_url));
        }
    }
}

// El orchestrator captura TextRedirect y reintenta con la nueva URL:
match worker.run().await {
    Err(PieceError::TextRedirect(new_url)) => {
        // Actualizar URL y reintentar
        self.update_url(&piece_id, new_url);
        self.retry_piece(piece_id);
    }
    _ => {}
}
```

---

### 6f. Session Expiry Detection

Si ya se descargaron bytes y luego el servidor responde con `401/403`, la sesión expiró (cookies caducadas). Es un error diferente a un `403` desde el principio:

```rust
impl PieceWorker {
    fn classify_error(&self, status: StatusCode, already_downloaded: u64) -> PieceError {
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                if already_downloaded > 0 {
                    // Ya teníamos sesión, se expiró
                    PieceError::SessionExpired
                } else {
                    // Nunca tuvimos acceso
                    PieceError::AccessDenied
                }
            }
            StatusCode::NOT_FOUND => PieceError::NotFound,
            s if s.is_server_error() => PieceError::ServerError(s.as_u16()),
            s => PieceError::UnexpectedStatus(s.as_u16()),
        }
    }
}

// En el CLI, SessionExpired muestra un mensaje específico:
// ❌ La sesión expiró (las cookies se invalidaron durante la descarga)
// → Pausa la descarga y vuelve a iniciar sesión en el sitio
// → Luego: darkdm reanudar <url> --cookie "nueva_cookie=valor"
```

---

### 6g. Disk Space Check

Antes de iniciar cualquier descarga grande, verificar espacio disponible tanto en el directorio temporal como en el destino final:

```rust
pub fn check_disk_space(
    temp_dir: &Path,
    output_dir: &Path,
    needed_bytes: u64,
) -> Result<(), DownloadError> {
    // En Linux: statvfs syscall
    let temp_free = available_space(temp_dir)?;
    let output_free = available_space(output_dir)?;

    // Para archivos multi-pieza necesitamos espacio en TEMP
    // para las piezas + espacio en OUTPUT para el archivo final
    // Total = 2x el tamaño (piezas temporales + archivo final)
    let needed_total = needed_bytes * 2;

    if temp_free < needed_bytes {
        return Err(DownloadError::InsufficientDiskSpace {
            location: temp_dir.to_path_buf(),
            needed: needed_bytes,
            available: temp_free,
        });
    }
    if output_free < needed_bytes {
        return Err(DownloadError::InsufficientDiskSpace {
            location: output_dir.to_path_buf(),
            needed: needed_bytes,
            available: output_free,
        });
    }
    Ok(())
}

// Crate disponible: `fs2` o `statvfs`
// fs2::available_space(path) -> u64
```

---

### 6h. Speed Limiter (opcional, como XDM)

Si el usuario configura un límite de velocidad (ej: 1 MB/s para no saturar la red), el limiter bloquea cada worker cuando va demasiado rápido:

```rust
pub struct SpeedLimiter {
    limit_bytes_per_sec: Option<u64>,  // None = sin límite
    last_check: Instant,
    bytes_since_check: u64,
    cancel: Arc<Notify>,  // despertable en Stop()
}

impl SpeedLimiter {
    pub async fn throttle(&mut self, bytes_written: u64) {
        let Some(limit) = self.limit_bytes_per_sec else { return };

        self.bytes_since_check += bytes_written;
        let elapsed = self.last_check.elapsed().as_millis() as u64;

        let max_bytes_per_ms = limit / 1000;
        let expected_ms = self.bytes_since_check / max_bytes_per_ms;

        if elapsed < expected_ms {
            let sleep_ms = expected_ms - elapsed;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(sleep_ms)) => {}
                _ = self.cancel.notified() => {}  // se despierta en Stop()
            }
        }

        if self.last_check.elapsed() > Duration::from_millis(1000) {
            self.last_check = Instant::now();
            self.bytes_since_check = 0;
        }
    }
}
```

---

### 6i. Auto-rename en conflictos de nombre

```rust
pub fn resolve_output_path(dir: &Path, filename: &str) -> PathBuf {
    let path = dir.join(filename);
    if !path.exists() {
        return path;
    }

    let stem = Path::new(filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = Path::new(filename)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for i in 1..=999 {
        let candidate = dir.join(format!("{} ({}){}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
    }

    // Fallback con timestamp
    dir.join(format!("{} ({}){}", stem, chrono::Utc::now().timestamp(), ext))
}
```

---

### 6j. HLS con Byte-Range

Algunos playlists HLS almacenan múltiples segmentos en un solo archivo, accedidos por byte ranges:

```
#EXTM3U
#EXTINF:10.0,
#EXT-X-BYTERANGE:1048576@0
video.ts
#EXTINF:10.0,
#EXT-X-BYTERANGE:1048576@1048576
video.ts
```

El parser HLS debe acumular el offset y emitir un `Chunk` con `Range` header:

```rust
#[derive(Debug)]
pub struct HlsChunk {
    pub url: String,
    pub byte_range: Option<ByteRange>,  // Some(start, len) si es byterange
    pub duration: f64,
    pub sequence: u32,
}

#[derive(Debug)]
pub struct ByteRange {
    pub offset: u64,
    pub length: u64,
}

// El downloader de chunks usa Range si byte_range es Some:
if let Some(range) = &chunk.byte_range {
    request = request.header(
        "Range",
        format!("bytes={}-{}", range.offset, range.offset + range.length - 1)
    );
}
```

---

### 7. Queue Manager

Múltiples descargas se encolan y ejecutan con límite de concurrentes.

```rust
pub struct QueueManager {
    event_tx: broadcast::Sender<DownloadEvent>,
    max_concurrent: usize,       // default: 3
    queue: VecDeque<QueuedDownload>,
    active: HashMap<Uuid, DownloadHandle>,
    completed: Vec<DownloadResult>,
}

impl QueueManager {
    /// Añadir una URL a la cola
    pub async fn enqueue(&mut self, url: String, opts: DownloadOptions) -> Uuid {
        let id = Uuid::new_v4();
        self.queue.push_back(QueuedDownload { id, url, opts });
        
        let _ = self.event_tx.send(DownloadEvent::StateChanged {
            download_id: id,
            old_state: DownloadState::Queued,
            new_state: DownloadState::Queued,
        });
        
        self.try_process_next().await;
        id
    }

    /// Procesar la siguiente descarga si hay espacio
    async fn try_process_next(&mut self) {
        while self.active.len() < self.max_concurrent {
            if let Some(next) = self.queue.pop_front() {
                let id = next.id;
                let tx = self.event_tx.clone();
                
                let handle = tokio::spawn(async move {
                    let mut pipeline = Pipeline::default();
                    let mut ctx = DownloadContext::new(id, next.url, next.opts);
                    let result = pipeline.execute(&mut ctx).await;
                    
                    let _ = tx.send(DownloadEvent::Completed {
                        download_id: id,
                        result: result.into_result(),
                    });
                });
                
                self.active.insert(id, DownloadHandle { task: handle });
            } else {
                break;
            }
        }
    }

    /// Obtener estado de la cola
    pub fn status(&self) -> QueueStatus {
        QueueStatus {
            queued: self.queue.len(),
            active: self.active.len(),
            completed: self.completed.len(),
            max_concurrent: self.max_concurrent,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct QueueStatus {
    pub queued: usize,
    pub active: usize,
    pub completed: usize,
    pub max_concurrent: usize,
}
```

---

### 8. File Layout final (cómo queda todo)

```
src-tauri/src/
├── lib.rs                        ← Tauri app + re-exporta downloader
├── main.rs                       ← Entry point Tauri
│
├── bin/
│   └── cli.rs                    ← darkdm CLI (clap)
│
├── downloader/                   ← Engine compartido
│   ├── mod.rs                    ← Re-exporta todo
│   ├── pipeline.rs               ← Pipeline orchestrator
│   ├── state.rs                  ← State machine
│   ├── context.rs                ← DownloadContext
│   ├── events.rs                 ← Event system
│   │
│   ├── stages/                   ← Pipeline stages
│   │   ├── mod.rs
│   │   ├── url_resolver.rs       ← Stage 1: HEAD + detectar tipo
│   │   ├── link_extractor.rs     ← Stage 2: Plugin registry
│   │   ├── download_engine.rs    │
│   │   │   ├── direct.rs         ← Descarga single-thread
│   │   │   ├── segmented.rs      ← Multi-hilo con Range
│   │   │   └── resume.rs         ← Resume handler
│   │   ├── hls.rs                ← HLS download
│   │   ├── dash.rs               ← DASH download
│   │   └── post_processor.rs     │
│   │       ├── extract.rs        ← Archive extraction
│   │       └── organizer.rs      ← Video/organize
│   │
│   ├── plugins/                  ← Site-specific extractors
│   │   ├── mod.rs                ← Registry + trait
│   │   ├── mediafire.rs
│   │   ├── youtube.rs            ← yt-dlp wrapper
│   │   ├── mega.rs
│   │   ├── googledrive.rs
│   │   ├── hls_detector.rs
│   │   ├── dash_detector.rs
│   │   └── generic_page.rs       ← Fallback genérico
│   │
│   ├── queue.rs                  ← Queue manager
│   ├── retry.rs                  ← Exponential backoff
│   └── progress.rs               ← Progress types
│
└── ...
```

---

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
| `--interactive` | Elegir manualmente entre múltiples links | false |
| `--show-all` | Mostrar todos los links encontrados (sin filtrar) | false |
| `--min-size <n>` | Tamaño mínimo del archivo (ej: 10MB, 1GB) | 1MB |
| `--max-size <n>` | Tamaño máximo del archivo | — |
| `--ext <exts>` | Extensiones permitidas (ej: mp4,mkv,avi) | — |
| `--quality <res>` | Calidad preferida (ej: 1080p, 720p, best) | best |
| `--audio-only` | Solo audio | false |
| `--video-only` | Solo video del archive | false |

---

## Pros / Cons del enfoque

### ✅ Pros (vs bash actual)

| Aspecto | Bash actual | Rust nativo |
|---------|------------|-------------|
| **Velocidad** | Lento (curl 1 hilo) | Multi-hilo con Range (3-5x más rápido) |
| **Resume** | `-C -` frágil | Range headers + verificación |
| **HTML parsing** | grep/regex frágil | scraper crate (CSS selectors) |
| **Dependencias** | curl, unrar, 7z, ffmpeg | Solo runtime de Rust (estático) |
| **Progreso** | Barra de curl, no capturable | Callbacks → terminal + Tauri |
| **Errores** | Códigos de salida genéricos | Tipos de error específicos |
| **Multi-hilo** | No | Sí, segmentación con Range |
| **Extensible** | Scripts separados | Plugin trait + registro |
| **Testing** | Casi imposible | Tests unitarios + integración |
| **Mantenimiento** | Parches sobre parches | Código estructurado |

### ❌ Contras (Rust nativo vs bash)

| Aspecto | Impacto |
|---------|---------|
| **Compilación** | `cargo build` tarda ~2-3 min (vs bash instantáneo) |
| **Tamaño binario** | ~10-15 MB (vs bash ~5 KB) |
| **Curva de aprendizaje** | Rust es complejo (vs bash que cualquiera modifica) |
| **Prototipado** | Bash es más rápido para probar ideas |
| **Dependencias crates** | Necesitas internet para `cargo build` |
| **RAR nativo** | No hay buen crate Rust para RAR → sigue dependiendo de `unrar` CLI |
| **yt-dlp** | Sigue siendo dependencia externa para YouTube |

### ⚖️ Balance

| Situación | Recomendación |
|-----------|---------------|
| Quieres hacer cambios rápidos | Bash |
| Quieres velocidad y confiabilidad | Rust |
| Archivos pequeños (< 100MB) | Bash o Rust, da igual |
| Archivos grandes (1GB+) | Rust (multi-hilo cambia todo) |
| Producción/estable | Rust |
| Scripting personal | Da igual, el CLI funciona igual |

---

## Casos de uso completos

### 1. URL directa de un archivo

```bash
darkdm descargar "https://cdn.ejemplo.com/video.mp4"
```
| Paso | Qué pasa |
|------|----------|
| 1 | HEAD request → `Content-Type: video/mp4`, `Content-Length: 1.2GB` |
| 2 | Verifica `Accept-Ranges: bytes` → sí, multi-hilo posible |
| 3 | Divide en 4 partes (300MB c/u) |
| 4 | Descarga en paralelo con Range headers |
| 5 | Ensambla en `~/Descargas/DarkDM/video.mp4` |
| ✅ | **3-5x más rápido que curl single-thread** |

### 2. URL con resume

```bash
# Se cortó a los 500MB...
darkdm descargar "https://cdn.ejemplo.com/video.mp4"
# Vuelve a ejecutar
darkdm descargar "https://cdn.ejemplo.com/video.mp4"
```
| Paso | Qué pasa |
|------|----------|
| 1 | Detecta `video.mp4` parcial (500MB) en destino |
| 2 | HEAD request → Content-Length: 1.2GB |
| 3 | Calcula faltante: 700MB |
| 4 | Range: `bytes=500000000-` |
| 5 | Descarga solo lo que falta + verifica integridad |
| ✅ | **No descarga lo que ya tiene** |

### 3. MediaFire con contraseña

```bash
darkdm descargar "https://www.mediafire.com/file/XXXX/archivo.rar/file" --password "mipass"
```
| Paso | Qué pasa |
|------|----------|
| 1 | Detecta `mediafire.com` → plugin MediaFire |
| 2 | Fetch página HTML con `reqwest` |
| 3 | CSS selector `#downloadButton` → href |
| 4 | Extrae link directo: `https://download1350.mediafire.com/.../archivo.rar` |
| 5 | Descarga link directo (multi-hilo si servidor lo permite) |
| 6 | Detecta `.rar` → `unrar x -p"mipass"` |
| 7 | Busca videos → los mueve al destino |
| ✅ | **Todo automático** |

### 4. YouTube (vía yt-dlp)

```bash
darkdm descargar "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
```
| Paso | Qué pasa |
|------|----------|
| 1 | Detecta `youtube.com` → plugin YouTube |
| 2 | `yt-dlp --dump-json` → obtiene info del video |
| 3 | Muestra: título, duración, formatos disponibles |
| 4 | `yt-dlp -f bestvideo+bestaudio --merge-output-format mp4` |
| 5 | yt-dlp descarga y mergea video+audio |
| ✅ | **1000+ sitios soportados** |

### 5. Página genérica con video

```bash
darkdm descargar "https://ejemplo.com/pelicula"
```
| Paso | Qué pasa |
|------|----------|
| 1 | HEAD → `Content-Type: text/html` |
| 2 | Fetch HTML completo |
| 3 | page_analyzer busca: `<video>`, `<source>`, anchors `.mp4`, scripts |
| 4 | Filtra: ads, archivos < 1MB, duplicados |
| 5 | Encuentra 3 videos: 1080p, 720p, 480p |
| 6 | Selecciona el mejor (1080p) automáticamente |
| 7 | Descarga link directo |
| ✅ | **Sin configuración específica por sitio** |

### 6. Múltiples URLs (cola)

```bash
darkdm descargar \
  "https://mediafire.com/file/XXXX/aaa.rar/file" \
  "https://mediafire.com/file/XXXX/bbb.rar/file" \
  "https://youtube.com/watch?v=xxxx"
```
| Paso | Qué pasa |
|------|----------|
| 1 | Encola las 3 URLs |
| 2 | Descarga una por una |
| 3 | Muestra progreso total: "2/3 completadas" |
| ✅ | **Descarga por lotes** |

### 7. Solo info (sin descargar)

```bash
darkdm info "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
```
```
📹 Rick Astley - Never Gonna Give You Up
   Canal: Rick Astley
   Duración: 3:32
   Publicado: 2009-10-25
   Vistas: 1.5B

Formatos:
  18   360p  mp4    video+audio  22 MB
  22   720p  mp4    video+audio  45 MB
  137  1080p mp4    video only   38 MB
  140         m4a    audio only   3 MB
```

```bash
darkdm info "https://mediafire.com/file/XXXX/archivo.rar/file"
```
```
📦 archivo.rar (2.7 GB)
   Link directo: https://download1350.mediafire.com/...
   Contraseña: no detectada
   Contenido probable: video (por tamaño)
```

### 8. Búsqueda en YouTube

```bash
darkdm descargar "ytsearch:never gonna give you up"
```
```
🔍 Buscando: "never gonna give you up"
   Resultado #1: Rick Astley - Never Gonna Give You Up (3:32)
   Descargando...
```

### 9. Pipe a otro programa (--json)

```bash
darkdm descargar "https://..." --json | jq '.files[].path'
# "/home/dark/Descargas/DarkDM/video.mp4"

darkdm descargar "https://..." --json | notify-send "Descarga completa"
```

### 10. Interactivo (elegir entre múltiples opciones)

```bash
darkdm descargar "https://ejemplo.com" --interactive
```
```
🔍 3 videos encontrados:
   1) 1080p  video.mp4  2.1 GB
   2) 720p   video.mp4  1.2 GB
   3) 480p   video.mp4  700 MB
   
Elige (1-3): 2
⬇️  Descargando 720p...
```

---

## Manejo de errores de links

### Categorías de error

| Categoría | Ejemplo | Mensaje |
|-----------|---------|---------|
| **URL inválida** | `darkdm descargar "no-es-url"` | `❌ URL inválida: "no-es-url". Formato esperado: https://...` |
| **Sin conexión** | `darkdm descargar "https://ejemplo.com"` (sin internet) | `❌ No se pudo conectar a ejemplo.com. Verifica tu conexión a internet.` |
| **DNS fail** | `darkdm descargar "https://sitioquenoexiste123.com"` | `❌ No se pudo resolver el dominio: sitioquenoexiste123.com` |
| **Timeout** | `darkdm descargar "https://server-lento.com/video.mp4"` | `❌ Timeout después de 30s. --timeout para aumentarlo.` |
| **404** | `darkdm descargar "https://ejemplo.com/noexiste.mp4"` | `❌ 404 Not Found. El archivo ya no existe en el servidor.` |
| **403** | `darkdm descargar "https://ejemplo.com/video.mp4"` | `❌ 403 Forbidden. Necesitas --referer o --cookie.` |
| **SSL Error** | `darkdm descargar "https://sitio-con-cert-viejo.com"` | `❌ Error SSL: certificado expirado.` |
| **Redirect loop** | `darkdm descargar "https://sitio.com/a"` | `❌ Demasiados redirects. El servidor está en un bucle.` |
| **Sin espacio** | Disco lleno | `❌ No hay espacio en disco. Faltan 500 MB libres.` |
| **Permiso denegado** | `darkdm descargar "..." --dir /root/` | `❌ No tienes permisos de escritura en /root/. Usa --dir ~/Descargas/` |

### Errores específicos de scraping

| Error | Causa | Solución |
|-------|-------|----------|
| `No se encontró link en la página` | La página no tiene videos ni enlaces de descarga | El sitio no es compatible. Usa la extensión de Chrome. |
| `MediaFire: no se encontró downloadButton` | MediaFire cambió su HTML | Reportar issue. Usar `--get-link` con URL directa. |
| `YouTube requiere yt-dlp` | yt-dlp no está instalado | `pip install yt-dlp` o `sudo pacman -S yt-dlp` |
| `Página requiere login` | El contenido está detrás de autenticación | Usa la extensión de Chrome (tiene tus cookies de sesión) |
| `Video protegido por DRM` | Netflix, Disney+, etc. | No soportado. DRM no se puede descifrar. |

### Errores de extracción

| Error | Causa | Solución |
|-------|-------|----------|
| `RAR protegido o corrupto` | Contraseña incorrecta o archivo dañado | Verifica la contraseña con `--password`. Si está dañado, reintenta descarga. |
| `ZIP corrupto` | Descarga incompleta | Vuelve a ejecutar (resume automático). |
| `unrar no está instalado` | Falta dependencia | `sudo pacman -S unrar` o `sudo apt install unrar` |
| `Formato de archive no soportado` | .ace, .arj, etc. | Instala `unar` (The Unarchiver) que soporta más formatos. |

### Códigos de salida

| Código | Significado |
|--------|-------------|
| `0` | ✅ Descarga completada exitosamente |
| `1` | ❌ Error general (URL inválida, sin conexión, etc.) |
| `2` | ❌ Error de red (timeout, 404, 403, etc.) |
| `3` | ❌ Error de scraping (no se encontró link) |
| `4` | ❌ Error de extracción (RAR corrupto, contraseña incorrecta) |
| `5` | ❌ Error de disco (sin espacio, permisos) |
| `6` | ❌ Dependencia faltante (yt-dlp, unrar no instalado) |
| `130` | 🛑 Cancelado por el usuario (Ctrl+C) |

### Ejemplos de output de error

```bash
# Error de conexión
$ darkdm descargar "https://sitioquenoexiste123.com/video.mp4"
❌ No se pudo resolver el dominio: sitioquenoexiste123.com
   → Verifica que la URL sea correcta
   → Verifica tu conexión a internet
   → Si el sitio existe, prueba con --timeout 60
   Salida: 1

# Error 404
$ darkdm descargar "https://mediafire.com/file/XXXX/viejo.rar/file"
❌ 404 Not Found (MediaFire)
   → El archivo fue eliminado o la URL es incorrecta
   → Verifica que el archivo sigue disponible en mediafire.com
   Salida: 2

# Error de scraping
$ darkdm descargar "https://ejemplo.com"
❌ No se encontró ningún link de descarga en la página
   → El analizador revisó: <video>, <source>, anchors, scripts
   → Sugerencias:
     1. Usa la extensión de Chrome (captura cualquier stream)
     2. Si es MediaFire/YouTube, el plugin debería detectarlo
     3. Pasa la URL directa si la tienes
   Salida: 3

# Error de contraseña
$ darkdm descargar "https://mediafire.com/file/XXXX/archivo.rar/file" --password "wrong"
❌ Contraseña incorrecta para archivo.rar
   → --password "contraseña_correcta"
   → Si no sabes la contraseña, el .rar se guardó sin extraer
   Salida: 4

# Dependencia faltante
$ darkdm descargar "https://youtube.com/watch?v=xxxx"
⚠️  YouTube requiere yt-dlp
   → Instálalo: pip install yt-dlp
   → O: sudo pacman -S yt-dlp
   → Luego reintenta
   Salida: 6

# Cancelado por usuario
$ darkdm descargar "https://cdn.ejemplo.com/video-grande.mp4"
⬇️  Descargando... (Ctrl+C presionado)
🛑 Descarga cancelada
   → El archivo parcial queda en ~/Descargas/DarkDM/video-grande.mp4.partial
   → Reintenta y se reanudará automáticamente
   Salida: 130
```

### Manejo de errores en JSON

```json
{
  "status": "error",
  "code": 2,
  "error": "404 Not Found",
  "url": "https://ejemplo.com/noexiste.mp4",
  "suggestion": "El archivo fue eliminado o la URL es incorrecta",
  "partial_path": null,
  "timestamp": "2026-06-24T10:30:00Z"
}
```

```json
{
  "status": "cancelled",
  "code": 130,
  "error": "Cancelado por usuario",
  "partial_path": "/home/dark/Descargas/DarkDM/video.mp4.partial",
  "partial_bytes": 524288000,
  "total_bytes": 1073741824,
  "progress_percent": 48.8,
  "timestamp": "2026-06-24T10:30:00Z"
}
```

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

## Multi-hilo y Resume

> Cubierto en detalle en las secciones **6a** (Dynamic Piece-Splitting), **6b** (TransactedIO), y **6c** (Resume automático) dentro de los Patrones de diseño estándar.

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

### 3. Dynamic piece-splitting (no estático) — algoritmo de XDM

**Decisión**: Usar el algoritmo de splitting dinámico de XDM en lugar de dividir el archivo en N partes iguales al inicio.

**Rationale**: El splitting dinámico redistribuye trabajo automáticamente si un servidor es lento en un rango específico. Empieza con 1 pieza y divide la más grande conforme los workers terminan. Referencia: `HTTPDownloaderBase.cs` de XDM.

### 4. TransactedIO para estado crash-safe

**Decisión**: Persistir el estado de las piezas con rotación de 3 archivos y marcador END validado.

**Rationale**: Si el proceso muere durante escritura de estado, el archivo anterior queda intacto gracias a `rename(2)` atómico en Linux. Referencia: `TransactedIO.cs` de XDM.

### 5. Accept-Encoding: identity en todos los requests

**Decisión**: Siempre enviar `Accept-Encoding: identity` en los requests del download engine.

**Rationale**: Si el servidor comprime la respuesta, el `Content-Length` corresponde al tamaño comprimido, rompiendo los cálculos de Range. No hay excepción a esta regla. Referencia: `SingleSourceHTTPDownloader.cs` de XDM.

### 6. PieceCallback trait (Inversion of Control)

**Decisión**: El worker (`PieceWorker`) solo habla con el trait `PieceCallback`, nunca con el orchestrator directamente.

**Rationale**: Desacopla el worker del downloader. Permite tests unitarios con mocks. Referencia: `IPieceCallback.cs` de XDM.

### 7. ContinueAdjacentPiece (reutilizar conexión TCP)

**Decisión**: Cuando un worker termina su pieza y el servidor ya mandó datos del rango siguiente, adoptar la pieza adyacente sin cerrar la conexión TCP.

**Rationale**: Reduce latencia de reconexión en servidores con alta latencia. Referencia: `PieceGrabber.cs` de XDM.

### 8. Page Analyzer genérico + plugins site-specific

**Decisión**: El page analyzer busca enlaces genéricamente (video tags, anchors, scripts). Los site-plugins (MediaFire, YouTube) son un extra.

**Rationale**: No podemos tener scrapers para cada sitio. Lo genérico cubre el 90%. Los plugins cubren sitios populares.

### 9. Session expiry como error diferenciado

**Decisión**: Si ya se descargaron bytes y el servidor responde `401/403`, es `SessionExpired` — diferente a `AccessDenied`.

**Rationale**: El usuario necesita mensajes de error accionables. `SessionExpired` le dice "renueva tu cookie", `AccessDenied` le dice "no tienes acceso". Referencia: `PieceGrabber.cs` de XDM.

### 10. Disk space check antes de descargar

**Decisión**: Verificar espacio disponible en temp y output antes de iniciar la descarga.

**Rationale**: Fallar rápido es mejor que fallar a los 2 GB. Referencia: `SingleSourceHTTPDownloader.cs` de XDM.

### 11. Output JSON para integraciones

Con `--json`, cualquier comando debe output JSON parseable:

```bash
darkdm descargar "https://..." --json
# {"status":"success","files":[{"path":"...","size":123}],"duration":154}
```

---

## Referencia XDM → DarkDM (tabla de mapeo)

| XDM (C#) | DarkDM (Rust) | Notas |
|-----------|---------------|-------|
| `Piece.cs` | `downloader/piece.rs` | Modelo de datos, AtomicU64 para downloaded |
| `PieceGrabber.cs` | `downloader/stages/piece_worker.rs` | Worker por pieza |
| `HTTPDownloaderBase.cs` | `downloader/stages/download_engine.rs` | Orchestrator + splitting dinámico |
| `IPieceCallback` | `PieceCallback` trait | Inversion of control |
| `TransactedIO.cs` | `downloader/transacted_io.rs` | 3-file rotation + END marker |
| `SpeedLimiter.cs` | `downloader/speed_limiter.rs` | Wakeable sleep con Notify |
| `ProbeResult` | `downloader/probe.rs` | Struct del primer HEAD/GET |
| `SingleSourceHTTPDownloader` | `downloader/stages/direct.rs` | Descarga directa single-source |
| `DualSourceHTTPDownloader` | N/A | lo hace yt-dlp |
| `MultiSourceHLSDownloader` | `downloader/stages/hls.rs` | Con byte-range support |
| `MultiSourceDASHDownloader` | `downloader/stages/dash.rs` | |
| `HlsParser.cs` | `downloader/parsers/hls.rs` | Con #EXT-X-BYTERANGE |
| `MpdParser.cs` | `downloader/parsers/dash.rs` | SegmentTemplate |
| `DownloadQueue.cs` + Scheduler | `downloader/queue.rs` | Cola + horario semanal |
| `IpcHttpMessageProcessor` | `native-host/src/server.rs` | Ya existe |
| `FileNameFetchMode` | `resolve_output_path()` | Auto-rename colisiones |
| `WinHttpClient` / `WinInetClient` | N/A | No necesario en Linux |

---

## Risks / Trade-offs

- **[Riesgo] Servidores sin Range** → Fallback automático a single-thread tras ProbeResult.
- **[Riesgo] Rate limiting con multi-hilo** → `--threads` configurable, default conservador (4), splitting dinámico reduce el problema.
- **[Riesgo] Content-Length dinámico** → Si `Content-Range` difiere del tamaño guardado → `FileChangedOnServer` → reinicio.
- **[Riesgo] Estado corrupto tras crash** → TransactedIO con rotación de 3 archivos y validación END marker.
- **[Trade-off] Rust compile time** → ~3min la primera vez. Se compila una vez, corre sin dependencias.
- **[Trade-off] RAR sigue siendo externo** → unrar CLI, no hay crate Rust maduro para RAR5.

## Definition of Done

- [ ] `darkdm descargar <direct_url>` descarga con barra de progreso
- [ ] Dynamic piece-splitting funciona (splitting dinámico, no estático)
- [ ] `ContinueAdjacentPiece` reutiliza conexiones TCP
- [ ] TransactedIO persiste estado crash-safe
- [ ] `Accept-Encoding: identity` en todos los requests del engine
- [ ] Resume entre sesiones funciona (TransactedIO + deserialize pieces)
- [ ] Resume dentro de pieza funciona (seek + Range offset)
- [ ] ProbeResult detecta: tamaño, resumable, filename, content-type
- [ ] Text redirect detection en primer request
- [ ] Session expiry = error diferenciado de 403 normal
- [ ] Disk space check antes de empezar
- [ ] Auto-rename en conflicto de nombre de archivo
- [ ] `darkdm descargar <mediafire_url> --password x` extrae RAR
- [ ] `darkdm descargar <hls_url>` con byte-range HLS support
- [ ] `darkdm descargar <url> --json` output JSON
- [ ] `darkdm info <url>` muestra info sin descargar
- [ ] `./init.sh` pasa
- [ ] Engine funciona desde Tauri
