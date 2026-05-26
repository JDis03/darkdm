# DarkDM — Captura de Segmentos `.ts` vía TLS en Vivaldi/Chromium

> **Contexto:** Vivaldi usa Chromium, y Chromium compila BoringSSL **estáticamente** dentro
> del binario principal. Eso cambia radicalmente las opciones disponibles.

---

## 1. El problema raíz con LD_PRELOAD en Vivaldi

### Por qué LD_PRELOAD **no funciona** para BoringSSL en Chromium

LD_PRELOAD solo intercepta símbolos resueltos en tiempo de enlace **dinámico**. Si corres:

```bash
ldd /opt/vivaldi/vivaldi-bin | grep -i ssl
# → (vacío o sólo libssl del sistema, que Vivaldi no usa para HTTPS)
```

```bash
nm -D /opt/vivaldi/vivaldi-bin | grep SSL_read
# → (vacío — símbolo no exportado, es interno/estático)
```

El resultado esperado es silencio: BoringSSL está compilado dentro del binario como código
estático. `SSL_read` no es un símbolo dinámico exportado, por lo que el linker dinámico
nunca lo resuelve y LD_PRELOAD no tiene nada que interceptar.

**LD_PRELOAD sí funciona** para aplicaciones que usen OpenSSL como `.so` (Firefox con
`libssl.so`, curl, wget, etc.), pero **no** para Chromium/Vivaldi/Electron.

---

## 2. La opción más simple: `SSLKEYLOGFILE` (built-in en Chromium)

Chromium tiene soporte nativo para exportar claves de sesión TLS, exactamente para este
propósito (análisis con Wireshark).

### Configuración

```bash
# Lanzar Vivaldi exportando las claves de sesión
SSLKEYLOGFILE=/tmp/vivaldi_keys.log /opt/vivaldi/vivaldi
```

### Captura simultánea del tráfico

```bash
# En otra terminal, capturar tráfico de red
sudo tcpdump -i any -w /tmp/capture.pcap 'tcp port 443'
```

### Descifrar y extraer los .ts

```bash
# Opción A: con tshark (línea de comandos de Wireshark)
tshark -r /tmp/capture.pcap \
       -o "tls.keylog_file:/tmp/vivaldi_keys.log" \
       -Y 'http2 and (http2.header.value contains ".ts" or http2.data.len > 100000)' \
       -T fields -e http2.data.data > segments_hex.txt

# Opción B: exportar objetos HTTP desde Wireshark GUI
# Edit → Preferences → TLS → (Pre)-Master-Secret log filename → vivaldi_keys.log
# File → Export Objects → HTTP
```

### Script Python para automatizar la extracción

```python
#!/usr/bin/env python3
"""
Extrae segmentos .ts descifrados usando SSLKEYLOGFILE + scapy/pyshark
"""
import pyshark
import os

PCAP = "/tmp/capture.pcap"
KEYLOG = "/tmp/vivaldi_keys.log"
OUT_DIR = "/tmp/ts_segments"

os.makedirs(OUT_DIR, exist_ok=True)

cap = pyshark.FileCapture(
    PCAP,
    override_prefs={"tls.keylog_file": KEYLOG},
    display_filter="http2"
)

for i, pkt in enumerate(cap):
    try:
        # Filtrar por URL que contenga .ts o por tamaño > 100KB
        if hasattr(pkt, 'http2'):
            data = bytes.fromhex(pkt.http2.data_data.replace(':', ''))
            if len(data) > 50_000:  # segmentos típicos > 50KB
                fname = os.path.join(OUT_DIR, f"segment_{i:04d}.ts")
                with open(fname, 'wb') as f:
                    f.write(data)
                print(f"[+] {fname} ({len(data)/1024:.1f} KB)")
    except AttributeError:
        pass
```

**Ventaja principal:** el CDN no ve absolutamente nada diferente — la conexión TLS es
100% nativa de Vivaldi. No hay MITM, no hay cambio de fingerprint.

---

## 3. Alternativa robusta: Frida (en lugar de LD_PRELOAD)

Frida puede hookear símbolos **dentro** del binario estático, sin necesidad de exportarlos.
Es el equivalente moderno a LD_PRELOAD para binarios sin símbolos dinámicos.

