// ============================================================
// DarkDM Video Stream Downloader
// Descarga y ensambla streams HLS (m3u8) y DASH (mpd)
// ============================================================

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Información de un stream detectado
#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub url: String,
    pub stream_type: StreamType,
    pub page_url: String,
    pub page_title: String,
    pub quality: Option<String>,
    pub resolution: Option<(u32, u32)>,
    pub bandwidth: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamType {
    HLS,       // .m3u8
    DASH,      // .mpd
    Direct,    // .mp4, .webm, etc
}

impl StreamType {
    pub fn from_url(url: &str) -> Self {
        let lower = url.to_lowercase();
        if lower.contains(".m3u8") {
            StreamType::HLS
        } else if lower.contains(".mpd") {
            StreamType::DASH
        } else {
            StreamType::Direct
        }
    }
}

/// Resultado de una descarga
#[derive(Debug)]
pub struct DownloadResult {
    pub output_path: PathBuf,
    pub total_bytes: u64,
    pub segments_downloaded: usize,
    pub duration_secs: f64,
    pub success: bool,
    pub error: Option<String>,
}

// ============================================================
// DESCARGA HLS (.m3u8)
// ============================================================

pub fn download_hls(
    manifest_url: &str,
    output_dir: &Path,
    filename: Option<&str>,
    cancel_flag: Option<Arc<AtomicBool>>,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> DownloadResult {
    let start = Instant::now();
    let name = filename.unwrap_or("darkdm_hls_video");
    let output_path = output_dir.join(format!("{}.ts", name));
    let temp_dir = output_dir.join(format!("{}_segments", name));

    fs::create_dir_all(&temp_dir).unwrap_or_default();
    fs::create_dir_all(output_dir).unwrap_or_default();

    // 1. Fetch manifest
    let manifest_content = match fetch_url(&manifest_url) {
        Ok(c) => c,
        Err(e) => return DownloadResult {
            output_path, total_bytes: 0, segments_downloaded: 0,
            duration_secs: start.elapsed().as_secs_f64(),
            success: false, error: Some(format!("Failed to fetch manifest: {}", e)),
        },
    };

    let manifest_str = String::from_utf8_lossy(&manifest_content);

    // 2. Parse segments from m3u8
    let base_url = get_base_url(manifest_url);
    let segments = parse_hls_playlist(&manifest_str, &base_url);
    
    if segments.is_empty() {
        // Maybe it's a variant playlist - try to find the highest quality
        let variant_url = find_best_variant(&manifest_str, &base_url);
        if let Some(v_url) = variant_url {
            return download_hls(&v_url, output_dir, filename, cancel_flag, progress);
        }
        return DownloadResult {
            output_path, total_bytes: 0, segments_downloaded: 0,
            duration_secs: start.elapsed().as_secs_f64(),
            success: false, error: Some("No segments found in HLS manifest".to_string()),
        };
    }

    eprintln!("[DarkDM HLS] Found {} segments", segments.len());

    // 3. Download segments in parallel
    let total = segments.len();
    let downloaded = Arc::new(AtomicUsize::new(0));
    let bytes_downloaded = Arc::new(AtomicUsize::new(0));
    let success_flag = Arc::new(AtomicBool::new(true));
    let cancel = cancel_flag.unwrap_or(Arc::new(AtomicBool::new(false)));

    let mut handles = vec![];

    for (i, segment_url) in segments.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) { break; }

        let url = segment_url.clone();
        let tdir = temp_dir.clone();
        let dl = downloaded.clone();
        let bytes = bytes_downloaded.clone();
        let success = success_flag.clone();
        let cancel = cancel.clone();

        let handle = thread::spawn(move || {
            if cancel.load(Ordering::Relaxed) { return; }
            
            let seg_path = tdir.join(format!("seg_{:05}.ts", i));
            
            match fetch_url(&url) {
                Ok(data) => {
                    if let Ok(mut f) = fs::File::create(&seg_path) {
                        let _ = f.write_all(&data);
                    }
                    dl.fetch_add(1, Ordering::Relaxed);
                    bytes.fetch_add(data.len(), Ordering::Relaxed);
                }
                Err(e) => {
                    eprintln!("[DarkDM HLS] Segment {} failed: {}", i, e);
                    success.store(false, Ordering::Relaxed);
                }
            }
        });

        handles.push(handle);

        // Limit concurrent downloads
        if handles.len() >= 8 {
            let _ = handles.remove(0).join();
        }
    }

    // Wait for all downloads
    for h in handles {
        let _ = h.join();
    }

    let total_bytes = bytes_downloaded.load(Ordering::Relaxed);
    let segs_ok = downloaded.load(Ordering::Relaxed);

    // 4. Assemble segments into final file
    let mut output_file = fs::File::create(&output_path).unwrap_or_else(|_| panic!("Cannot create {}", output_path.display()));
    let mut written: u64 = 0;

    for i in 0..total {
        let seg_path = temp_dir.join(format!("seg_{:05}.ts", i));
        if seg_path.exists() {
            if let Ok(mut f) = fs::File::open(&seg_path) {
                let mut buf = Vec::new();
                if f.read_to_end(&mut buf).is_ok() {
                    let _ = output_file.write_all(&buf);
                    written += buf.len() as u64;
                }
            }
        }
    }

    // 5. Cleanup temp
    let _ = fs::remove_dir_all(&temp_dir);

    let elapsed = start.elapsed().as_secs_f64();
    let ok = success_flag.load(Ordering::Relaxed) && segs_ok > 0;

    DownloadResult {
        output_path,
        total_bytes: written,
        segments_downloaded: segs_ok,
        duration_secs: elapsed,
        success: ok,
        error: if ok { None } else { Some(format!("Only {}/{} segments downloaded", segs_ok, total)) },
    }
}

