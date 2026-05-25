// DarkDM Background — Captura de manifests + yt-dlp (como IDM)
console.log('[DarkDM] v6-restore loaded');

const NH = 'com.darkdm.manager';
const debuggerTabs = new Set();
const manifestCache = {};

// Keep alive
setInterval(() => { chrome.runtime.getPlatformInfo(() => {}); }, 15000);

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
// Debugger — captura manifests
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

      if (method === 'Network.requestWillBeSent') {
        const url = params.request?.url || '';
        const accept = (params.request?.headers?.['Accept'] || '').toLowerCase();
        // Detect manifests: .m3u8/.mpd en la URL o en los headers
        if (url.match(/\.(m3u8|mpd)(\?|$)/i) || url.includes('.m3u8') || url.includes('.mpd') ||
            accept.includes('mpegurl') || accept.includes('dash+xml') ||
            url.match(/playlist|master\.m3u/i)) {
          manifestCache[tabId] = {
            url: url,
            headers: params.request?.headers || {},
            pageUrl: params.documentURL || '',
            requestId: params.requestId,
            body: null,
            capturedAt: Date.now()
          };
          console.log('[DarkDM] Manifest detected:', url.substring(0, 100));
        }
      }

      if (method === 'Network.loadingFinished') {
        const reqId = params.requestId;
        // Check all manifests
        for (const key of Object.keys(manifestCache)) {
          const entry = manifestCache[key];
          if (entry && entry.requestId === reqId && !entry.body) {
            chrome.debugger.sendCommand({ tabId }, 'Network.getResponseBody', { requestId: reqId }, resp => {
              if (resp?.body) {
                entry.body = resp.base64Encoded ? atob(resp.body) : resp.body;
                const isMaster = entry.body.includes('#EXT-X-STREAM-INF');
                console.log('[DarkDM] Manifest body:', 
                  isMaster ? 'MASTER' : 'VARIANT',
                  entry.body.substring(0, 100).replace(/\n/g, ' | '));
              }
            });
          }
        }
      }
    });

    chrome.debugger.onDetach.addListener(src => {
      if (src.tabId === tabId) {
        debuggerTabs.delete(tabId);
        Object.keys(manifestCache).forEach(k => {
          if (k.startsWith(tabId + '')) delete manifestCache[k];
        });
      }
    });
  } catch (e) {
    console.error('[DarkDM] Debugger error:', e);
  }
}

// ============================================================
// Buscar mejor manifest: master > variante, actualizar URL
// ============================================================
function findBestManifest(tabId) {
  let best = manifestCache[tabId];
  if (!best?.body) return null;

  const isMaster = best.body.includes('#EXT-X-STREAM-INF');
  
  // Si es variante, intentar encontrar el master
  if (!isMaster) {
    console.log('[DarkDM] Variant manifest, trying to find master');
    // La URL de la variante podría ser como:
    // https://site.com/hls/720p/playlist.m3u8
    // El master suele estar en el directorio padre
    try {
      const url = new URL(best.url);
      const pathParts = url.pathname.split('/');
      // Quitar el nombre del archivo y probar patrones comunes
      const basePath = pathParts.slice(0, -1).join('/');
      const candidates = [
        `${url.origin}${basePath}/master.m3u8`,
        `${url.origin}${basePath}/playlist.m3u8`,
        `${url.origin}${basePath}/index.m3u8`,
        // Quitar un nivel más (si es /720p/playlist.m3u8)
        `${url.origin}${pathParts.slice(0, -2).join('/')}/master.m3u8`,
        `${url.origin}${pathParts.slice(0, -2).join('/')}/playlist.m3u8`,
      ];
      console.log('[DarkDM] Master candidates:', candidates);
      // Store candidates for the backend to try
      best.masterCandidates = candidates;
    } catch(e) {}
  }

  return best;
}

// ============================================================
// Descargar video via yt-dlp (como IDM)
// ============================================================
async function downloadVideo(manifest, title) {
  if (!manifest?.url) return { success: false, error: 'No manifest URL' };

  // Extraer cookies del dominio
  let cookieStr = '';
  try {
    const domain = new URL(manifest.url).hostname;
    const cookies = await new Promise(resolve => {
      chrome.cookies.getAll({ domain }, resolve);
    });
    if (cookies?.length) {
      cookieStr = '# Netscape HTTP Cookie File\n' + cookies.map(c => {
        const exp = Math.floor(c.expirationDate || 4102444800);
        const dom = c.domain.startsWith('.') ? c.domain : '.' + c.domain;
        return `${dom}\tTRUE\t${c.path}\t${c.secure ? 'TRUE' : 'FALSE'}\t${exp}\t${c.name}\t${c.value}`;
      }).join('\n');
    }
  } catch(e) {}

  // Enviar a native host
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
// Context menus
// ============================================================
try {
  if (chrome.contextMenus) {
    chrome.runtime.onInstalled.addListener(() => {
      try {
        chrome.contextMenus.create({id:'ddm-video', title:'Download with DarkDM', contexts:['video','audio']});
        chrome.contextMenus.create({id:'ddm-page', title:'DarkDM - Detect video', contexts:['page']});
        chrome.contextMenus.create({id:'ddm-link', title:'Download with DarkDM', contexts:['link']});
      } catch(e) {}
    });
    chrome.contextMenus.onClicked.addListener((info, tab) => {
      if (info.menuItemId === 'ddm-link') sn({ type: 'START_DOWNLOAD', url: info.linkUrl });
      else chrome.tabs.sendMessage(tab.id, { type: 'TOGGLE_CAPTURE' }).catch(() => {});
    });
  }
} catch(e) {}

try {
  if (chrome.action)
    chrome.action.onClicked.addListener(tab => {
      chrome.tabs.sendMessage(tab.id, { type: 'TOGGLE_CAPTURE' }).catch(() => {});
    });
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

      // Buscar manifest capturado
      const manifest = findBestManifest(tabId);
      if (!manifest) {
        // Fallback: intentar yt-dlp con la URL de la página
        if (sender?.tab?.url) {
          downloadVideo({ url: sender.tab.url, headers: {}, body: '' }, msg.title).then(sendResponse);
        } else {
          sendResponse({ success: false, error: 'No se detectó manifest .m3u8. Asegúrate de que el video se esté reproduciendo.' });
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

// ============================================================
// Auto-attach en tabs con video
// ============================================================
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === 'complete' && tab.url) {
    setTimeout(() => attachDebugger(tabId), 1500);
  }
});