### Instalación

```bash
pip install frida-tools
```

### Encontrar los offsets de SSL_read / SSL_write en Vivaldi

```bash
# Buscar el símbolo dentro del binario (puede estar strip'd o con debug info)
nm /opt/vivaldi/vivaldi-bin 2>/dev/null | grep -i 'ssl_read\|bio_read\|ssl_write'

# Si está strip'd, buscar con patrones de bytes (más avanzado)
# O usar el script de Frida directamente con búsqueda de módulo
```

### Script Frida para capturar datos TLS descifrados

```javascript
// ssl_capture.js — hookea SSL_read en el proceso de Vivaldi
// Uso: frida -n vivaldi-bin -l ssl_capture.js

"use strict";

// Busca el módulo principal (el binario de Vivaldi)
const mainModule = Process.enumerateModules()[0];
console.log(`[*] Módulo principal: ${mainModule.name} @ ${mainModule.base}`);

// Intenta localizar SSL_read por nombre de símbolo
// (funciona si el binario tiene tabla de símbolos, aunque sea parcial)
let sslReadAddr = null;
try {
    sslReadAddr = Module.findExportByName(null, "SSL_read");
} catch(e) {}

if (!sslReadAddr) {
    // Fallback: buscarlo en el binario de Vivaldi por patrón
    // Requiere análisis previo con Ghidra/IDA para obtener el offset
    // Ejemplo: sslReadAddr = mainModule.base.add(0xABCD1234);
    console.log("[-] SSL_read no encontrado como símbolo exportado.");
    console.log("    Usa Ghidra/IDA para obtener el offset y ajusta el script.");
} else {
    console.log(`[+] SSL_read @ ${sslReadAddr}`);

    Interceptor.attach(sslReadAddr, {
        onEnter(args) {
            // args[0] = SSL*, args[1] = buffer, args[2] = num
            this.buf = args[1];
            this.ssl = args[0];
        },
        onLeave(retval) {
            const bytesRead = retval.toInt32();
            if (bytesRead > 0) {
                const data = this.buf.readByteArray(bytesRead);
                const bytes = new Uint8Array(data);

                // Detectar segmentos MPEG-TS: magic bytes 0x47 (sync byte) cada 188 bytes
                if (bytes[0] === 0x47 && (bytesRead < 188 || bytes[188] === 0x47)) {
                    console.log(`[TS] ${bytesRead} bytes de segmento .ts`);
                    send({ type: "ts_data", size: bytesRead }, data);
                }

                // Detectar respuestas HTTP/2 con .ts en la URL (revisar headers)
                // Los headers HTTP/2 vienen en frames separados (HEADERS frame)
                // Los datos en DATA frames — detectar por tamaño + contexto
            }
        }
    });
}
```

### Receptor Python para guardar los segmentos

```python
#!/usr/bin/env python3
import frida, sys, os

OUT_DIR = "/tmp/ts_frida"
os.makedirs(OUT_DIR, exist_ok=True)
counter = [0]

def on_message(message, data):
    if message.get("type") == "send":
        payload = message.get("payload", {})
        if payload.get("type") == "ts_data" and data:
            fname = os.path.join(OUT_DIR, f"segment_{counter[0]:04d}.ts")
            with open(fname, "wb") as f:
                f.write(data)
            print(f"[+] {fname} ({len(data)/1024:.1f} KB)")
            counter[0] += 1

session = frida.attach("vivaldi-bin")
with open("ssl_capture.js") as f:
    script = session.create_script(f.read())
script.on("message", on_message)
script.load()
sys.stdin.read()
```

---

## 4. eBPF + kTLS (el enfoque más estable a largo plazo)

kTLS delega el cifrado/descifrado TLS al kernel. Cuando está activo, los datos viajan
**en claro** por los sockets a nivel kernel y pueden capturarse con eBPF.

### Verificar si Vivaldi usa kTLS

```bash
# Monitorear si el socket activa TLS_TX / TLS_RX en el kernel
sudo bpftrace -e '
  kprobe:tls_sw_sendmsg { printf("kTLS TX: pid=%d\n", pid); }
  kprobe:tls_sw_recvmsg { printf("kTLS RX: pid=%d\n", pid); }
' &
# Luego abrir Vivaldi y navegar a un sitio HTTPS
```

