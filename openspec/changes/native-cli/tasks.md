# DarkDM Native CLI — Tasks

## Prioridad correcta

**Core = Download Engine universal** (como IDM).  
**Site-plugins** (MediaFire) = feature secundaria.  
**Page Analyzer** genérico = intermedio.

## Fase 1: Engine de descarga universal

- [ ] **1.1** Crear estructura `src-tauri/src/downloader/` con `mod.rs`
- [ ] **1.2** Implementar `engine.rs` — descarga directa con `reqwest`:
  - Streaming a archivo
  - Headers custom (User-Agent, Referer, Cookie)
  - Timeout por conexión y total
  - Callbacks de progreso
  - Redirecciones automáticas
- [ ] **1.3** Implementar `segmented.rs` — multi-hilo con Range:
  - HEAD request para obtener tamaño
  - Particionado en N segmentos
  - Descarga paralela con reqwest Range
  - Ensamblado de segmentos
- [ ] **1.4** Implementar `resume.rs` — resume automático:
  - Detectar archivo parcial existente
  - Verificar Content-Length
  - Range con offset
- [ ] **1.5** Implementar `extract.rs` — extracción de archives:
  - ZIP con crate `zip`
  - RAR llamando a `unrar` CLI
  - 7z llamando a `7z` CLI
  - tar.gz / tar.xz con crate `tar`

## Fase 2: CLI con clap

- [ ] **2.1** Crear `src/bin/cli.rs` con `clap`
- [ ] **2.2** Comando `descargar` con todas las flags
- [ ] **2.3** Comando `info`
- [ ] **2.4** Barra de progreso con `indicatif`
- [ ] **2.5** Output `--json`
- [ ] **2.6** Output normal con resumen de sesión

## Fase 3: Page Analyzer (genérico)

- [ ] **3.1** Implementar `page_analyzer.rs`:
  - Detectar si URL devuelve HTML
  - Buscar `<video>`/`<source>`/`<audio>` tags
  - Buscar anchors con extensiones de archivo (.mp4, .mkv, .rar, etc.)
  - Buscar enlaces HLS/DASH en la página
  - Buscar scripts con config (`window.__NUXT__`, `window.__INITIAL_STATE__`)
  - Buscar `og:video` meta tags
  - Buscar iframes con contenido de video

- [ ] **3.2** Plugin trait + MediaFire plugin:
  - `trait SitePlugin { fn matches(), fn extract() }`
  - `MediaFirePlugin`: busca `#downloadButton` href
  - Plugin auto-detect por dominio

## Fase 4: Integración Tauri

- [ ] **4.1** Engine compartido en `lib.rs`
- [ ] **4.2** Comando Tauri `download_url` que usa el engine
- [ ] **4.3** Eventos de progreso → frontend Svelte

## Fase 5: HLS / DASH

- [ ] **5.1** Migrar `downloader.rs` actual (HLS + DASH) al nuevo engine
- [ ] **5.2** Unificar APIs (mismas structs Progress/Result)
