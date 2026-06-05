// DarkDM Popup - Auto-detection + HTTP download
document.addEventListener('DOMContentLoaded', async () => {
  const streamsList = document.getElementById('streamsList');
  if (!streamsList) return;

  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab) return;

  // Refresh button
  const refreshBtn = document.getElementById('refreshBtn');
  if (refreshBtn) refreshBtn.addEventListener('click', loadStreams);

  // Listen for stream updates from background
  chrome.runtime.onMessage.addListener(function(msg) {
    if (msg.type === 'STREAMS_UPDATED' && msg.tabId === tab.id) {
      loadStreams();
    }
  });

  let autoRefreshInterval = null;

  function loadStreams() {
    chrome.runtime.sendMessage({ type: 'GET_CAPTURED_MEDIA', tabId: tab.id }, render);
  }

  function startAutoRefresh() {
    if (!autoRefreshInterval) {
      autoRefreshInterval = setInterval(loadStreams, 2000);
    }
  }

  function stopAutoRefresh() {
    if (autoRefreshInterval) {
      clearInterval(autoRefreshInterval);
      autoRefreshInterval = null;
    }
  }

  function render(res) {
    if (!res || !res.media || res.media.length === 0) {
      streamsList.innerHTML = '<span class="empty">⏳ Esperando streams...<br>Navega a la película.</span>';
      return;
    }

    // Stop auto-refresh if there's at least one real (non-ad) stream
    const hasRealStream = res.media.some(m => !m.isAd);
    if (hasRealStream) stopAutoRefresh();

    streamsList.innerHTML = '';
    res.media.forEach((media, idx) => {
      const item = document.createElement('div');
      item.className = 'stream-item';

      const duration = media.duration ? formatDuration(media.duration) : '';
      const masterTag = media.isMaster ? '<span class="badge-master">MASTER</span>' : '';
      const adTag = media.isAd ? '<span class="badge-ad">📢 espera el video</span>' : '';
      const btnText = '⬇️ Descargar';

      item.innerHTML = `
        <div class="stream-header">
          <span>📹 M3U8</span>
          ${masterTag}${adTag}
          ${duration ? `<span class="duration">${duration}</span>` : ''}
        </div>
        <div class="stream-url" title="${media.url}">${truncateUrl(media.url, 60)}</div>
        <button class="btn-download" data-idx="${idx}">${btnText}</button>
      `;
      streamsList.appendChild(item);
    });

    // Download buttons
    streamsList.querySelectorAll('.btn-download').forEach(btn => {
      btn.addEventListener('click', async () => {
        const idx = parseInt(btn.getAttribute('data-idx'));
        const media = res.media[idx];
        const downloadUrl = media.variantUrl || media.url;

        const payload = {
          manifest_url: downloadUrl,
          title: tab.title || 'video',
          page_url: tab.url || '',
          headers: media.headers || {},
          cookies: ''
        };

        btn.textContent = '⏳ Obteniendo stream...';
        btn.disabled = true;

        // Re-fetch manifest NOW with browser credentials (has session cookies)
        // This gets the fresh content (real video if playing, not the cached ad)
        try {
          const manifestRes = await fetch(downloadUrl, { headers: media.headers || {} });
          if (manifestRes.ok) {
            payload.manifest_body = await manifestRes.text();
          }
        } catch(e) {}

        btn.textContent = '⏳ Iniciando...';

        try {
          const response = await fetch('http://localhost:8765/download', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
          });
          const result = await response.json();
          if (result.success) {
            window.close();
          } else {
            alert('Error: ' + (result.error || 'Unknown'));
            btn.textContent = '⬇️ Descargar';
            btn.disabled = false;
          }
        } catch (e) {
          alert('DarkDM no está corriendo.\n\nEjecuta:\nsystemctl --user start darkdm-host');
          btn.textContent = '⬇️ Descargar';
          btn.disabled = false;
        }
      });
    });
  }

  function formatDuration(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = Math.floor(seconds % 60);
    if (h > 0) return `${h}:${pad(m)}:${pad(s)}`;
    return `${m}:${pad(s)}`;
  }

  function pad(n) { return n < 10 ? '0' + n : n; }

  function truncateUrl(url, maxLen) {
    if (url.length <= maxLen) return url;
    return url.slice(0, maxLen - 3) + '...';
  }

  loadStreams();
  startAutoRefresh();
});
