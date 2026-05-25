<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  let files = [];
  let path = '';
  let loading = true;

  onMount(async () => {
    path = await invoke('downloads_path');
    await loadFiles();
    // Auto-refresh every 3 seconds
    setInterval(loadFiles, 3000);
  });

  async function loadFiles() {
    try {
      files = await invoke('list_downloads');
      loading = false;
    } catch(e) {
      console.error(e);
      loading = false;
    }
  }

  function formatDate(ts) {
    return ts;
  }

  $: videos = files.filter(f => f.is_video);
  $: others = files.filter(f => !f.is_video);
  $: totalSize = files.reduce((a, f) => a + f.size, 0);

  function formatTotal(bytes) {
    const units = ['B', 'KB', 'MB', 'GB'];
    let s = bytes;
    let u = 0;
    while (s > 1024 && u < units.length - 1) { s /= 1024; u++; }
    return s.toFixed(1) + ' ' + units[u];
  }
</script>

<main>
  <header>
    <div class="logo">
      <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="#FF6B35" stroke-width="2.5">
        <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
        <polyline points="7 10 12 15 17 10"/>
        <line x1="12" y1="15" x2="12" y2="3"/>
      </svg>
      <h1>DarkDM</h1>
    </div>
    <span class="path">{path}</span>
    <button class="refresh" on:click={loadFiles}>🔄</button>
  </header>

  <div class="stats">
    <span class="stat">{files.length} archivos</span>
    <span class="stat">{formatTotal(totalSize)}</span>
    <span class="stat">{videos.length} videos</span>
  </div>

  {#if loading}
    <div class="empty">Cargando...</div>
  {:else if files.length === 0}
    <div class="empty">
      <p>No hay descargas aún</p>
      <p class="hint">Haz clic en ⬇️ DarkDM en cualquier video</p>
    </div>
  {:else}
    <div class="files">
      {#each files as file}
        <div class="file-row">
          <span class="icon">{file.is_video ? '🎬' : '📄'}</span>
          <div class="info">
            <span class="name" title={file.name}>{file.name}</span>
            <span class="meta">{file.size_display} · {file.modified}</span>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</main>

<style>
  :global(*) { margin: 0; padding: 0; box-sizing: border-box; }
  :global(body) {
    font-family: 'Segoe UI', system-ui, sans-serif;
    background: #0f0f1a;
    color: #e0e0e0;
    user-select: none;
  }
  main { max-width: 700px; margin: 0 auto; padding: 20px; }
  header {
    display: flex; align-items: center; gap: 12px;
    padding: 16px 20px; background: #1a1a2e; border-radius: 12px;
    margin-bottom: 12px;
  }
  .logo { display: flex; align-items: center; gap: 8px; }
  .logo h1 { font-size: 20px; font-weight: 700; color: #fff; }
  .path { flex: 1; font-size: 11px; color: #666; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .refresh {
    background: rgba(255,255,255,0.08); border: none; color: #aaa;
    width: 32px; height: 32px; border-radius: 8px; cursor: pointer;
    font-size: 16px;
  }
  .refresh:hover { background: rgba(255,255,255,0.15); }
  .stats {
    display: flex; gap: 8px; margin-bottom: 12px;
  }
  .stat {
    padding: 6px 14px; background: rgba(255,255,255,0.05);
    border-radius: 8px; font-size: 12px; color: #888;
  }
  .empty {
    text-align: center; padding: 60px 20px; color: #555;
  }
  .empty p { font-size: 16px; margin-bottom: 8px; }
  .empty .hint { font-size: 13px; color: #444; }
  .files { display: flex; flex-direction: column; gap: 4px; }
  .file-row {
    display: flex; align-items: center; gap: 10px;
    padding: 10px 14px; background: rgba(255,255,255,0.03);
    border-radius: 8px; transition: background 0.2s;
  }
  .file-row:hover { background: rgba(255,255,255,0.06); }
  .icon { font-size: 20px; width: 30px; text-align: center; }
  .info { flex: 1; min-width: 0; }
  .name {
    display: block; font-size: 13px; color: #ddd;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .meta { font-size: 11px; color: #666; }
</style>
