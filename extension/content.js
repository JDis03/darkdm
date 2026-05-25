// ============================================================
// DarkDM — Solo proxy (tú configuras el proxy en Vivaldi)
// ============================================================
(function() {
'use strict';
console.log('[DarkDM] Proxy-only loaded');

var overlay = null, currentVideo = null, hideTimer = null;
var proxyRunning = false;

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
  el.onclick = function(e) { e.stopPropagation(); e.preventDefault(); toggleProxy(); };
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
// PROXY
// ============================================================
function toggleProxy() {
  if (proxyRunning) {
    stopProxy();
  } else {
    startProxy();
  }
}

function startProxy() {
  var pass = prompt('🔒 Contraseña sudo para iptables\n(redirige solo tráfico HTTP del video al proxy):');
  if (!pass) {
    showStatus('❌ Cancelado', 'error');
    setTimeout(hideStatus, 2000);
    return;
  }

  showStatus('🔄 <b>Iniciando proxy + iptables...</b>');
  overlay.textContent = '⏳...';
  overlay.style.borderColor = '#FF9800';
  overlay.style.background = '#e65100';

  chrome.runtime.sendMessage({ type: 'START_PROXY', password: pass, domain: location.hostname }, function(resp) {
    if (resp && resp.success) {
      proxyRunning = true;
      showStatus('🔴 <b>Proxy + iptables activo</b><br><span style="font-size:11px;color:#aaa">Solo tráfico HTTP del sitio capturado</span><br><span style="font-size:10px;color:#aaa">Recarga la página para que fluya por el proxy</span><br><span style="color:#FF6B35;font-size:11px;font-weight:bold">⏹️ Clic para PARAR y guardar</span>');
      overlay.textContent = '⏹️ Parar';
      overlay.style.borderColor = '#f44336';
      overlay.style.background = '#c62828';
    } else {
      showStatus('❌ Error: ' + (resp?.error || 'no se pudo iniciar'), 'error');
      overlay.textContent = '⬇️ DarkDM';
      overlay.style.borderColor = '#FF6B35';
      overlay.style.background = '#1a1a2e';
      setTimeout(hideStatus, 4000);
    }
  });
}

function stopProxy() {
  proxyRunning = false;
  showStatus('📦 <b>Deteniendo proxy...</b><br><span style="font-size:11px;color:#aaa">Concatenando segmentos</span>');
  overlay.textContent = '⬇️ DarkDM';
  overlay.style.borderColor = '#FF6B35';
  overlay.style.background = '#1a1a2e';
  overlay.onclick = function(e) { e.stopPropagation(); e.preventDefault(); toggleProxy(); };

  chrome.runtime.sendMessage({ type: 'STOP_PROXY' }, function(resp) {
    if (resp && resp.success) {
      showStatus('✅ <b>' + resp.segments + ' segmentos capturados</b><br><span style="font-size:11px;color:#aaa">Revisa ~/Descargas/DarkDM/</span>', 'success');
    } else {
      showStatus('⚠️ ' + (resp?.error || 'Error'), 'error');
    }
    setTimeout(hideStatus, 6000);
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
    console.log('[DarkDM] Ready');
  }
}, 2000);

})();