> **Nota:** Chromium/Vivaldi en Linux generalmente **no activa kTLS** por defecto.
> kTLS es más común en servidores (nginx, curl con `--tls-earlydata`) que en clientes.
> Si Vivaldi no usa kTLS, este vector no aplica.

### Script eBPF para captura si kTLS está activo

```python
#!/usr/bin/env python3
"""
Captura datos TLS descifrados vía kTLS con BCC (si el proceso usa kTLS)
Requiere: pip install bcc, kernel >= 4.13 con CONFIG_TLS=m
"""
from bcc import BPF
import os, ctypes

program = r"""
#include <uapi/linux/ptrace.h>
#include <net/sock.h>

BPF_PERF_OUTPUT(tls_data);

struct data_t {
    u32 pid;
    u32 len;
    char buf[4096];
};

// Hookear tls_sw_recvmsg — datos ya descifrados
int kprobe__tls_sw_recvmsg(struct pt_regs *ctx, struct sock *sk,
                            struct msghdr *msg, size_t len) {
    u32 pid = bpf_get_current_pid_tgid() >> 32;
    // Filtrar por PID de Vivaldi
    if (pid != VIVALDI_PID) return 0;

    struct data_t data = {};
    data.pid = pid;
    data.len = len;
    // Nota: leer msg->msg_iov requiere helpers adicionales
    tls_data.perf_submit(ctx, &data, sizeof(data));
    return 0;
}
"""

vivaldi_pid = int(input("PID de vivaldi-bin: "))
program = program.replace("VIVALDI_PID", str(vivaldi_pid))
b = BPF(text=program)

def print_event(cpu, data, size):
    event = b["tls_data"].event(data)
    print(f"[kTLS] pid={event.pid} len={event.len}")

b["tls_data"].open_perf_buffer(print_event)
print("Escuchando datos kTLS...")
while True:
    b.perf_buffer_poll()
```

---

## 5. Distinguir tráfico de video vs otros datos

Independientemente del método de captura, necesitas filtrar los segmentos `.ts` del
resto del tráfico HTTPS.

### Señales para identificar segmentos .ts

| Señal | Descripción |
|-------|-------------|
| **Magic bytes** | Segmentos MPEG-TS empiezan con `0x47` (sync byte), repetido cada 188 bytes |
| **Tamaño** | Los segmentos típicos miden 500 KB – 5 MB; los frames JS/CSS suelen ser < 100 KB |
| **URL pattern** | Contienen `.ts`, `/seg`, `segment`, `chunk`, `frag`, números de secuencia |
| **Content-Type** | `video/mp2t`, `application/octet-stream` para .ts |
| **Host/SNI** | Los CDNs de video usan dominios como `*.akamaized.net`, `*.llnwd.net`, `*.fastly.net` |

### Función de detección en Python

```python
def is_ts_segment(data: bytes) -> bool:
    """Detecta si los datos son un segmento MPEG-TS"""
    if len(data) < 188:
        return False
    # El sync byte 0x47 debe aparecer en posiciones 0, 188, 376...
    return (data[0] == 0x47 and 
            (len(data) < 376 or data[188] == 0x47) and
            (len(data) < 564 or data[376] == 0x47))

def is_likely_video(data: bytes, threshold_kb: int = 200) -> bool:
    return len(data) > threshold_kb * 1024 or is_ts_segment(data)
```

---

## 6. Herramientas open source relevantes

