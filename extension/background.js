// DarkDM Background — Descarga de streams como IDM
// 1. Debugger captura manifest .m3u8 + headers
// 2. Extension extrae cookies
// 3. Native host descarga manifest, parsea segmentos, descarga todo
console.log('[DarkDM] IDM-mode loaded');

const NH = 'com.darkdm.manager';
const debuggerTabs = new Set();
// Store manifest info by tabId: { url, body, headers, pageUrl }
const manifestCache = {};

// Keep service worker alive
setInterval(() => { chrome.runtime.getPlatformInfo(() => {}); }, 15000);

// ============================================================
// Native messaging
// ============================================================
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
// Debugger — intercepta manifests .m3u8/.mpd
// ============================================================
async function attachDebugger(tabId) {
  if (!chrome.debugger || debuggerTabs.has(tabId)) return;
  try {
    await chrome.debugger.attach({ tabId }, '1.3');
    await chrome.debugger.sendCommand({ tabId }, 'Network.enable');
    debuggerTabs.add(tabId);

    chrome.debugger.onEvent.addListener((src, method, params) => {
      if (src.tabId !== tabId) return;

      // Intercept manifest requests
      if (method === 'Network.requestWillBeSent') {
        const url = params.request?.url || '';
        const ct = params.request?.headers?.['Accept'] || '';
        // Detect manifests by URL pattern (.m3u8/.mpd anywhere) OR by Accept header
        // Some sites hide the extension (e.g., /get.php?id=m3u8)
        if (url.match(/\.(m3u8|mpd)(\?|$)/i) || 
            url.includes('.m3u8') || url.includes('.mpd') ||
            ct.includes('mpegurl') || ct.includes('dash+xml') ||
            url.match(/playlist|manifest|master\.m3u/i)) {
          const entry = {
            url: url,
            headers: params.request?.headers || {},
            pageUrl: params.documentURL || '',
            requestId: params.requestId,
            body: null
          };
          
          // Store ALL manifests found. The first one is likely the master.
          // If we already have one, only overwrite if new one looks like master
          // or if the current one has no body yet.
          if (!manifestCache[tabId] || !manifestCache[tabId].body) {
            manifestCache[tabId] = entry;
          } else if (manifestCache[tabId] && url !== manifestCache[tabId].url) {
            // Different URL — could be a variant or better manifest
            console.log('[DarkDM] Additional manifest detected:', url);
            // Store as a secondary candidate (might be master if we only had variant)
            manifestCache[tabId + '_secondary'] = entry;
          }
          console.log('[DarkDM] Manifest detected:', url);
        }
      }

      // Get the manifest body when it finishes loading
      if (method === 'Network.loadingFinished') {
        const reqId = params.requestId;
        // Check primary manifest
        if (manifestCache[tabId] && manifestCache[tabId].requestId === reqId && !manifestCache[tabId].body) {
          chrome.debugger.sendCommand({ tabId }, 'Network.getResponseBody', { requestId: reqId }, resp => {
            if (resp?.body) {
              const decoded = resp.base64Encoded ? atob(resp.body) : resp.body;
              manifestCache[tabId].body = decoded;
              console.log('[DarkDM] Manifest body:', decoded.substring(0, 200));
              // If this is a variant (has EXTINF but no STREAM-INF), 
              // try to find a master manifest from secondary
              if (!decoded.includes('#EXT-X-STREAM-INF') && manifestCache[tabId + '_secondary']) {
                const sec = manifestCache[tabId + '_secondary'];
                if (sec.body && sec.body.includes('#EXT-X-STREAM-INF')) {
                  console.log('[DarkDM] Switching to master manifest');
                  manifestCache[tabId] = sec;
                }
              }
            }
          });
        }
        // Check secondary manifest
        if (manifestCache[tabId + '_secondary'] && manifestCache[tabId + '_secondary'].requestId === reqId && !manifestCache[tabId + '_secondary'].body) {
          chrome.debugger.sendCommand({ tabId }, 'Network.getResponseBody', { requestId: reqId }, resp => {
            if (resp?.body) {
              manifestCache[tabId + '_secondary'].body = resp.base64Encoded ? atob(resp.body) : resp.body;
              console.log('[DarkDM] Secondary manifest body:', manifestCache[tabId + '_secondary'].body.substring(0, 200));
            }
          });
        }
      }
    });

    chrome.debugger.onDetach.addListener(src => {
      if (src.tabId === tabId) {
        debuggerTabs.delete(tabId);
        delete manifestCache[tabId];
      }
    });
  } catch (e) {}
}

