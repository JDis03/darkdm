// DarkDM Popup - Auto-detection + HTTP download
document.addEventListener('DOMContentLoaded', async () => {
  const streamsList = document.getElementById('streamsList');
  if (!streamsList) return;
  
  // Get current tab
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab) return;
  
  // Load captured media from background
  chrome.runtime.sendMessage({ type: 'GET_CAPTURED_MEDIA', tabId: tab.id }, (res) => {
    if (!res || !res.media || res.media.length === 0) {
      streamsList.innerHTML = '<span class="empty">No se detectaron streams aún</span>';
      return;
    }
    
    // Display streams
    streamsList.innerHTML = '';
    res.media.forEach((media, idx) => {
      const item = document.createElement('div');
      item.className = 'stream-item';
      
      const duration = media.duration ? formatDuration(media.duration) : '';
      const masterTag = media.isMaster ? '<span class="badge-master">MASTER</span>' : '';
      const adTag = media.isAd ? '<span class="badge-ad">📢 Ad</span>' : '';
      
      item.innerHTML = `
        <div class="stream-header">
          <span>📹 M3U8</span>
          ${masterTag}
          ${adTag}
          ${duration ? `<span class="duration">${duration}</span>` : ''}
        </div>
        <div class="stream-url" title="${media.url}">${truncateUrl(media.url, 60)}</div>
        <button class="btn-download" data-idx="${idx}" ${media.isAd ? 'disabled title="Ad stream — wait for real video"' : ''}>
          ${media.isAd ? '🚫 Ad' : '⬇️ Descargar'}
        </button>
      `;
      
      streamsList.appendChild(item);
    });
    
    // Download buttons - send directly to HTTP server
    streamsList.querySelectorAll('.btn-download').forEach(btn => {
      btn.addEventListener('click', async () => {
        const idx = parseInt(btn.getAttribute('data-idx'));
        const media = res.media[idx];
        
        // Use variant URL if it's a master playlist
        const downloadUrl = media.variantUrl || media.url;
        
        const payload = {
          manifest_url: downloadUrl,
          title: tab.title || 'video',
          page_url: tab.url || '',
          headers: media.headers || {},
          cookies: ''
        };
        
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
          }
        } catch (e) {
          alert('DarkDM no está corriendo.\n\nEjecuta:\nsystemctl --user start darkdm-host\n\nO manualmente:\n~/.local/bin/darkdm-host');
        }
      });
    });
  });
  
  function formatDuration(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = Math.floor(seconds % 60);
    if (h > 0) return `${h}:${pad(m)}:${pad(s)}`;
    return `${m}:${pad(s)}`;
  }
  
  function pad(n) {
    return n < 10 ? '0' + n : n;
  }
  
  function truncateUrl(url, maxLen) {
    if (url.length <= maxLen) return url;
    return url.slice(0, maxLen - 3) + '...';
  }
});
