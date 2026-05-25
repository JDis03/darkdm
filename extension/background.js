// DarkDM Background — Proxy mode con proceso independiente
console.log('[DarkDM] Proxy-standalone mode loaded');

const NH = 'com.darkdm.manager';
let proxyActive = false;

// Keep alive (evita que el SW se duerma mientras el proxy corre)
setInterval(() => { chrome.runtime.getPlatformInfo(() => {}); }, 10000);

// Native messaging
function sn(msg) {
  return new Promise(r => {
    try {
      chrome.runtime.sendNativeMessage(NH, msg, resp => {
        if (chrome.runtime.lastError) r(null); else r(resp);
      });
    } catch (e) { r(null); }
  });
}

// ============================================================
// Proxy Capture (proceso independiente)
// ============================================================
async function startProxyCapture() {
  if (proxyActive) return { success: false, error: 'Proxy ya activo' };

  // 1) Iniciar proxy en native host (lanza darkdm-proxy como proceso hijo)
  const resp = await sn({ type: 'PROXY_START' });
  if (!resp?.success) {
    return { success: false, error: resp?.error || 'Error al iniciar proxy' };
  }

  // 2) Configurar navegador para usar el proxy
  try {
    await new Promise((resolve, reject) => {
      chrome.proxy.settings.set({
        value: {
          mode: 'fixed_servers',
          rules: {
            singleProxy: { scheme: 'http', host: '127.0.0.1', port: 8899 },
            bypassList: ['127.0.0.1', 'localhost', '::1']
          }
        },
        scope: 'regular'
      }, () => {
        if (chrome.runtime.lastError) reject(chrome.runtime.lastError);
        else resolve();
      });
    });

    proxyActive = true;
    console.log('[DarkDM] Proxy active: localhost:8899');
    return { success: true, port: 8899, message: resp.message };

  } catch (e) {
    // Si falla el proxy, detenerlo
    await sn({ type: 'PROXY_STOP' });
    return { success: false, error: 'Error al configurar proxy: ' + e.message };
  }
}

async function stopProxyCapture() {
  if (!proxyActive) return { success: false, error: 'No hay proxy activo' };

  // 1) Limpiar proxy del navegador
  try {
    await new Promise((resolve) => {
      chrome.proxy.settings.clear({ scope: 'regular' }, () => resolve());
    });
  } catch (e) {}

  // 2) Detener el proxy (mata el proceso)
  const resp = await sn({ type: 'PROXY_STOP' });
  proxyActive = false;

  return {
    success: resp?.success !== false,
    segments: resp?.segments || 0,
    message: resp?.message || 'Proxy detenido'
  };
}

// ============================================================
// Message handler
// ============================================================
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  switch (msg.type) {
    case 'CONNECTION_STATUS':
      sn({ type: 'PING' }).then(r => sendResponse({ connected: r !== null }));
      return true;

    case 'START_PROXY_CAPTURE':
      startProxyCapture().then(sendResponse);
      return true;

    case 'STOP_PROXY_CAPTURE':
      stopProxyCapture().then(sendResponse);
      return true;

    case 'DOWNLOAD_STREAM':
      // Fallback: usa yt-dlp directo (para YouTube, etc.)
      if (!sender?.tab?.id) { sendResponse({ success: false }); return false; }
      const pageUrl = sender.tab.url || msg.url;
      sn({
        type: 'EXTRACT_PAGE',
        url: pageUrl,
        title: msg.title || '',
        hasDrm: false
      }).then(resp => {
        const ok = resp && (resp.msg_type === 'DOWNLOAD_STARTED' || resp.msg_type === 'DOWNLOAD_RESULT') && resp.success;
        sendResponse({ success: !!ok, message: resp?.message, bytes: resp?.bytes });
      }).catch(() => sendResponse({ success: false, error: 'Host no disponible' }));
      return true;
  }
});
