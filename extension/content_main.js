// ============================================================
// DarkDM — MAIN World (hookea fetch/XHR antes que la página)
// Atrapa manifests DESDE EL PRIMER request
// ============================================================
(function() {
'use strict';

const SEEN = new Set();

function notifyManifest(url, body, headers) {
  if (SEEN.has(url)) return;
  SEEN.add(url);

  // Enviar al ISOLATED world via CustomEvent
  document.dispatchEvent(new CustomEvent('__ddm_manifest', {
    detail: { url, body, headers }
  }));
}

// Hook fetch
const origFetch = window.fetch;
window.fetch = async function(...args) {
  const req = args[0];
  const urlStr = (req && req.url ? req.url : req)?.toString() || '';
  const res = await origFetch.apply(this, args);

  // Solo nos interesan manifests
  if (urlStr.match(/\.(m3u8|mpd)(\?|$)/i) || urlStr.includes('.m3u8') || urlStr.includes('.mpd')) {
    try {
      const clone = res.clone();
      const body = await clone.text();
      // Capturar headers del request original
      const headers = {};
      if (req && req.headers) {
        ['referer', 'origin', 'user-agent', 'cookie'].forEach(h => {
          try {
            const v = req.headers.get(h);
            if (v) headers[h] = v;
          } catch(e) {}
        });
      }
      notifyManifest(urlStr, body, headers);
    } catch(e) {}
  }

  return res;
};

// Hook XMLHttpRequest
const origOpen = XMLHttpRequest.prototype.open;
const origSend = XMLHttpRequest.prototype.send;

XMLHttpRequest.prototype.open = function(method, url, ...rest) {
  this.__ddm_url = url?.toString() || '';
  return origOpen.apply(this, [method, url, ...rest]);
};

XMLHttpRequest.prototype.send = function(...args) {
  if (this.__ddm_url && this.__ddm_url.match(/\.(m3u8|mpd)(\?|$)/i)) {
    this.addEventListener('load', () => {
      notifyManifest(this.__ddm_url, this.responseText, {});
    });
  }
  return origSend.apply(this, args);
};

console.log('[DarkDM] MAIN hooks active');
})();
