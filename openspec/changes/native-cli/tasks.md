# DarkDM Native CLI — Tasks

## Fase 1: Engine básico (download + MediaFire + extract)

- [ ] **1.1** Crear estructura `src-tauri/src/downloader/` con `mod.rs`
- [ ] **1.2** Implementar descarga directa con `reqwest` (streaming a archivo)
- [ ] **1.3** Implementar MediaFire scraper (`scraper` crate, CSS `#downloadButton`)
- [ ] **1.4** Implementar extracción de ZIP (`zip` crate)
- [ ] **1.5** Implementar extracción de RAR (llamada a `unrar` CLI)
- [ ] **1.6** Implementar extracción de tar.gz/tar.xz/7z

## Fase 2: CLI

- [ ] **2.1** Añadir `clap` y crear binario `darkdm` en `src/bin/cli.rs`
- [ ] **2.2** Comando `descargar` con URL + flags
- [ ] **2.3** Flag `--get-link` (solo extraer, no descargar)
- [ ] **2.4** Flag `--password` para archives
- [ ] **2.5** Flag `--json` output
- [ ] **2.6** Comando `info` (muestra nombre, tamaño, link directo)
- [ ] **2.7** Barra de progreso con `indicatif`

## Fase 3: Resume + multi-hilo

- [ ] **3.1** Resume con Range headers
- [ ] **3.2** Reanudar descarga parcial
- [ ] **3.3** Timeout granular (por conexión y total)

## Fase 4: Integración Tauri

- [ ] **4.1** Engine compartido en `lib.rs`
- [ ] **4.2** Comando Tauri `download` que llama al engine
- [ ] **4.3** Eventos de progreso → frontend Svelte

## Prioridad

Implementar en orden: **Fase 1 → Fase 2 → Fase 3 → Fase 4**
