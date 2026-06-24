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

### 6. Segmented Download (como IDM)

IDM descarga archivos en partes paralelas. Esto es lo que lo hace rápido.

```rust
pub struct SegmentDownloader {
    client: reqwest::Client,
    num_threads: u32,
}

impl SegmentDownloader {
    /// Descarga un archivo en N segmentos paralelos
    pub async fn download(
        &self,
        url: &str,
        output: &Path,
        progress: impl Fn(SegmentProgress) + Send + 'static,
    ) -> Result<DownloadStats, DownloadError> {
        
        // 1. HEAD request para obtener metadata
        let head = self.client.head(url)
            .send()
            .await
            .map_err(|e| DownloadError::network(e))?;
        
        let total_size = head
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or(DownloadError::UnknownSize)?;
        
        let supports_range = head
            .headers()
            .get("accept-ranges")
            .map(|v| v == "bytes")
            .unwrap_or(false);
        
        if !supports_range || total_size < 1024 * 1024 * 10 { // < 10MB
            // Archivos pequeños: single thread
            return self.download_single(url, output, progress).await;
        }
        
        // 2. Dividir en segmentos
        let segment_size = total_size / self.num_threads as u64;
        let segments: Vec<Range> = (0..self.num_threads)
            .map(|i| {
                let start = i as u64 * segment_size;
                let end = if i == self.num_threads - 1 {
                    total_size - 1
                } else {
                    start + segment_size - 1
                };
                Range { start, end }
            })
            .collect();
        
        // 3. Descargar segmentos en paralelo
        let mut handles = Vec::new();
        let completed = Arc::new(AtomicU64::new(0));
        
        for (i, range) in segments.iter().enumerate() {
            let client = self.client.clone();
            let url = url.to_string();
            let output = output.to_path_buf();
            let seg_file = output.with_extension(format!("part{}", i));
            let range = *range;
            let completed = completed.clone();
            
            handles.push(tokio::spawn(async move {
                let mut response = client
                    .get(&url)
                    .header("Range", format!("bytes={}-{}", range.start, range.end))
                    .send()
                    .await?;
                
                let mut file = tokio::fs::File::create(&seg_file).await?;
                while let Some(chunk) = response.chunk().await? {
                    file.write_all(&chunk).await?;
                    completed.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
                Ok::<_, DownloadError>(())
            }));
        }
        
        // 4. Esperar todos los segmentos
        for handle in handles {
            handle.await.map_err(|_| DownloadError::JoinError)??;
        }
        
        // 5. Ensamblar archivo final
        let mut output_file = tokio::fs::File::create(output).await?;
        for i in 0..self.num_threads {
            let seg_file = output.with_extension(format!("part{}", i));
            let mut seg = tokio::fs::File::open(&seg_file).await?;
            tokio::io::copy(&mut seg, &mut output_file).await?;
            tokio::fs::remove_file(&seg_file).await?;
        }
        
        Ok(DownloadStats {
            total_bytes: total_size,
            segments: segments.len() as u32,
            duration: todo!(),
        })
    }
}
```

#### Cuándo usar multi-hilo

| Tamaño archivo | Threads recomendados | Ganancia vs 1 hilo |
|---------------|---------------------|-------------------|
| < 10 MB | 1 | Ninguna (overhead no vale la pena) |
| 10-100 MB | 2 | ~1.5x |
| 100 MB - 1 GB | 4 | ~2.5x |
| 1 GB - 10 GB | 8 | ~4x |
| > 10 GB | 16 | ~5x (limitado por ancho de banda) |

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
