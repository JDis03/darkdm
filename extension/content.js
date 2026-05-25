// ============================================================
// DarkDM v9 — Proxy mode (proceso independiente, como IDM)
// ============================================================
(function() {
'use strict';
console.log('[DarkDM] v9 loaded');

var overlay = null, currentVideo = null, hideTimer = null;
var capturing = false;

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
  if (capturing) {
    stopCapture();
    return;
  }

  if (video.paused || video.readyState < 2) {
    showStatus('⏳ Reproduce el video primero');
    return;
  }

  // YouTube / sitios sin proteccion: yt-dlp directo
  var hostname = location.hostname.toLowerCase();
  if (hostname.includes('youtube') || hostname.includes('youtu.be') || 
      hostname.includes('vimeo') || hostname.includes('dailymotion')) {
    showStatus('📡 <b>Iniciando descarga...</b><br><span style="font-size:11px;color:#aaa">Usando yt-dlp</span>');
    overlay.textContent = '⏳...';
    overlay.style.borderColor = '#2196F3';
    chrome.runtime.sendMessage({
      type: 'DOWNLOAD_STREAM',
      title: document.title.substring(0, 100),
      url: location.href
    }, function(resp) {
      overlay.textContent = '⬇️ DarkDM';
      overlay.style.borderColor = '#FF6B35';
      if (resp && resp.success) {
        var s = resp.bytes ? ' (' + (resp.bytes/1048576).toFixed(0) + 'MB)' : '';
        showStatus('✅ <b>Descarga completa</b>' + s, 'success');
      } else {
        showStatus('❌ Error: ' + ((resp && (resp.error||resp.msg)) || 'desconocido'), 'error');
      }
      setTimeout(hideStatus, 5000);
    });
    return;
  }

  // Movie sites / sitios con proteccion: proxy
  startCapture();
}

function startCapture() {
  capturing = true;
  
  // Pedir contraseña sudo para iptables
  var sudoPass = prompt('🔒 DarkDM necesita acceso sudo\ningresa tu contraseña para configurar iptables\n(proxy transparente de video):');
  if (!sudoPass) {
    showStatus('❌ Captura cancelada (sin contraseña sudo)', 'error');
    capturing = false;
    setTimeout(hideStatus, 3000);
    return;
  }
  
  // Detectar dominio del video
  var domain = location.hostname;
  // Intentar encontrar el CDN del video (de los elementos video)
  var v = currentVideo || findBestVideo();
  if (v && v.currentSrc) {
    try { domain = new URL(v.currentSrc).hostname; } catch(e) {}
  }
  
  showStatus('🔄 <b>Iniciando proxy + iptables...</b><br><span style="font-size:11px;color:#aaa">Redirigiendo tráfico HTTP de ' + domain + '</span>');
  overlay.textContent = '⏳...';
  overlay.style.borderColor = '#FF9800';
  overlay.style.background = '#e65100';

  chrome.runtime.sendMessage({
    type: 'START_PROXY_CAPTURE',
    password: sudoPass,
    domain: domain
  }, function(resp) {
    if (resp && resp.success) {
      showStatus('🔴 <b>Proxy + iptables activo</b><br><span style="font-size:11px;color:#aaa">Capturando tráfico HTTP de ' + domain + '</span><br><span style="font-size:10px;color:#aaa;display:block;margin-top:2px">Recarga la página para que el tráfico pase por el proxy</span><br><span style="color:#FF6B35;font-size:11px;font-weight:bold">⏹️ Clic para DETENER y guardar</span>');
      overlay.textContent = '⏹️ Parar';
      overlay.style.borderColor = '#f44336';
      overlay.style.background = '#c62828';
    } else {
      showStatus('❌ Error: ' + (resp?.error || 'no se pudo iniciar'), 'error');
      capturing = false;
      overlay.textContent = '⬇️ DarkDM';
      overlay.style.borderColor = '#FF6B35';
      overlay.style.background = '#1a1a2e';
      setTimeout(hideStatus, 5000);
    }
  });
}

function stopCapture() {
  capturing = false;
  showStatus('📦 <b>Deteniendo proxy...</b><br><span style="font-size:11px;color:#aaa">Concatenando segmentos</span>');
  overlay.textContent = '⬇️ DarkDM';
  overlay.style.borderColor = '#FF6B35';
  overlay.style.background = '#1a1a2e';
  overlay.onclick = function(e) { e.stopPropagation(); e.preventDefault(); doCapture(currentVideo); };

  chrome.runtime.sendMessage({ type: 'STOP_PROXY_CAPTURE' }, function(resp) {
    if (resp && resp.success) {
      showStatus('✅ <b>Captura completada</b><br><span style="font-size:11px;color:#aaa">' + resp.segments + ' segmentos capturados. Revisa ~/Descargas/DarkDM/</span>', 'success');
    } else {
      showStatus('⚠️ ' + (resp?.error || 'Error al detener proxy'), 'error');
    }
    setTimeout(hideStatus, 8000);
  });
}

// ============================================================
// Mouse tracking
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

// Auto-init
setInterval(function() {
  var v = findBestVideo();
  if (v && !v.dataset.ddmReady) {
    v.dataset.ddmReady = '1';
    console.log('[DarkDM] Auto-initialized');
  }
}, 2000);

})();
