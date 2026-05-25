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
// MANIFEST_FOUND — desde content script MAIN world (el más temprano)
// ============================================================
chrome.runtime.onMessage.addListener((msg, sender) => {
  if (msg.type === 'MANIFEST_FOUND' && sender.tab?.id) {
    const tabId = sender.tab.id;
    // Guardar en cache (priorizar el que tiene body)
    if (!manifestCache[tabId] || !manifestCache[tabId].body) {
      manifestCache[tabId] = {
        url: msg.url,
        body: msg.body,
        headers: msg.headers || {},
        pageUrl: sender.tab.url || '',
        requestId: 'cs_' + Date.now(),
        source: 'content_script'
      };
      console.log('[DarkDM] Manifest from content script:', 
        msg.url.substring(0, 80),
        msg.body?.includes('#EXT-X-STREAM-INF') ? '(MASTER)' : '(VARIANT)');
    }
  }
});

// ============================================================
// webRequest fallback — captura headers aunque el body no esté disponible
// ============================================================
try {
  if (chrome.webRequest) {
    chrome.webRequest.onSendHeaders.addListener(
      (details) => {
        const url = details.url;
        if (!url.match(/\.(m3u8|mpd)(\?|$)/i)) return;
        const tabId = details.tabId;
        if (tabId < 0) return;
        if (manifestCache[tabId]?.url === url) return; // ya lo tenemos
        
        // Extraer headers relevantes
        const headers = {};
        (details.requestHeaders || []).forEach(({ name, value }) => {
          const n = name.toLowerCase();
          if (['referer', 'origin', 'cookie', 'user-agent'].includes(n)) {
            headers[name] = value;
          }
        });
        
        manifestCache[tabId] = {
          url, headers, body: null, pageUrl: details.documentUrl || '',
          requestId: 'wr_' + details.requestId,
          source: 'webrequest'
        };
        console.log('[DarkDM] Manifest via webRequest:', url.substring(0, 80));
      },
      { urls: ['<all_urls>'], types: ['xmlhttprequest', 'media', 'other', 'script'] },
      ['requestHeaders']
    );
  }
} catch(e) {}

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
