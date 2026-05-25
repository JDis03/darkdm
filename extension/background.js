// DarkDM Background — Intercepción de video por Content-Type (bajo nivel)
console.log('[DarkDM] v7 loaded — content-type interception');

const NH = 'com.darkdm.manager';
const debuggerTabs = new Set();
const captureSessions = {}; // tabId -> { session, count, pending }

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
// Debugger — intercepta respuestas de video por Content-Type
// ============================================================
async function attachDebugger(tabId) {
  if (!chrome.debugger || debuggerTabs.has(tabId)) return;
  try {
    await chrome.debugger.attach({ tabId }, '1.3');
    await chrome.debugger.sendCommand({ tabId }, 'Network.enable', {
      maxTotalBufferSize: 100000000,  // 100MB buffer
      maxResourceBufferSize: 5000000   // 5MB per resource
    });
    debuggerTabs.add(tabId);
    console.log('[DarkDM] Debugger attached:', tabId);

    chrome.debugger.onEvent.addListener((src, method, params) => {
      if (src.tabId !== tabId) return;
      
      // === Detect video responses by Content-Type ===
      if (method === 'Network.responseReceived') {
        const url = params.response?.url || '';
        const ct = (params.response?.headers?.['Content-Type'] || params.response?.mimeType || '').toLowerCase();
        const isVideo = ct.includes('video/') || ct.includes('application/vnd.apple.mpegurl') || 
                        ct.includes('application/dash+xml') || ct.includes('binary/octet-stream');
        const isVideoExt = url.match(/\.(ts|m4s|m4v|mp4|webm|m3u8|mpd)(\?|$)/i);
        
        if (isVideo || isVideoExt) {
          // Store the request ID for capture
          if (captureSessions[tabId]) {
            const reqId = params.requestId;
            captureSessions[tabId].pending[reqId] = { url, contentType: ct };
            
            // Update status via content script
            try {
              chrome.tabs.sendMessage(tabId, { 
                type: 'CAPTURE_STATUS', 
                pending: Object.keys(captureSessions[tabId].pending).length,
                captured: captureSessions[tabId].count
              });
            } catch(e) {}
          }
        }
      }
      
      // === Capture response body when loading finishes ===
      if (method === 'Network.loadingFinished' && captureSessions[tabId]) {
        const session = captureSessions[tabId];
        const pending = session.pending[params.requestId];
        if (!pending) return;
        
        delete session.pending[params.requestId];
        
        // Get the response body
        chrome.debugger.sendCommand(
          { tabId },
          'Network.getResponseBody',
          { requestId: params.requestId },
          resp => {
            if (chrome.runtime.lastError || !resp?.body) return;
            
            const seq = session.count++;
            const data = resp.base64Encoded ? resp.body : btoa(resp.body);
            
            // Send to native host
            sn({
              type: 'SEGMENT_DATA',
              data: data,
              base64: true,
              seq: seq,
              session: session.session,
              url: pending.url,
              content_type: pending.contentType
            });
          }
        );
      }
    });

    chrome.debugger.onDetach.addListener(src => {
      if (src.tabId === tabId) {
        debuggerTabs.delete(tabId);
        delete captureSessions[tabId];
      }
    });
  } catch (e) {
    console.error('[DarkDM] Debugger attach failed:', e);
  }
}

// ============================================================
// Capture session controls
// ============================================================
function startCapture(tabId, title) {
  const session = 'darkdm_' + Date.now() + '_' + Math.random().toString(36).substr(2, 6);
  captureSessions[tabId] = {
    session: session,
    count: 0,
    pending: {},
    title: title || 'video',
    startTime: Date.now()
  };
  
  if (!debuggerTabs.has(tabId)) attachDebugger(tabId);
  
  console.log('[DarkDM] Capture started:', session);
  return { success: true, session };
}

async function stopCapture(tabId) {
  if (!captureSessions[tabId]) return { success: false, error: 'No active capture' };
  
  const info = captureSessions[tabId];
  console.log(`[DarkDM] Capture stopped: ${info.count} segments in ${(Date.now() - info.startTime)/1000}s`);
  
  delete captureSessions[tabId];
  
  // Tell native host to concatenate
  const resp = await sn({
    type: 'CONCATENATE_SEGMENTS',
    session: info.session,
    count: info.count,
    title: info.title
  });
  
  return { success: true, segments: info.count, message: resp?.message || '' };
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

    case 'START_CAPTURE':
      if (!tabId) { sendResponse({ success: false }); return false; }
      sendResponse(startCapture(tabId, msg.title));
      break;

    case 'STOP_CAPTURE':
      if (!tabId) { sendResponse({ success: false }); return false; }
      stopCapture(tabId).then(sendResponse);
      return true;

    case 'ATTACH_DEBUGGER':
      if (tabId) attachDebugger(tabId);
      break;
  }
});

// ============================================================
// Auto-attach debugger on sites with video
// ============================================================
chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status === 'complete' && tab.url) {
    setTimeout(() => attachDebugger(tabId), 1500);
  }
});