fn get_base_url(url: &str) -> String {
    if let Some(pos) = url.rfind('/') {
        url[..=pos].to_string()
    } else {
        url.to_string()
    }
}

fn parse_hls_playlist(content: &str, base_url: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut is_segment = false;

    for line in content.lines() {
        let line = line.trim();
        
        if line.starts_with("#EXTINF:") {
            is_segment = true;
            continue;
        }
        
        if is_segment && !line.starts_with('#') && !line.is_empty() {
            let url = if line.starts_with("http") {
                line.to_string()
            } else {
                format!("{}{}", base_url, line.trim_start_matches('/'))
            };
            segments.push(url);
            is_segment = false;
        }
    }

    segments
}

fn find_best_variant(content: &str, base_url: &str) -> Option<String> {
    let mut best_bandwidth = 0u64;
    let mut best_url = None;

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("#EXT-X-STREAM-INF:") {
            if let Some(bw_str) = line.split(|c| c == ',' || c == ' ')
                .find(|s| s.starts_with("BANDWIDTH="))
            {
                let bw = bw_str.trim_start_matches("BANDWIDTH=").parse::<u64>().unwrap_or(0);
                // Next non-empty, non-comment line is the URL
                if i + 1 < lines.len() {
                    let url_line = lines[i + 1].trim();
                    if !url_line.starts_with('#') && !url_line.is_empty() {
                        let url = if url_line.starts_with("http") {
                            url_line.to_string()
                        } else {
                            format!("{}{}", base_url, url_line.trim_start_matches('/'))
                        };
                        if bw > best_bandwidth {
                            best_bandwidth = bw;
                            best_url = Some(url);
                        }
                    }
                }
            }
        }
        i += 1;
    }

    best_url
}

// ============================================================
// DESCARGA DASH (.mpd)
// ============================================================

