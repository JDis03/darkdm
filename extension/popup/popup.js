// DarkDM Popup
document.addEventListener('DOMContentLoaded', () => {
  const statusEl = document.getElementById('connectionStatus');
  const nativeStatus = document.getElementById('nativeStatus');
  const scanBtn = document.getElementById('scanVideos');

  // Check connection — retry if service worker is waking up
  function checkConnection(retries = 3) {
    chrome.runtime.sendMessage({ type: 'CONNECTION_STATUS' }, (res) => {
      if (chrome.runtime.lastError) {
        if (retries > 0) {
          setTimeout(() => checkConnection(retries - 1), 500);
          return;
        }
        setDisconnected('Error de conexión');
        return;
      }
      if (res?.connected) {
        setConnected();
      } else {
        setDisconnected('App no disponible');
      }
    });
  }

  function setConnected() {
    if (statusEl) { statusEl.textContent = '● Conectado'; statusEl.className = 'status connected'; }
    if (nativeStatus) { nativeStatus.textContent = '● App conectada'; nativeStatus.className = 'badge badge-on'; }
  }

  function setDisconnected(msg) {
    if (statusEl) { statusEl.textContent = '● Desconectado'; statusEl.className = 'status'; }
    if (nativeStatus) { nativeStatus.textContent = `● ${msg || 'App no conectada'}`; nativeStatus.className = 'badge badge-off'; }
  }

  checkConnection();

  // Scan page for videos
  if (scanBtn) {
    scanBtn.addEventListener('click', async () => {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      chrome.tabs.sendMessage(tab.id, { type: 'TOGGLE_CAPTURE' }).catch(() => {});
      chrome.runtime.sendMessage({ type: 'ATTACH_DEBUGGER', tabId: tab.id }).catch(() => {});
      window.close();
    });
  }

  // Settings link
  const settingsLink = document.getElementById('openSettings');
  if (settingsLink) {
    settingsLink.addEventListener('click', (e) => {
      e.preventDefault();
      chrome.tabs.create({ url: 'settings.html' });
    });
  }
});
