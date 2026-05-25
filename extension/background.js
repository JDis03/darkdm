// DarkDM — Debugger + yt-dlp (La solución limpia)
console.log('[DarkDM] Final version loaded');

const NH = 'com.darkdm.manager';
const debuggerTabs = new Set();
const manifestCache = {};

setInterval(() => { chrome.runtime.getPlatformInfo(() => {}); }, 15000);

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
// Debugger — captura manifests por URL y Content-Type
// ============================================================
async function attachDebugger(tabId) {
  if (!chrome.debugger || debuggerTabs.has(tabId)) return;
  try {
    await chrome.debugger.attach({ tabId }, '1.3');
    await chrome.debugger.sendCommand({ tabId }, 'Network.enable', {
      maxTotalBufferSize: 100000000,
      maxResourceBufferSize: 5000000
    });
    debuggerTabs.add(tabId);

    chrome.debugger.onEvent.addListener((src, method, params) => {
      if (src.tabId !== tabId) return;

      // Detectar manifests (.m3u8, .mpd)
      if (method === 'Network.requestWillBeSent') {
        const url = params.request?.url || '';
        const accept = (params.request?.headers?.['Accept'] || '').toLowerCase();
        if (url.match(/\.(m3u8|mpd)(\?|$)/i) || url.includes('.m3u8') || url.includes('.mpd') ||
            accept.includes('mpegurl') || accept.includes('dash+xml') ||
            url.match(/playlist|master\.m3u/i)) {
          manifestCache[tabId] = {
            url, headers: params.request?.headers || {},
            pageUrl: params.documentURL || '',
            requestId: params.requestId, body: null
          };
          console.log('[DarkDM] Manifest:', url.substring(0, 100));
        }
      }

      // Capturar body del manifest
      if (method === 'Network.loadingFinished' && manifestCache[tabId]?.requestId === params.requestId) {
        chrome.debugger.sendCommand({ tabId }, 'Network.getResponseBody',
          { requestId: params.requestId }, resp => {
          if (resp?.body) {
            manifestCache[tabId].body = resp.base64Encoded ? atob(resp.body) : resp.body;
            const isMaster = manifestCache[tabId].body.includes('#EXT-X-STREAM-INF');
            console.log('[DarkDM] Manifest:', isMaster ? 'MASTER' : 'VARIANT',
              manifestCache[tabId].body.substring(0, 80).replace(/\n/g, ' | '));
          }
        });
      }
    });

    chrome.debugger.onDetach.addListener(() => {
      debuggerTabs.delete(tabId);
      delete manifestCache[tabId];
    });
  } catch (e) {
    console.error('[DarkDM] Debugger error:', e);
  }
}

// ============================================================
// Buscar mejor manifest
// ============================================================
function findBestManifest(tabId) {
  const m = manifestCache[tabId];
  if (!m?.body) return m;

  // Si es variante, intentar construir URL del master
  if (m.body.includes('#EXTINF') && !m.body.includes('#EXT-X-STREAM-INF')) {
    try {
      const url = new URL(m.url);
      const parts = url.pathname.split('/');
      // Quitar nombre de archivo y probar patrones comunes
      const base = parts.slice(0, -1).join('/');
      m.masterCandidates = [
        `${url.origin}${base}/master.m3u8`,
        `${url.origin}${base}/playlist.m3u8`,
        `${url.origin}${base}/index.m3u8`,
        `${url.origin}${parts.slice(0, -2).join('/')}/master.m3u8`,
      ];
      console.log('[DarkDM] Master candidates:', m.masterCandidates);
    } catch(e) {}
  }
  return m;
}

// ============================================================
// Descargar
// ============================================================
async function downloadVideo(manifest, title) {
  if (!manifest?.url) return { success: false, error: 'Sin URL de manifest' };

  // Cookies via extension API
  let cookieStr = '';
  try {
    const domain = new URL(manifest.url).hostname;
    const cookies = await new Promise(resolve => chrome.cookies.getAll({ domain }, resolve));
    if (cookies?.length) {
      cookieStr = '# Netscape HTTP Cookie File\n' + cookies.map(c => {
        const exp = Math.floor(c.expirationDate || 4102444800);
        const dom = c.domain.startsWith('.') ? c.domain : '.' + c.domain;
        return `${dom}\tTRUE\t${c.path}\t${c.secure ? 'TRUE' : 'FALSE'}\t${exp}\t${c.name}\t${c.value}`;
      }).join('\n');
    }
  } catch(e) {}

  return await sn({
    type: 'DOWNLOAD_MANIFEST',
    manifest_url: manifest.url,
    manifest_body: manifest.body || '',
    cookies: cookieStr,
    headers: JSON.stringify(manifest.headers || {}),
    page_url: manifest.pageUrl || '',
    title: title || 'video',
    master_candidates: JSON.stringify(manifest.masterCandidates || [])
  });
}

// ============================================================
// Message handler
// ============================================================
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  const tabId = sender.tab?.id;

  switch (msg.type) {
    case 'CONNECTION_STATUS':
      sn({ type: 'PING' }).then(r => sendResponse({ connected: r !== null }));
      return true;

    case 'DOWNLOAD_STREAM':
      if (!tabId) { sendResponse({ success: false, error: 'No tab' }); return false; }
      const manifest = findBestManifest(tabId);
      if (!manifest) {
        // Fallback: yt-dlp con URL de la página
        if (sender?.tab?.url) {
          downloadVideo({ url: sender.tab.url, body: '' }, msg.title).then(sendResponse);
        } else {
          sendResponse({ success: false, error: 'No manifest. Reproduce el video primero.' });
        }
        return true;
      }
      downloadVideo(manifest, msg.title).then(sendResponse);
      return true;

    case 'ATTACH_DEBUGGER':
      if (tabId) attachDebugger(tabId);
      break;
  }
});

// Auto-attach en todas las tabs
chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.status === 'complete') {
    setTimeout(() => attachDebugger(tabId), 2000);
  }
});