pub fn download_dash(
    manifest_url: &str,
    output_dir: &Path,
    filename: Option<&str>,
    cancel_flag: Option<Arc<AtomicBool>>,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> DownloadResult {
    let start = Instant::now();
    let name = filename.unwrap_or("darkdm_dash_video");
    let output_path = output_dir.join(format!("{}.mp4", name));
    let temp_dir = output_dir.join(format!("{}_segments", name));

    fs::create_dir_all(&temp_dir).unwrap_or_default();
    fs::create_dir_all(output_dir).unwrap_or_default();

    // 1. Fetch MPD manifest
    let manifest = match fetch_url(&manifest_url) {
        Ok(c) => String::from_utf8_lossy(&c).to_string(),
        Err(e) => return DownloadResult {
            output_path, total_bytes: 0, segments_downloaded: 0,
            duration_secs: start.elapsed().as_secs_f64(),
            success: false, error: Some(format!("Failed to fetch MPD: {}", e)),
        },
    };

    // 2. Simple XML parsing for MPD
    let base_url = get_base_url(manifest_url);
    let segments = parse_dash_manifest(&manifest, &base_url);

    if segments.is_empty() {
        return DownloadResult {
            output_path, total_bytes: 0, segments_downloaded: 0,
            duration_secs: start.elapsed().as_secs_f64(),
            success: false, error: Some("No segments found in DASH manifest".to_string()),
        };
    }

    eprintln!("[DarkDM DASH] Found {} segments", segments.len());

    // 3. Download segments
    let total = segments.len();
    let downloaded = Arc::new(AtomicUsize::new(0));
    let bytes_downloaded = Arc::new(AtomicUsize::new(0));
    let cancel = cancel_flag.unwrap_or(Arc::new(AtomicBool::new(false)));

    let mut handles = vec![];

    for (i, seg_url) in segments.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) { break; }

        let url = seg_url.clone();
        let tdir = temp_dir.clone();
        let dl = downloaded.clone();
        let bytes = bytes_downloaded.clone();
        let cancel = cancel.clone();

        let handle = thread::spawn(move || {
            if cancel.load(Ordering::Relaxed) { return; }
            
            let ext = if url.contains("init") { "m4s" } else { "m4s" };
            let seg_path = tdir.join(format!("seg_{:05}.{}", i, ext));
            
            match fetch_url(&url) {
                Ok(data) => {
                    if let Ok(mut f) = fs::File::create(&seg_path) {
                        let _ = f.write_all(&data);
                    }
                    dl.fetch_add(1, Ordering::Relaxed);
                    bytes.fetch_add(data.len(), Ordering::Relaxed);
                }
                Err(e) => {
                    eprintln!("[DarkDM DASH] Segment {} failed: {}", i, e);
                }
            }
        });

        handles.push(handle);
        if handles.len() >= 8 {
            let _ = handles.remove(0).join();
        }
    }

    for h in handles { let _ = h.join(); }

    let total_bytes = bytes_downloaded.load(Ordering::Relaxed);
    let segs_ok = downloaded.load(Ordering::Relaxed);
    
    // 4. Assemble: DASH segments need concatenation with init segment first
    let mut output_file = fs::File::create(&output_path).unwrap_or_else(|_| panic!("Cannot create {}", output_path.display()));
    let mut written: u64 = 0;

    // Write init segment first (if exists)
    let init_path = temp_dir.join("seg_00000.m4s");
    if init_path.exists() {
        if let Ok(mut f) = fs::File::open(&init_path) {
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() {
                let _ = output_file.write_all(&buf);
                written += buf.len() as u64;
            }
        }
    }

    // Write media segments
    for i in 1..total {
        let seg_path = temp_dir.join(format!("seg_{:05}.m4s", i));
        if seg_path.exists() {
            if let Ok(mut f) = fs::File::open(&seg_path) {
                let mut buf = Vec::new();
                if f.read_to_end(&mut buf).is_ok() {
                    let _ = output_file.write_all(&buf);
                    written += buf.len() as u64;
                }
            }
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);

    DownloadResult {
        output_path,
        total_bytes: written,
        segments_downloaded: segs_ok,
        duration_secs: start.elapsed().as_secs_f64(),
        success: segs_ok > 0,
        error: if segs_ok == total { None } else { Some(format!("{}/{} segments", segs_ok, total)) },
    }
}

