// ============================================================
// hook.js — Inyectado en MAIN world via registerContentScripts
// NO está en manifest.json → Vivaldi no lo cachea como content script
// ============================================================
(function() {
  if (window.__dmHooked) return;
  window.__dmHooked = true;
  window.__dmData = { manifests: [] };
  console.log('[DM] Hook active');

  function dmEv(type, detail) {
    try { document.dispatchEvent(new CustomEvent('__dm_' + type, { detail: detail || {} })); } catch(e) {}
  }

  // Hook fetch
  var origFetch = window.fetch;
  window.fetch = async function() {
    var url = ((arguments[0] && arguments[0].url) || arguments[0] || '').toString();
    var res = await origFetch.apply(this, arguments);
    var isM3u8 = url.includes('.m3u8');
    if (!isM3u8) {
      try { isM3u8 = (res.headers.get('content-type') || '').includes('mpegurl'); } catch(e) {}
    }
    if (isM3u8) {
      try {
        var clone = res.clone();
        var body = await clone.text();
        if (body.includes('#EXT')) {
          window.__dmData.manifests.push({ url: url, body: body });
          console.log('[DM] Manifest fetch:', url.slice(0, 100));
          dmEv('manifest', { url: url });
        }
      } catch(e) {}
    }
    return res;
  };

  // Hook XHR
  var origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(m, url) {
    this.__dmUrl = (url || '').toString();
    return origOpen.apply(this, arguments);
  };
  var origSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function() {
    var url = this.__dmUrl;
    if (url && url.includes('.m3u8')) {
      this.addEventListener('load', function() {
        try {
          // HLS.js usa responseType='arraybuffer' → responseText falla
          var body = '';
          try { body = this.responseText; } catch(e) {}
          if (!body && this.response && typeof this.response === 'object' && 'byteLength' in this.response) {
            body = new TextDecoder().decode(new Uint8Array(this.response));
          }
          if (body && body.includes('#EXT')) {
            window.__dmData.manifests.push({ url: url, body: body });
            console.log('[DM] Manifest XHR:', url.slice(0, 100));
            dmEv('manifest', { url: url });
          }
        } catch(e) {}
      });
    }
    return origSend.apply(this, arguments);
  };

  dmEv('ready', {});
})();
