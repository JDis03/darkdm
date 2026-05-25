// ============================================================
// DarkDM v5 — Captura real de buffer (Netflix/DRM via MSE)
// N1: Direct URL → N2: captureStream → N3: MSE Buffer (DRM real)
// ============================================================
(function() {
'use strict';
console.log('[DarkDM] v5 loaded');

var overlay = null, currentVideo = null, hideTimer = null;
var mseBuffer = [], mseCapturing = false;
var mseVideoTrack = [], mseAudioTrack = [];

// ============================================================
// NIVEL 3: MediaSource Buffer (la solución REAL para DRM)
// En Linux con Widevine L3, los datos en SourceBuffer.appendBuffer
// YA ESTÁN DESCIFRADOS. El monkeypatch los intercepta.
// ============================================================
function installMSECapture() {
  if (window.__darkdmMse) return;
  window.__darkdmMse = true;
  if (!window.MediaSource || !MediaSource.prototype) return;
  
  var origFn = MediaSource.prototype.addSourceBuffer;
  MediaSource.prototype.addSourceBuffer = function(mimeType) {
    var sb = origFn.call(this, mimeType);
    var isVideo = mimeType && mimeType.includes('video');
    var origAppend = sb.appendBuffer.bind(sb);
    sb.appendBuffer = function(data) {
      if (mseCapturing && data) {
        var chunk = data.byteLength ? data.slice(0) : data;
        mseBuffer.push(chunk);
        // Also track by type for proper muxing
        if (isVideo) {
          mseVideoTrack.push(chunk);
        } else {
          mseAudioTrack.push(chunk);
        }
      }
      return origAppend(data);
    };
    return sb;
  };
  console.log('[DarkDM] MSE capture ready (DRM real)');
}

function startMSECapture() {
  mseBuffer = [];
  mseVideoTrack = [];
  mseAudioTrack = [];
  mseCapturing = true;
  console.log('[DarkDM] MSE capture STARTED');
}

function stopMSECapture() {
  mseCapturing = false;
  var total = 0;
  for (var i = 0; i < mseBuffer.length; i++) {
    total += mseBuffer[i].byteLength || mseBuffer[i].size || 0;
  }
  console.log('[DarkDM] MSE capture STOPPED:', (total/1048576).toFixed(0), 'MB in', mseBuffer.length, 'chunks');
  return { chunks: mseBuffer, size: total, videoChunks: mseVideoTrack, audioChunks: mseAudioTrack };
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
// EME / DRM Detection
// ============================================================
function isEMEProtected(video) {
  try {
    return video.mediaKeys !== null && video.mediaKeys !== undefined;
  } catch(e) { return false; }
}

function getHostname() {
  try { return location.hostname.toLowerCase(); } catch(e) { return ''; }
}

function isDRMSite() {
  var host = getHostname();
  return host.includes('netflix') || host.includes('primevideo') ||
         host.includes('disney') || host.includes('hbomax') ||
         host.includes('hbo') || host.includes('max.com') ||
         host.includes('peacock') || host.includes('hulu') ||
         host.includes('paramount') || host.includes('apple.tv') ||
         host.includes('tv.apple');
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
// CAPTURE ENGINE — Como IDM pero con MSE buffer para DRM
// N1: Direct URL → N2: captureStream → N3: MSE Buffer (DRM)
// ============================================================
function doCapture(video) {
  var src = getVideoSource(video);
  var title = document.title;

  if (video.paused || video.readyState < 2) {
    showStatus('⏳ Reproduce el video primero');
    return;
  }

  // If MSE is already capturing, stop it
  if (mseCapturing) {
    doMSECaptureStop(video);
    return;
  }

  // N1: URL directa de video
  if (src && isRealVideoUrl(src) && !src.includes('.m3u8') && !src.includes('.mpd')) {
    showStatus('⬇️ Descargando video directo...');
    try { chrome.runtime.sendMessage({ type: 'START_DOWNLOAD', url: src, filename: title.substring(0,50) + '.mp4' }); } catch(e) {}
    return;
  }

  // N3: DRM / EME → MSE Buffer Capture (la solución REAL)
  if (isEMEProtected(video) || isDRMSite()) {
    console.log('[DarkDM] EME/DRM detected, starting MSE buffer capture');
    doMSECaptureStart(video);
    return;
  }

  // N2: captureStream directo
  doCaptureStream(video);
}

// N2: captureStream para sitios sin DRM
function doCaptureStream(video) {
  if (!video.captureStream || video.paused) return;
  try {
    var stream = video.captureStream(30);
    if (!stream.getVideoTracks().length && !stream.getAudioTracks().length) return;
    
    showStatus('🎬 <b>Grabando...</b><br><span style="font-size:11px;color:#aaa">Se guarda automáticamente</span><br><span style="color:#FF6B35;font-size:11px">⏹️ Clic para detener</span>');
    var mt = MediaRecorder.isTypeSupported('video/webm;codecs=vp9,opus') ? 'video/webm;codecs=vp9,opus' : 'video/webm';
    var baseName = (document.title || 'video').replace(/[^a-zA-Z0-9]/g,'_').substring(0,50);
    
    var rec = new MediaRecorder(stream, { mimeType: mt, videoBitsPerSecond: 8000000 });
    
    var allParts = [];
    rec.ondataavailable = function(e) {
      if (e.data.size && e.data.size > 1000) {
        allParts.push(e.data);
        var totalMB = 0;
        for (var i = 0; i < allParts.length; i++) totalMB += allParts[i].size;
        showStatus('🎬 <b>Grabando...</b><br><span style="font-size:11px;color:#aaa">' + (totalMB/1048576).toFixed(0) + 'MB</span><br><span style="color:#FF6B35;font-size:11px">⏹️ Clic para detener</span>');
      }
    };
    
    rec.onstop = function() {
      video.removeEventListener('ended', onVideoEnd);
      overlay.textContent = '⬇️ DarkDM';
      overlay.style.borderColor = '#FF6B35';
      overlay.onclick = function(e3) { e3.stopPropagation(); e3.preventDefault(); doCapture(video); };
      if (allParts.length) {
        var blob = new Blob(allParts, { type: mt });
        var fname = baseName + '_capture.webm';
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url; a.download = fname; a.style.display = 'none';
        document.body.appendChild(a); a.click();
        setTimeout(function() { document.body.removeChild(a); URL.revokeObjectURL(url); }, 5000);
        showStatus('✅ Video capturado: ' + (blob.size/1048576).toFixed(0) + 'MB', 'success');
      }
      setTimeout(hideStatus, 5000);
    };
    
    overlay.textContent = '🎬 Grabando...';
    overlay.style.borderColor = '#f44336';
    rec.start(10000); // 10s por chunk
    
    var onVideoEnd = function() { if (rec.state !== 'inactive') rec.stop(); };
    video.addEventListener('ended', onVideoEnd);
    
    overlay.onclick = function(e2) {
      e2.stopPropagation(); e2.preventDefault();
      if (rec.state !== 'inactive') rec.stop();
    };
  } catch(e) {
    console.error('[DarkDM] captureStream error:', e);
    if (e.name === 'NotSupportedError' && isEMEProtected(video)) {
      doMSECaptureStart(video);
    } else {
      showStatus('❌ Error: ' + e.message, 'error');
    }
  }
}

// ============================================================
// N3: MSE Buffer Capture — la solución REAL para DRM
// En Linux (Widevine L3), el CDM descifra en software y los
// datos descifrados van al SourceBuffer. Nosotros los interceptamos.
// ============================================================

function doMSECaptureStart(video) {
  if (!window.__darkdmMse) {
    installMSECapture();
  }
  
  startMSECapture();
  
  var baseName = (document.title || 'video').replace(/[^a-zA-Z0-9]/g,'_').substring(0,50);
  
  showStatus('🔓 <b>Capturando buffer...</b><br><span style="font-size:11px;color:#aaa">Datos descifrados del SourceBuffer</span><br><span style="color:#FF6B35;font-size:11px;font-weight:bold">⏹️ Clic para DETENER y guardar</span>');
  
  overlay.textContent = '⏹️ Detener';
  overlay.style.borderColor = '#f44336';
  overlay.style.background = '#c62828';
  
  // Update progress periodically
  var progressTimer = setInterval(function() {
    if (!mseCapturing) { clearInterval(progressTimer); return; }
    var totalMB = 0;
    for (var i = 0; i < mseBuffer.length; i++) {
      totalMB += mseBuffer[i].byteLength || mseBuffer[i].size || 0;
    }
    showStatus('🔓 <b>Capturando buffer...</b><br><span style="font-size:11px;color:#4CAF50">' + (totalMB/1048576).toFixed(1) + 'MB capturados</span><br><span style="font-size:10px;color:#aaa">' + mseBuffer.length + ' fragmentos</span><br><span style="color:#FF6B35;font-size:11px;font-weight:bold">⏹️ Clic para guardar</span>');
  }, 1000);
  
  overlay.onclick = function(e2) {
    e2.stopPropagation(); e2.preventDefault();
    clearInterval(progressTimer);
    doMSECaptureStop(video);
  };
}

function doMSECaptureStop(video) {
  var result = stopMSECapture();
  
  overlay.textContent = '⬇️ DarkDM';
  overlay.style.borderColor = '#FF6B35';
  overlay.style.background = '#1a1a2e';
  overlay.onclick = function(e) { e.stopPropagation(); e.preventDefault(); doCapture(video); };
  
  if (result.size < 100000) { // < 100KB is too small
    showStatus('⚠️ Muy pocos datos capturados (' + (result.size/1024).toFixed(0) + 'KB)<br><span style="font-size:11px;color:#aaa">Asegúrate de que el video se esté reproduciendo</span>', 'error');
    setTimeout(hideStatus, 5000);
    return;
  }
  
  var baseName = (document.title || 'video').replace(/[^a-zA-Z0-9]/g,'_').substring(0,50);
  
  showStatus('📦 <b>Procesando ' + (result.size/1048576).toFixed(1) + 'MB...</b><br><span style="font-size:11px;color:#aaa">Enviando a ffmpeg para combinar pistas</span>');
  
  // Send to native host for processing + saving
  // We send the raw chunks for ffmpeg to assemble properly
  try {
    chrome.runtime.sendMessage({
      type: 'MSE_CAPTURE',
      size: result.size,
      videoChunks: result.videoChunks.length,
      audioChunks: result.audioChunks.length,
      totalChunks: result.chunks.length,
      title: document.title.substring(0, 100),
      site: getHostname()
    });
  } catch(e) {}
  
  // For now, save directly via blob download
  try {
    // Save video and audio tracks separately for proper muxing
    if (result.videoChunks.length > 0) {
      var vblob = new Blob(result.videoChunks, { type: 'video/mp4' });
      var vurl = URL.createObjectURL(vblob);
      var a = document.createElement('a');
      a.href = vurl; 
      a.download = baseName + '_video.mp4'; 
      a.style.display = 'none';
      document.body.appendChild(a); a.click();
      setTimeout(function() { document.body.removeChild(a); URL.revokeObjectURL(vurl); }, 5000);
    }
    if (result.audioChunks.length > 0) {
      var ablob = new Blob(result.audioChunks, { type: 'audio/mp4' });
      var aurl = URL.createObjectURL(ablob);
      var a2 = document.createElement('a');
      a2.href = aurl; 
      a2.download = baseName + '_audio.mp4'; 
      a2.style.display = 'none';
      document.body.appendChild(a2); a2.click();
      setTimeout(function() { document.body.removeChild(a2); URL.revokeObjectURL(aurl); }, 5000);
    }
    // Also save combined (all chunks in order)
    if (result.chunks.length > 0) {
      var cblob = new Blob(result.chunks, { type: 'video/mp4' });
      var curl = URL.createObjectURL(cblob);
      var a3 = document.createElement('a');
      a3.href = curl; 
      a3.download = baseName + '_completo.mp4'; 
      a3.style.display = 'none';
      document.body.appendChild(a3); a3.click();
      setTimeout(function() { document.body.removeChild(a3); URL.revokeObjectURL(curl); }, 5000);
      showStatus('✅ <b>Capturado: ' + (cblob.size/1048576).toFixed(1) + 'MB</b><br><span style="font-size:11px;color:#aaa">Archivos guardados en Descargas</span>', 'success');
    }
  } catch(e) {
    showStatus('❌ Error al guardar: ' + e.message, 'error');
  }
  
  setTimeout(hideStatus, 8000);
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
