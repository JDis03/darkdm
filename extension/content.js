// ============================================================
// DarkDM v6 — Captura de manifest + yt-dlp (como IDM)
// ============================================================
(function() {
'use strict';
console.log('[DarkDM] v6 loaded');

var overlay = null, currentVideo = null, hideTimer = null;
var downloading = false;

function findAllVideos(root) {
  root = root || document;
  var vids = [];
  try { vids = Array.from(root.querySelectorAll('video')); } catch(e) {}
  try {
    root.querySelectorAll('*').forEach(function(el) {
      if (el.shadowRoot) vids = vids.concat(findAllVideos(el.shadowRoot));
    });
  } catch(e) {}
  return vids;
}

function findBestVideo() {
  var vids = findAllVideos();
  if (!vids.length) return null;
  var best = null, bestArea = 0;
  vids.forEach(function(v) {
    try {
      var r = v.getBoundingClientRect();
      var a = r.width * r.height;
      if (a > 5000 && a > bestArea) { best = v; bestArea = a; }
    } catch(e) {}
  });
  return best || vids[0];
}

function getVideoSource(video) {
  var src = video.currentSrc || video.src || '';
  if (src.startsWith('blob:')) src = '';
  if (!src) {
    video.querySelectorAll('source').forEach(function(s) {
      if (s.src && !s.src.startsWith('blob:')) src = s.src;
    });
  }
  return src;
}

function createOverlay(v) {
  removeOverlay();
  var el = document.createElement('div');
  el.id = 'ddm-overlay';
  el.textContent = '⬇️ DarkDM';
  el.style.cssText = 'position:fixed;z-index:99999999;display:flex;align-items:center;gap:5px;padding:5px 10px;background:#1a1a2e;color:#fff;border:1px solid #FF6B35;border-radius:6px;font:600 11px sans-serif;cursor:pointer;box-shadow:0 2px 8px rgba(0,0,0,0.5);pointer-events:auto';
  el.onclick = function(e) { e.stopPropagation(); e.preventDefault(); doCapture(v); };
  document.body.appendChild(el);
  overlay = el; currentVideo = v;
  positionOverlay(v);
}

function positionOverlay(v) {
  if (!overlay) return;
  try {
    var r = v.getBoundingClientRect();
    overlay.style.display = 'flex';
    overlay.style.left = Math.max(0, r.left + r.width - 130) + 'px';
    overlay.style.top = Math.max(0, r.top + 8) + 'px';
  } catch(e) { overlay.style.display = 'none'; }
}

function removeOverlay() {
  if (overlay) { overlay.remove(); overlay = null; }
  clearTimeout(hideTimer);
  currentVideo = null;
}

var panel = null;
function showStatus(msg, type) {
  if (!panel) {
    panel = document.createElement('div');
    panel.id = 'ddm-panel';
    panel.style.cssText = 'position:fixed;bottom:20px;right:20px;z-index:99999999;min-width:280px;max-width:400px;padding:12px 16px;background:#1a1a2e;color:#fff;border:2px solid #FF6B35;border-radius:10px;font:13px sans-serif;box-shadow:0 4px 20px rgba(0,0,0,0.6)';
    document.body.appendChild(panel);
  }
  panel.innerHTML = msg;
  panel.style.display = 'block';
  panel.style.borderColor = type === 'success' ? '#4CAF50' : type === 'error' ? '#f44336' : '#FF6B35';
}
function hideStatus() { if (panel) panel.style.display = 'none'; }

// ============================================================
// CAPTURE
// ============================================================
function doCapture(video) {
  var src = getVideoSource(video);
  var title = document.title;

  if (video.paused || video.readyState < 2) {
    showStatus('⏳ Reproduce el video primero');
    return;
  }

  if (downloading) {
    showStatus('⏳ Ya hay una descarga en curso');
    return;
  }

  // URL directa
  if (src && !src.startsWith('blob:') && !src.includes('.m3u8') && !src.includes('.mpd')) {
    showStatus('⬇️ Descargando video directo...');
    try { chrome.runtime.sendMessage({ type: 'START_DOWNLOAD', url: src, filename: title.substring(0,50) + '.mp4' }); } catch(e) {}
    return;
  }

  // Descargar via manifest + yt-dlp (como IDM)
  showStatus('📡 <b>Iniciando descarga...</b><br><span style="font-size:11px;color:#aaa">Detectando manifest .m3u8</span>');
  overlay.textContent = '⏳...';
  overlay.style.borderColor = '#2196F3';
  downloading = true;

  chrome.runtime.sendMessage({
    type: 'DOWNLOAD_STREAM',
    title: title.substring(0, 100),
    url: location.href
  }, function(resp) {
    downloading = false;
    overlay.textContent = '⬇️ DarkDM';
    overlay.style.borderColor = '#FF6B35';
    overlay.onclick = function(e) { e.stopPropagation(); e.preventDefault(); doCapture(video); };

    if (!resp) {
      showStatus('❌ No hay respuesta del background', 'error');
      setTimeout(hideStatus, 4000);
      return;
    }

    if (resp.success) {
      var size = resp.bytes ? ' (' + (resp.bytes / 1048576).toFixed(0) + 'MB)' : '';
      showStatus('✅ <b>Descarga completada</b>' + size + '<br><span style="font-size:11px;color:#aaa">' + (resp.message || 'Archivo en ~/Descargas/DarkDM/') + '</span>', 'success');
      setTimeout(hideStatus, 6000);
    } else {
      showStatus('❌ <b>Error:</b> ' + (resp.error || resp.msg || 'Desconocido'), 'error');
      setTimeout(hideStatus, 6000);
    }
  });
}

// Mouse tracking
var scanTimer = null;
document.addEventListener('mousemove', function(e) {
  clearTimeout(scanTimer);
  scanTimer = setTimeout(function() {
    var v = findBestVideo();
    if (!v) { removeOverlay(); return; }
    try {
      var r = v.getBoundingClientRect();
      var over = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      if (over) {
        if (!overlay) createOverlay(v); else positionOverlay(v);
        overlay.style.display = 'flex';
        clearTimeout(hideTimer);
      } else if (overlay) {
        clearTimeout(hideTimer);
        hideTimer = setTimeout(function() { if (overlay) overlay.style.display = 'none'; }, 2000);
      }
    } catch(err) {}
  }, 150);
});

// Auto-attach debugger
setInterval(function() {
  var v = findBestVideo();
  if (v && !v.dataset.ddmReady) {
    v.dataset.ddmReady = '1';
    try { chrome.runtime.sendMessage({ type: 'ATTACH_DEBUGGER' }); } catch(e) {}
    console.log('[DarkDM] Auto-initialized');
  }
}, 2000);

})();
