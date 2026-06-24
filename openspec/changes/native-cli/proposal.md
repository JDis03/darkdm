# DarkDM Native CLI — Proposal

## Resumen

Crear un CLI unificado en Rust (`darkdm`) que reemplace los scripts bash actuales y el `darkdm-cli` existente, compartiendo el engine de descarga con la app Tauri.

## ¿Por qué ahora?

El script `darkdm-mediafire` en bash ya está funcionando pero con limitaciones:
- Curl sin resume robusto
- Grep frágil para parsear HTML
- Sin integración con la GUI de Tauri
- Timeouts parcheados

Un CLI Rust nativo resuelve estos problemas de raíz.

## Impacto

**Lo que se reemplaza:**
- `scripts/darkdm-mediafire` → `darkdm descargar`
- `native-host/src/bin/cli.rs` → `darkdm descargar`
- `native-host/src/downloader.rs` → migrado a nuevo engine

**Lo que se mantiene:**
- `native-host/src/server.rs` (HTTP server para extensión Chrome)
- `scripts/darkdm-capture` (captura TLS nativa)
- La app Tauri (se beneficia del nuevo engine)

## Arquitectura

```
src-tauri/src/
├── lib.rs              ← Punto de entrada Tauri + engine compartido
├── bin/
│   └── cli.rs          ← Binario darkdm (CLI)
├── downloader/
│   ├── mod.rs          ← Download engine
│   ├── mediafire.rs    ← Scraper MediaFire
│   ├── hls.rs          ← HLS parser/descargador
│   ├── direct.rs       ← Descarga directa
│   └── extract.rs      ← Extracción de archives
```

## Próximo paso

Después de aprobar el spec, implementar Fase 1.1 (estructura del engine).
