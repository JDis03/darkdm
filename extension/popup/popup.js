// DarkDM Popup - Auto-detection
document.addEventListener('DOMContentLoaded', async () => {
  const streamsList = document.getElementById('streamsList');
  if (!streamsList) return;
  
  // Get current tab
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab) return;
  
  // Load captured media
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
      
      item.innerHTML = `
        <div class="stream-header">
          <span>📹 M3U8</span>
          ${masterTag}
          ${duration ? `<span class="duration">${duration}</span>` : ''}
        </div>
        <div class="stream-url" title="${media.url}">${truncateUrl(media.url, 60)}</div>
        <button class="btn-download" data-idx="${idx}">⬇️ Descargar</button>
      `;
      
      streamsList.appendChild(item);
    });
    
    // Download buttons
    streamsList.querySelectorAll('.btn-download').forEach(btn => {
      btn.addEventListener('click', () => {
        const idx = parseInt(btn.getAttribute('data-idx'));
        const media = res.media[idx];
        
        // Send native message DIRECTLY from popup (MV3 service worker unreliable)
        const downloadUrl = media.variantUrl || media.url;
        const msg = {
          type: 'DOWNLOAD_MANIFEST',
          manifest_url: downloadUrl,
          cookies: '',
          title: tab.title || 'video',
          manifest_body: media.manifestBody || '',
          page_url: tab.url || '',
          headers: JSON.stringify(media.headers || {})
        };
        
        chrome.runtime.sendNativeMessage('com.darkdm.manager', msg, (response) => {
          if (chrome.runtime.lastError) {
            alert('Error DM: ' + chrome.runtime.lastError.message);
            return;
          }
          console.log('[DM] Native response:', response);
          if (response && response.success) {
            window.close();
          } else {
            alert('Error: ' + (response?.error || 'Unknown'));
          }
        });
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