| Herramienta | Descripción | Aplica aquí |
|-------------|-------------|-------------|
| [**ssl_logger**](https://github.com/google/ssl_logger) | Frida script de Google para loggear SSL_read/write | Sí, con ajuste para filtrar .ts |
| [**frida-scripts**](https://github.com/interference-security/frida-scripts) | Colección de scripts Frida para hooking TLS | Sí |
| [**sslsplit**](https://github.com/droe/sslsplit) | MITM TLS transparente a nivel sistema | Sí, similar a mitmproxy |
| [**bettercap**](https://github.com/bettercap/bettercap) | Framework MITM con módulos TLS | Parcial |
| [**yt-dlp**](https://github.com/yt-dlp/yt-dlp) | Extractor de streams HLS/DASH directamente | **Mejor opción inicial** |
| [**N_m3u8DL-RE**](https://github.com/nilaoda/N_m3u8DL-RE) | Descargador de HLS multiplataforma | Sí |
| [**streamlink**](https://github.com/streamlink/streamlink) | Extracción de streams a stdout | Sí |

---

## 7. Comparación de enfoques

```
┌─────────────────────┬──────────────┬──────────────┬──────────────┬──────────────┐
│ Método              │ Detección CDN│ Complejidad  │ Estabilidad  │ Recomendado  │
├─────────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
│ SSLKEYLOGFILE       │ Ninguna ✅   │ Baja ✅      │ Alta ✅      │ ⭐ Sí        │
│ Frida               │ Ninguna ✅   │ Media        │ Media*       │ ✅ Sí        │
│ LD_PRELOAD          │ Ninguna      │ Alta         │ Baja ❌      │ ❌ No aplica │
│ eBPF + kTLS         │ Ninguna ✅   │ Alta ❌      │ Alta ✅      │ Solo si kTLS │
│ mitmproxy           │ TLS fp ⚠️   │ Baja         │ Alta         │ Ya lo usas   │
└─────────────────────┴──────────────┴──────────────┴──────────────┴──────────────┘
* Frida puede romperse con actualizaciones de Vivaldi si usa offsets directos
```

---

## 8. Flujo recomendado para DarkDM

```
Usuario solicita URL de película
         │
         ▼
1. Intentar yt-dlp / N_m3u8DL-RE directamente
   (extraen el m3u8 sin necesidad de browser)
         │ falla (DRM, protección JS)
         ▼
2. Lanzar Vivaldi con SSLKEYLOGFILE=/tmp/keys.log
   + tcpdump en background
         │
         ▼
3. Browser navega a la URL (headless con Playwright si quieres automatizar)
         │
         ▼
4. Parser de PCAP extrae DATA frames HTTP/2 > 100KB
   + filtra por magic bytes 0x47 (MPEG-TS)
         │
         ▼
5. Reconstruye secuencia de segmentos → muxea con ffmpeg
```

```bash
# Ejemplo del paso 5
ffmpeg -i "concat:$(ls /tmp/ts_segments/*.ts | tr '\n' '|')" \
       -c copy output.mp4
```

---

## 9. Símbolos BoringSSL en Vivaldi (referencia)

Si decides usar Frida con búsqueda de símbolos:

```bash
# Ver si el binario tiene tabla de símbolos (debug build o parcialmente strip'd)
nm /opt/vivaldi/vivaldi-bin 2>/dev/null | grep -i 'ssl\|tls\|bio' | head -30

# Con objdump
objdump -t /opt/vivaldi/vivaldi-bin 2>/dev/null | grep -i ssl_read

# Buscar con strings (a veces los nombres quedan en el binario)
strings /opt/vivaldi/vivaldi-bin | grep -i 'SSL_read\|SSL_write\|BIO_read'

# En Chromium/Vivaldi las funciones relevantes son:
# - SSL_read         → lectura de datos descifrados
# - SSL_write        → escritura antes del cifrado
# - BIO_read         → nivel inferior (normalmente no necesario)
# - SSL_do_handshake → para identificar inicio de sesión
```

> En builds de producción de Vivaldi, estos símbolos probablemente están strip'd.
> En ese caso, usar Ghidra + FLIRT signatures de BoringSSL para identificar las funciones
> por sus patrones de bytes.

---

## Resumen ejecutivo

1. **Empieza con `SSLKEYLOGFILE`** — es la opción menos invasiva, no requiere compilar
   nada, y el CDN no ve nada anormal.

2. **LD_PRELOAD no aplica** para Vivaldi porque BoringSSL es estático — no pierdas tiempo
   en ese camino.

3. **Frida es el LD_PRELOAD moderno** para este caso: puede hookear funciones estáticas
   dentro del binario.

4. **eBPF + kTLS** es elegante pero Chromium probablemente no activa kTLS como cliente;
   verificar primero.

5. **Para muchos sitios de películas**, yt-dlp o N_m3u8DL-RE ya funcionan sin necesitar
   nada de esto — pruébalos primero contra el sitio objetivo.