/// Parse DASH MPD manifest to extract segment URLs (simplified XML parsing)
fn parse_dash_manifest(content: &str, base_url: &str) -> Vec<String> {
    let mut segments = Vec::new();

    // Extract BaseURL
    let manifest_base_url = if let Some(start) = content.find("<BaseURL>") {
        let s = start + "<BaseURL>".len();
        if let Some(end) = content[s..].find("</BaseURL>") {
            let url = content[s..s + end].trim();
            if url.starts_with("http") { url.to_string() }
            else { format!("{}{}", base_url, url.trim_start_matches('/')) }
        } else { base_url.to_string() }
    } else { base_url.to_string() };

    // Extract SegmentTemplate or SegmentList
    let template_url = extract_xml_tag(&content, "SegmentTemplate", "media");
    let initialization = extract_xml_tag(&content, "SegmentTemplate", "initialization");
    let segment_duration = extract_xml_tag(&content, "SegmentTimeline", "duration");
    
    // Count segments from SegmentTimeline S elements
    let mut total_segments = 0;
    let mut pos = 0;
    while let Some(s_start) = content[pos..].find("<S ") {
        total_segments += 1;
        pos += s_start + 2;
    }
    
    // Also check for <S> without attributes
    if total_segments == 0 {
        pos = 0;
        while let Some(_) = content[pos..].find("<S>") {
            total_segments += 1;
            pos += 3;
        }
    }
    // Count <SegmentURL> for SegmentList
    if total_segments == 0 {
        pos = 0;
        while let Some(s_start) = content[pos..].find("<SegmentURL") {
            total_segments += 1;
            pos += s_start + 5;
        }
    }

    // Extract <SegmentURL media=""> elements
    pos = 0;
    while let Some(s_start) = content[pos..].find("<SegmentURL") {
        let chunk = &content[pos..];
        let end = chunk.find('>').unwrap_or(chunk.len());
        let tag = &chunk[..end];
        
        if let Some(media_start) = tag.find("media=\"") {
            let q = media_start + "media=\"".len();
            if let Some(media_end) = tag[q..].find('"') {
                let url = &tag[q..q + media_end];
                if !url.is_empty() {
                    let full_url = if url.starts_with("http") {
                        url.to_string()
                    } else {
                        format!("{}{}", manifest_base_url, url.trim_start_matches('/'))
                    };
                    segments.push(full_url);
                }
            }
        }
        pos += s_start + 1;
    }

    // If no SegmentURL but has template, generate URLs
    if segments.is_empty() && !template_url.is_empty() {
        let base = manifest_base_url.trim_end_matches('/');
        let tmpl = template_url.trim_start_matches('/');
        
        // Add initialization segment
        if !initialization.is_empty() {
            let init_url = format!("{}/{}", base, initialization.trim_start_matches('/'));
            segments.push(init_url);
        }
        
        // Add $Number$ segments
        for n in 1..=total_segments.max(100) {
            let url = template_url
                .replace("$Number$", &n.to_string())
                .replace("$Number%01d$", &format!("{:01}", n))
                .replace("$Number%02d$", &format!("{:02}", n))
                .replace("$Number%03d$", &format!("{:03}", n))
                .replace("$Number%04d$", &format!("{:04}", n))
                .replace("$Number%05d$", &format!("{:05}", n))
                .replace("$Time$", &((n as u64 - 1) * segment_duration.parse::<u64>().unwrap_or(2000)).to_string());
            
            let full_url = if url.starts_with("http") {
                url.to_string()
            } else {
                format!("{}/{}", base, url.trim_start_matches('/'))
            };
            segments.push(full_url);
        }
    }

    segments
}

fn extract_xml_tag(xml: &str, tag: &str, attr: &str) -> String {
    let search = format!("<{} ", tag);
    if let Some(start) = xml.find(&search) {
        let chunk = &xml[start..];
        let end = chunk.find('>').unwrap_or(chunk.len());
        let opening = &chunk[..end];
        
        let attr_search = format!("{}=\"", attr);
        if let Some(a_start) = opening.find(&attr_search) {
            let q = a_start + attr_search.len();
            if let Some(a_end) = opening[q..].find('"') {
                return opening[q..q + a_end].to_string();
            }
        }
    }
    String::new()
}

// ============================================================
// UTILIDADES DE RED
// ============================================================

fn fetch_url(url: &str) -> Result<Vec<u8>, String> {
    // Try to use system curl first (most compatible)
    let output = std::process::Command::new("curl")
        .args(["-s", "-L", "--max-time", "30", "-A", 
               "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
               url])
        .output()
        .map_err(|e| format!("curl failed: {}", e))?;

    if output.status.success() {
        return Ok(output.stdout);
    }

    // Fallback: use ureq library
    #[cfg(feature = "use-ureq")]
    {
        let resp = ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .call()
            .map_err(|e| format!("ureq failed: {}", e))?;
        let mut body = Vec::new();
        resp.into_reader().read_to_end(&mut body)
            .map_err(|e| format!("ureq read failed: {}", e))?;
        return Ok(body);
    }

    #[cfg(not(feature = "use-ureq"))]
    Err(format!("curl returned non-zero exit: {:?}", output.status))
}

/// Detect if a URL points to a playable video stream
pub fn detect_stream_type(url: &str, content_type: Option<&str>) -> StreamType {
    if let Some(ct) = content_type {
        let ct_lower = ct.to_lowercase();
        if ct_lower.contains("mpegurl") || ct_lower.contains("x-mpegurl") {
            return StreamType::HLS;
        }
        if ct_lower.contains("dash+xml") {
            return StreamType::DASH;
        }
        if ct_lower.starts_with("video/") || ct_lower.starts_with("audio/") {
            return StreamType::Direct;
        }
    }
    StreamType::from_url(url)
}
