// DarkDM — Solo proxy, sin complicaciones
console.log('[DarkDM] Proxy-only mode loaded');

const NH = 'com.darkdm.manager';
let proxyActive = false;

setInterval(() => { chrome.runtime.getPlatformInfo(() => {}); }, 15000);

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
    case 'START_PROXY':
      startProxy(msg.password, msg.domain).then(sendResponse);
      return true;
    case 'STOP_PROXY':
      stopProxy().then(sendResponse);
      return true;
  }
});
