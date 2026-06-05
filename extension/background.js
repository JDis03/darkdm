// DarkDM — Auto-detect HLS streams
console.log('[DarkDM] Auto-detection mode loaded');

const capturedMedia = {}; // Store detected streams with headers

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
    
    // Avoid duplicates — but re-fetch if existing entry was an ad
    var existing = capturedMedia[details.tabId].find(function(m) { return m.url === url; });
    var shouldFetch = !existing || existing.isAd;
    if (!existing) {
      capturedMedia[details.tabId].push(mediaInfo);
      console.log('[DM] M3U8 detected:', url.slice(0, 100));
    } else if (existing.isAd) {
      console.log('[DM] Re-fetching ad manifest (checking for real video):', url.slice(0, 80));
    }
    if (shouldFetch) {
      var captureTabId = details.tabId;
      
      // Fetch and parse manifest
      fetch(url, { headers: headers }).then(function(response) {
        return response.ok ? response.text() : null;
      }).then(function(body) {
        if (!body) return;
        
        var duration = 0;
        var isMaster = body.includes('#EXT-X-STREAM-INF');
        var variantUrl = null;
        
        // Detect if manifest is mostly ads (TikTok CDN etc.)
        var urlLines = body.split('\n').filter(function(l) { return l.startsWith('http'); });
        var adDomains = ['tiktokcdn.com', 'doubleclick.net', 'googlesyndication.com', 'fbcdn.net'];
        var isAd = urlLines.length > 0 && urlLines.filter(function(l) {
          return adDomains.some(function(d) { return l.includes(d); });
        }).length / urlLines.length > 0.5;
        
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
            entry.variantUrl = variantUrl;
            entry.isAd = isAd;
            if (!isAd) {
              console.log('[DM] Real stream ready:', url.slice(0, 80));
            }
          }
        }
        // Notify popup to refresh
        chrome.runtime.sendMessage({ type: 'STREAMS_UPDATED', tabId: captureTabId })
          .catch(function() {}); // popup may not be open
      }).catch(function() {});
    }
  }
}, { urls: ['http://*/*', 'https://*/*'] }, ['requestHeaders']);

chrome.tabs.onRemoved.addListener(function(tabId) {
  delete capturedMedia[tabId];
});

// Clear captured streams when page navigates/reloads
chrome.webNavigation.onCommitted.addListener(function(details) {
  if (details.frameId === 0) {
    delete capturedMedia[details.tabId];
  }
});

// ============================================================
// Message handler - only GET_CAPTURED_MEDIA needed
// ============================================================
chrome.runtime.onMessage.addListener((msg, _, sendResponse) => {
  if (msg.type === 'GET_CAPTURED_MEDIA') {
    if (msg.tabId && capturedMedia[msg.tabId]) {
      sendResponse({ media: capturedMedia[msg.tabId] });
    } else {
      sendResponse({ media: [] });
    }
    return true;
  }
});
