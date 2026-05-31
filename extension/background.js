// DarkDM — Auto-detect HLS streams
console.log('[DarkDM] Auto-detection mode loaded');

const NH = 'com.darkdm.manager';
let proxyActive = false;
const capturedMedia = {}; // Store detected streams with headers

setInterval(() => { chrome.runtime.getPlatformInfo(() => {}); }, 15000);

// ============================================================
// Auto-detect .m3u8 streams
// ============================================================
chrome.webRequest.onSendHeaders.addListener(function(details) {
  if (details.tabId <= 0) return;
  var url = details.url;
  var isM3u8 = url.includes('.m3u8');
  
  if (isM3u8) {
    // Capture request headers
    var headers = {};
    if (details.requestHeaders) {
      for (var i = 0; i < details.requestHeaders.length; i++) {
        var h = details.requestHeaders[i];
        headers[h.name.toLowerCase()] = h.value;
      }
    }
    // Ensure referer is captured (Chrome filters it without extraHeaders)
    if (!headers['referer'] && details.initiator) {
      headers['referer'] = details.initiator;
    }
    
    // Store media info
    if (!capturedMedia[details.tabId]) capturedMedia[details.tabId] = [];
    
    var mediaInfo = {
      url: url,
      type: 'm3u8',
      headers: headers,
      timestamp: Date.now()
    };
    
    // Avoid duplicates
    var existing = capturedMedia[details.tabId].find(function(m) { return m.url === url; });
    if (!existing) {
      capturedMedia[details.tabId].push(mediaInfo);
      console.log('[DM] M3U8 detected:', url.slice(0, 100));
      
      // Fetch and parse manifest
      fetch(url, { headers: headers }).then(function(response) {
        return response.ok ? response.text() : null;
      }).then(function(body) {
        if (!body) return;
        
        var duration = 0;
        var isMaster = body.includes('#EXT-X-STREAM-INF');
        var variantUrl = null;
        
        if (isMaster) {
          // Parse master to find best quality variant
          var lines = body.split('\n');
          var bestBandwidth = 0;
          for (var i = 0; i < lines.length; i++) {
            var line = lines[i];
            if (line.startsWith('#EXT-X-STREAM-INF')) {
              var bwMatch = line.match(/BANDWIDTH=(\d+)/);
              var bandwidth = bwMatch ? parseInt(bwMatch[1]) : 0;
              if (bandwidth > bestBandwidth && lines[i + 1] && !lines[i + 1].startsWith('#')) {
                bestBandwidth = bandwidth;
                variantUrl = lines[i + 1].trim();
              }
            }
          }
          // Resolve relative URL
          if (variantUrl && !variantUrl.startsWith('http')) {
            var baseUrl = url.substring(0, url.lastIndexOf('/') + 1);
            variantUrl = baseUrl + variantUrl;
          }
        } else {
          var lines = body.split('\n');
          for (var i = 0; i < lines.length; i++) {
            var match = lines[i].match(/#EXTINF:([\d.]+)/);
            if (match) duration += parseFloat(match[1]);
          }
        }
        
        if (capturedMedia[details.tabId]) {
          var entry = capturedMedia[details.tabId].find(function(m) { return m.url === url; });
          if (entry) {
            entry.duration = duration;
            entry.isMaster = isMaster;
            entry.manifestBody = body;
            entry.variantUrl = variantUrl; // Store best variant URL
          }
        }
      }).catch(function() {});
    }
  }
}, { urls: ['http://*/*', 'https://*/*'] }, ['requestHeaders']);

chrome.tabs.onRemoved.addListener(function(tabId) {
  delete capturedMedia[tabId];
});

// ============================================================
// Native messaging via persistent port (MV3 workaround)
// sendNativeMessage is unreliable from service workers in MV3.
// connectNative keeps the port open for reliable communication.
// ============================================================
let nativePort = null;

function ensurePort() {
  return new Promise(resolve => {
    if (nativePort && nativePort.onMessage) {
      resolve(true);
      return;
    }
    try {
      nativePort = chrome.runtime.connectNative(NH);
      nativePort.onDisconnect.addListener(() => {
        console.log('[DM] Native port disconnected');
        nativePort = null;
      });
      nativePort.onMessage.addListener(() => {});
      resolve(true);
    } catch (e) {
      console.log('[DM] Native port error:', e);
      nativePort = null;
      resolve(false);
    }
  });
}

function sn(msg) {
  return new Promise(async r => {
    try {
      await ensurePort();
      if (!nativePort) { r(null); return; }
      
      var handler = function(resp) {
        try { nativePort.onMessage.removeListener(handler); } catch(e) {}
        r(resp);
      };
      nativePort.onMessage.addListener(handler);
      
      try {
        nativePort.postMessage(msg);
      } catch (e) {
        try { nativePort.onMessage.removeListener(handler); } catch(e2) {}
        r(null);
      }
      
      // Timeout
      setTimeout(() => {
        try { nativePort.onMessage.removeListener(handler); } catch(e) {}
        r(null);
      }, 120000); // 2min for downloads
    } catch (e) { r(null); }
  });
}

// ============================================================
// Proxy control
// ============================================================
async function startProxy(password, domain) {
  if (proxyActive) return { success: false, error: 'Ya activo' };
  const resp = await sn({
    type: 'PROXY_START',
    sudo_password: password || '',
    target_domain: domain || ''
  });
  if (resp?.success) {
    proxyActive = true;
    return { success: true, message: 'Proxy + iptables activo' };
  }
  return { success: false, error: resp?.error || 'Error al iniciar proxy' };
}

async function stopProxy() {
  if (!proxyActive) return { success: false, error: 'No activo' };
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
chrome.runtime.onMessage.addListener((msg, _, sendResponse) => {
  switch (msg.type) {
    case 'GET_CAPTURED_MEDIA':
      if (msg.tabId && capturedMedia[msg.tabId]) {
        sendResponse({ media: capturedMedia[msg.tabId] });
      } else {
        sendResponse({ media: [] });
      }
      return true;
    
    case 'DOWNLOAD_MEDIA':
      (async function() {
        try {
          const media = msg.media;
          const tabUrl = msg.tabUrl || '';
          const title = msg.tabTitle || 'video';
          
          // Use variant URL if it's a master playlist
          const downloadUrl = media.variantUrl || media.url;
          
          console.log('[DM] Download:', media.isMaster ? 'variant' : 'direct', downloadUrl.slice(0, 100));
          
          var resp = await sn({
            type: 'DOWNLOAD_MANIFEST',
            manifest_url: downloadUrl,
            cookies: '',
            title: title,
            manifest_body: media.manifestBody || '',
            page_url: tabUrl,
            headers: JSON.stringify(media.headers || {})
          });
          sendResponse({
            success: !!(resp && resp.success),
            error: (resp && resp.error) || ''
          });
        } catch(e) {
          sendResponse({ success: false, error: String(e) });
        }
      })();
      return true;
    
    case 'START_PROXY':
      startProxy(msg.password, msg.domain).then(sendResponse);
      return true;
    case 'STOP_PROXY':
      stopProxy().then(sendResponse);
      return true;
  }
});
