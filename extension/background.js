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
      // Enviar a native host (yt-dlp con cookies)
      sn({type:'EXTRACT_PAGE', url: msg.url, title: msg.title, tab_id: tabId, 
          site: msg.site, hasDrm: msg.hasDrm})
        .then(resp => {
          const success = resp && (resp.msg_type === 'DOWNLOAD_STARTED' || resp.msg_type === 'DOWNLOAD_RESULT') && resp.success;
          sendResponse({success: !!success, msg: resp?.message || 'No response'});
        })
        .catch(() => sendResponse({success: false, msg: 'Native host not reachable'}));
      return true;

    case 'TAB_CAPTURE':
      // Fallback: chrome.tabCapture para sitios con DRM
      chrome.tabCapture.capture({ video: true, audio: true }, stream => {
        if (chrome.runtime.lastError || !stream) {
          sendResponse({success: false, error: chrome.runtime.lastError?.message || 'No stream'});
          return;
        }
        // Iniciar grabación en background
        try {
          const mt = MediaRecorder.isTypeSupported('video/webm;codecs=vp9,opus') 
            ? 'video/webm;codecs=vp9,opus' : 'video/webm';
          const rec = new MediaRecorder(stream, { mimeType: mt, videoBitsPerSecond: 8000000 });
          const parts = [];
          rec.ondataavailable = e => { if (e.data?.size > 1000) parts.push(e.data); };
          rec.onstop = () => {
            stream.getTracks().forEach(t => t.stop());
            if (parts.length) {
              const blob = new Blob(parts, { type: mt });
              const url = URL.createObjectURL(blob);
              const fname = msg.filename || ('tab_capture_' + Date.now() + '.webm');
              chrome.downloads.download({ url, filename: 'DarkDM/' + fname, saveAs: false });
              setTimeout(() => URL.revokeObjectURL(url), 10000);
            }
          };
          rec.start(15000);
          sendResponse({success: true, recording: true});
        } catch(e) {
          sendResponse({success: false, error: e.message});
        }
      });
      return true;

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
