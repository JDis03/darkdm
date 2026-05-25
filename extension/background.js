// DarkDM Background — Limpio y optimizado
console.log('[DarkDM] loaded');
setInterval(() => { chrome.runtime.getPlatformInfo(() => {}); }, 15000);

const NH = 'com.darkdm.manager';
const debuggerTabs = new Set();

function sn(msg) {
  return new Promise(r => {
    try {
      chrome.runtime.sendNativeMessage(NH, msg, resp => {
        if (chrome.runtime.lastError) r(null); else r(resp);
      });
    } catch (e) { r(null); }
  });
}

// Debugger: detecta streams de video en el tráfico de red
async function attachDebugger(tabId) {
  if (!chrome.debugger || debuggerTabs.has(tabId)) return;
  try {
    await chrome.debugger.attach({ tabId }, '1.3');
    await chrome.debugger.sendCommand({ tabId }, 'Network.enable');
    
    const handler = (src, method, params) => {
      if (src.tabId !== tabId) return;
      if (method === 'Network.responseReceived') {
        const ct = params.response?.headers?.['Content-Type'] || params.response?.mimeType || '';
        const url = params.response?.url || '';
        if (ct.match(/(video|audio|mpegurl|dash)/i) || 
            url.match(/\.(m3u8|mpd|m4s|ts|mp4|webm|woff2)(\?|$)/i) ||
            url.match(/seg-\d+.*\.\w+$/i)) {
          sn({ type: 'STREAM_DETECTED', url, content_type: ct, tab_id: tabId });
        }
      }
      if (method === 'Network.requestWillBeSent') {
        const url = params.request?.url || '';
        if (url.match(/\.(mpd|m3u8)(\?|$)/i)) {
          sn({ type: 'MANIFEST_DETECTED', url, page_url: params.documentURL, tab_id: tabId });
        }
      }
    };
    chrome.debugger.onEvent.addListener(handler);
    chrome.debugger.onDetach.addListener(src => {
      if (src.tabId === tabId) debuggerTabs.delete(tabId);
    });
    debuggerTabs.add(tabId);
  } catch (e) {}
}

// Context menus
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
// Extract cookies from a domain and forward to native host
// (Solves Vivaldi encrypted cookies issue)
// ============================================================
function cookiesToNetscape(cookies) {
  return '# Netscape HTTP Cookie File\n' +
    cookies.map(c => {
      const exp = Math.floor(c.expirationDate || 4102444800);
      const domain = c.domain.startsWith('.') ? c.domain : '.' + c.domain;
      return `${domain}\tTRUE\t${c.path}\t${c.secure ? 'TRUE' : 'FALSE'}\t${exp}\t${c.name}\t${c.value}`;
    }).join('\n');
}

function handleExtractPage(msg, tabId, sendResponse) {
  try {
    const url = new URL(msg.url);
    const domain = url.hostname;
    
    // Get cookies from the extension API (they're already decrypted)
    chrome.cookies.getAll({ domain }, cookieList => {
      if (chrome.runtime.lastError || !cookieList || cookieList.length === 0) {
        // No cookies found via API — maybe need different domain or fallback
        console.log('[DarkDM] No cookies from API for', domain);
        // Still try with empty cookies — yt-dlp might work for some sites
      }
      
      const cookieStr = cookieList?.length ? cookiesToNetscape(cookieList) : '';
      console.log(`[DarkDM] Got ${cookieList?.length || 0} cookies for ${domain}`);
      
      // Send to native host with cookies
      sn({type:'EXTRACT_PAGE', url: msg.url, title: msg.title, tab_id: tabId, 
          site: msg.site, hasDrm: msg.hasDrm, cookies: cookieStr})
        .then(resp => {
          const success = resp && (['DOWNLOAD_STARTED','DOWNLOAD_RESULT'].includes(resp.msg_type) && resp.success);
          const errMsg = resp?.error || resp?.message || 'Sin respuesta del host nativo';
          sendResponse({success: !!success, msg: success ? (resp?.message || 'OK') : errMsg});
        })
        .catch(() => sendResponse({success: false, msg: 'Host nativo no disponible'}));
    });
  } catch (e) {
    console.error('[DarkDM] handleExtractPage error:', e);
    sendResponse({success: false, msg: 'Error: ' + e.message});
  }
  return true; // Keep channel open for async response
}

// Message handler — soporta respuestas asíncronas
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  const tabId = sender.tab?.id;
  switch (msg.type) {
    case 'CONNECTION_STATUS':
      sn({type:'PING'}).then(r => sendResponse({connected: r !== null}));
      return true;

    case 'START_DOWNLOAD':
      sn({type:'START_DOWNLOAD', url: msg.url, filename: msg.filename, tab_id: tabId});
      break;

    case 'VIDEO_DETECTED':
    case 'MEDIA_STREAM':
      sn({type:'STREAM_DETECTED', url: msg.url, tab_id: tabId});
      break;

    case 'EXTRACT_PAGE':
      return handleExtractPage(msg, tabId, sendResponse);

    case 'BUFFER_CAPTURE':
      break;

    case 'SAVE_FILE':
      try { chrome.downloads.download({url: msg.data, filename: 'DarkDM/' + msg.filename, saveAs: false}); } catch(e) {}
      break;

    case 'ATTACH_DEBUGGER':
      if (tabId) attachDebugger(tabId);
      break;
  }
});

// ============================================================
// Auto: detectar DRM y extraer con yt-dlp al cargar página
// ============================================================
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === 'complete' && tab.url) {
    // Si es un sitio DRM conocido, intentar attach debugger automáticamente
    const drmSites = ['netflix.com', 'primevideo.com', 'disneyplus.com', 'hbomax.com', 
                       'max.com', 'hulu.com', 'paramountplus.com', 'tv.apple.com'];
    if (drmSites.some(s => tab.url.includes(s))) {
      setTimeout(() => attachDebugger(tabId), 2000);
    }
  }
});
