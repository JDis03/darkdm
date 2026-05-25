// ============================================================
// DarkDM v3 — 3 niveles como IDM
// N1: Network → N2: Native → N3: MediaSource Buffer
// ============================================================
(function() {
'use strict';
console.log('[DarkDM] v3 loaded');

var overlay = null, currentVideo = null, hideTimer = null;
var mseBuffer = [], mseCapturing = false;

// ============================================================
// NIVEL 3: MediaSource Buffer (como IDM)
// ============================================================
function installMSECapture() {
  if (window.__darkdmMse) return;
  window.__darkdmMse = true;
  if (!window.MediaSource || !MediaSource.prototype) return;
  
  var origFn = MediaSource.prototype.addSourceBuffer;
  MediaSource.prototype.addSourceBuffer = function(mimeType) {
    var sb = origFn.call(this, mimeType);
    var origAppend = sb.appendBuffer.bind(sb);
    sb.appendBuffer = function(data) {
      if (mseCapturing && data) {
        mseBuffer.push(data.byteLength ? data.slice(0) : data);
      }
      return origAppend(data);
    };
    return sb;
  };
  console.log('[DarkDM] MSE capture ready');
}

function startMSECapture() {
  mseBuffer = [];
  mseCapturing = true;
}
function stopMSECapture() {
  mseCapturing = false;
  var total = 0;
  for (var i = 0; i < mseBuffer.length; i++) {
    total += mseBuffer[i].byteLength || mseBuffer[i].size || 0;
  }
  return { chunks: mseBuffer, size: total };
}

// ============================================================
// Find videos
// ============================================================
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

function isRealVideoUrl(url) {
  if (!url || url.length < 10) return false;
  if (url.match(/\.(txt|php|html?|json|xml)(\?|$)/i)) return false;
  return true;
}

// ============================================================
// OVERLAY
// ============================================================
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

// ============================================================
// STATUS PANEL
// ============================================================
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
// CAPTURE ENGINE — 3 niveles como IDM
// ============================================================
function doCapture(video) {
  var src = getVideoSource(video);
  var url = location.href;
  var title = document.title;

  if (video.paused || video.readyState < 2) {
    showStatus('⏳ Reproduce el video primero');
    return;
  }

  // N1: URL directa de video
  if (src && isRealVideoUrl(src) && !src.includes('.m3u8') && !src.includes('.mpd')) {
    showStatus('⬇️ Descargando video directo...');
    try { chrome.runtime.sendMessage({ type: 'START_DOWNLOAD', url: src, filename: title.substring(0,50) + '.mp4' }); } catch(e) {}
    return;
  }

  // captureStream directo con guardado automático cada 30s
  doCaptureStream(video);
}

// Fallback: captureStream con guardado automático cada 30s
function doCaptureStream(video) {
  if (!video.captureStream || video.paused) return;
  try {
    var stream = video.captureStream(30);
    if (!stream.getVideoTracks().length && !stream.getAudioTracks().length) return;
    
    showStatus('🎬 <b>Grabando...</b><br><span style="font-size:11px;color:#aaa">Se guarda automáticamente cada 30s</span><br><span style="color:#FF6B35;font-size:11px">⏹️ Clic para detener</span>');
    var mt = MediaRecorder.isTypeSupported('video/webm;codecs=vp9,opus') ? 'video/webm;codecs=vp9,opus' : 'video/webm';
    var baseName = (document.title || 'video').replace(/[^a-zA-Z0-9]/g,'_').substring(0,50);
    var chunkNum = 1;
    
    var rec = new MediaRecorder(stream, { mimeType: mt, videoBitsPerSecond: 8000000 });
    
    var allParts = [];
    rec.ondataavailable = function(e) {
      if (e.data.size && e.data.size > 1000) {
        allParts.push(e.data);
        var totalMB = 0;
        for (var i = 0; i < allParts.length; i++) totalMB += allParts[i].size;
        showStatus('🎬 <b>Grabando...</b><br><span style="font-size:11px;color:#aaa">' + (totalMB/1048576).toFixed(0) + 'MB acumulados</span><br><span style="color:#FF6B35;font-size:11px">⏹️ Clic para detener y guardar</span>');
      }
    };
    
    rec.onstop = function() {
      video.removeEventListener('ended', onVideoEnd);
      overlay.textContent = '⬇️ DarkDM';
      overlay.style.borderColor = '#FF6B35';
      overlay.onclick = function(e3) { e3.stopPropagation(); e3.preventDefault(); doCapture(video); };
      if (allParts.length) {
        var blob = new Blob(allParts, { type: mt });
        var fname = baseName + '_completa.webm';
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url; a.download = fname; a.style.display = 'none';
        document.body.appendChild(a); a.click();
        setTimeout(function() { document.body.removeChild(a); URL.revokeObjectURL(url); }, 5000);
        showStatus('✅ Película capturada: ' + (blob.size/1048576).toFixed(0) + 'MB', 'success');
      }
      setTimeout(hideStatus, 5000);
    };
    
    overlay.textContent = '🎬 Grabando...';
    overlay.style.borderColor = '#f44336';
    rec.start(30000); // 30s por chunk
    
    // Auto-detener cuando el video termine
    var onVideoEnd = function() { if (rec.state !== 'inactive') rec.stop(); };
    video.addEventListener('ended', onVideoEnd);
    
    overlay.onclick = function(e2) {
      e2.stopPropagation(); e2.preventDefault();
      if (rec.state !== 'inactive') rec.stop();
    };
  } catch(e) { console.error('[DarkDM] captureStream error:', e); }
}

// ============================================================
// MOUSE TRACKING
// ============================================================
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

// Auto-instalar MSE capture + debugger
setInterval(function() {
  var v = findBestVideo();
  if (v && !v.dataset.ddmReady) {
    v.dataset.ddmReady = '1';
    try { chrome.runtime.sendMessage({ type: 'ATTACH_DEBUGGER' }); } catch(e) {}
    installMSECapture();
    console.log('[DarkDM] Auto-initialized');
  }
}, 2000);
})();