// ============================================================
// Cookie extraction (Netscape format)
// ============================================================
function cookiesToNetscape(cookies) {
  return '# Netscape HTTP Cookie File\n' +
    cookies.map(c => {
      const exp = Math.floor(c.expirationDate || 4102444800);
      const domain = c.domain.startsWith('.') ? c.domain : '.' + c.domain;
      return `${domain}\tTRUE\t${c.path}\t${c.secure ? 'TRUE' : 'FALSE'}\t${exp}\t${c.name}\t${c.value}`;
    }).join('\n');
}

// ============================================================
// Helper: extract cookies for a URL
// ============================================================
async function extractCookies(pageUrl) {
  try {
    const domain = new URL(pageUrl).hostname;
    const cookies = await new Promise(resolve => {
      chrome.cookies.getAll({ domain }, resolve);
    });
    if (cookies?.length) return cookiesToNetscape(cookies);
  } catch(e) {}
  return '';
}

// ============================================================
// DOWNLOAD STREAM — como IDM
// ============================================================
async function downloadStream(tabId, title, sender) {
  if (!tabId) return { success: false, error: 'No tab' };

  // 1) Get manifest from cache — prefer master (has #EXT-X-STREAM-INF) over variant
  let manifest = manifestCache[tabId];
  const secondary = manifestCache[tabId + '_secondary'];
  
  if (secondary?.body?.includes('#EXT-X-STREAM-INF') && !manifest?.body?.includes('#EXT-X-STREAM-INF')) {
    // Secondary is the master manifest, primary is a variant — switch
    console.log('[DarkDM] Using master manifest from secondary cache');
    manifest = secondary;
  }
  
  if (!manifest || !manifest.body) {
    // Fallback: try yt-dlp on the page URL directly (works for YouTube, etc.)
    console.log('[DarkDM] No manifest captured, trying yt-dlp on page URL');
    if (sender?.tab?.url) {
      const pageUrl = sender.tab.url;
      const cookies = await extractCookies(pageUrl);
      const resp = await sn({
        type: 'EXTRACT_PAGE', url: pageUrl, hasDrm: false, cookies: cookies,
        title: title || ''
      });
      if (resp?.success) return resp;
    }
    return { success: false, error: 'No se detectó manifest .m3u8. Asegúrate de que el video se esté reproduciendo y el debugger esté conectado.' };
  }

  // 2) Extract cookies for the domain
  let pageUrl = manifest.pageUrl;
  if (!pageUrl && manifest.url) {
    try { pageUrl = new URL(manifest.url).origin; } catch(e) {}
  }
  
  let cookieStr = '';
  try {
    const domain = new URL(pageUrl).hostname;
    const cookies = await new Promise(resolve => {
      chrome.cookies.getAll({ domain }, resolve);
    });
    if (cookies?.length) {
      cookieStr = cookiesToNetscape(cookies);
      console.log(`[DarkDM] Got ${cookies.length} cookies for ${domain}`);
    }
  } catch(e) {}

  // 3) Build headers string (critical: User-Agent, Referer, etc.)
  const headers = manifest.headers || {};
  // Ensure we have a proper User-Agent
  if (!headers['User-Agent'] && !headers['user-agent']) {
    headers['User-Agent'] = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36';
  }
  // Ensure Referer is set
  if (!headers['Referer'] && !headers['referer']) {
    headers['Referer'] = manifest.pageUrl || pageUrl;
  }

  // 4) Send to native host
  return await sn({
    type: 'DOWNLOAD_MANIFEST',
    manifest_url: manifest.url,
    manifest_body: manifest.body,
    cookies: cookieStr,
    headers: JSON.stringify(headers),
    page_url: manifest.pageUrl,
    title: title || 'video',
    tab_id: tabId
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

// Action button
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
      downloadStream(tabId, msg.title, sender).then(sendResponse);
      return true;

    case 'ATTACH_DEBUGGER':
      if (tabId) attachDebugger(tabId);
      break;

    case 'START_DOWNLOAD':
      sn({ type: 'START_DOWNLOAD', url: msg.url, filename: msg.filename, tab_id: tabId });
      break;
  }
});

// ============================================================
// Auto-attach debugger on known video/DRM sites
// ============================================================
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === 'complete' && tab.url) {
    const videoSites = ['netflix.com', 'primevideo.com', 'disneyplus.com', 
                        'hbomax.com', 'max.com', 'hulu.com', 'paramountplus.com',
                        'tv.apple.com', 'youtube.com', 'vimeo.com',
                        'player4me', 'solo-latino', 'stream'];
    if (videoSites.some(s => tab.url.includes(s))) {
      setTimeout(() => attachDebugger(tabId), 2000);
    }
  }
});
