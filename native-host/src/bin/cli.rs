// DarkDM CLI — download HLS streams from terminal
// Usage: darkdm-cli <m3u8_url> [output.mp4]
//        darkdm-cli <m3u8_url> --referer <url> [--user-agent <ua>]

use std::env;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: darkdm-cli <m3u8_url> [output.mp4] [--referer <url>] [--user-agent <ua>]");
        eprintln!("Example: darkdm-cli https://cdn.example.com/stream.m3u8 video.mp4 --referer https://example.com");
        std::process::exit(1);
    }

    let url = &args[1];
    let output = if args.len() > 2 && !args[2].starts_with("--") {
        args[2].clone()
    } else {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let out_dir = Path::new(&home).join("Descargas/DarkDM");
        let _ = std::fs::create_dir_all(&out_dir);
        out_dir.join("video.mp4").to_string_lossy().to_string()
    };

    let referer = get_flag(&args, "--referer");
    let default_ua = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/146.0.0.0 Safari/537.36";
    let user_agent = if let Some(ua) = get_flag_opt(&args, "--user-agent") { ua } else { default_ua.to_string() };

    let page_url = if referer.is_empty() {
        if let Some(pos) = url.find("/hls/") {
            url[..url.find("?v=").unwrap_or(url.len())].to_string()
        } else {
            url.to_string()
        }
    } else {
        referer.clone()
    };

    eprintln!("Downloading: {}", &url[..url.len().min(100)]);
    eprintln!("Output: {}", output);
    eprintln!("Referer: {}", page_url);

    // Strategy 1: Try ffmpeg direct (works for standard HLS)
    eprintln!("Trying ffmpeg...");
    let ffmpeg_ok = try_ffmpeg(url, &output, &user_agent, &page_url);

    if ffmpeg_ok {
        let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        eprintln!("Done! {} bytes -> {}", size, output);
        return;
    }

    // Strategy 2: Download manifest + segments with PNG strip fallback
    eprintln!("ffmpeg failed, downloading segments manually...");
    download_segments(url, &output, &user_agent, &page_url);

    let size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    eprintln!("Done! {} bytes -> {}", size, output);
}

fn get_flag(args: &[String], flag: &str) -> String {
    get_flag_opt(args, flag).unwrap_or_default()
}

fn get_flag_opt(args: &[String], flag: &str) -> Option<String> {
    for i in 0..args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
    }
    None
}

fn try_ffmpeg(url: &str, output: &str, user_agent: &str, referer: &str) -> bool {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-hide_banner", "-loglevel", "error",
              "-user_agent", user_agent]);
    if !referer.is_empty() {
        cmd.args(["-referer", referer]);
    }
    cmd.args(["-i", url, "-c", "copy", "-movflags", "+faststart", output]);
    matches!(cmd.status(), Ok(s) if s.success())
}

fn download_segments(url: &str, output: &str, user_agent: &str, referer: &str) {
    // Download manifest
    let out = Command::new("curl")
        .args(["-s", "-L", "-A", user_agent, "-H", &format!("Referer: {}", referer), url])
        .output();
    let manifest = match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => { eprintln!("Failed to download manifest"); return; }
    };

    // Parse segments
    let base = if let Some(pos) = url.rfind('/') { &url[..=pos] } else { url };
    let segments: Vec<String> = manifest.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            if l.starts_with("http") { l.to_string() }
            else { format!("{}{}", base, l) }
        })
        .collect();

    if segments.is_empty() { eprintln!("No segments found"); return; }

    // Detect PNG wrapper
    let curl_args = || {
        let mut cmd = Command::new("curl");
        cmd.args(["-s", "-L", "-A", user_agent, "-H", &format!("Referer: {}", referer)]);
        cmd
    };

    let tmp = "/tmp/darkdm_cli_seg.bin";
    let _ = curl_args().args(["-o", tmp, &segments[0]]).status();
    let test_data = std::fs::read(tmp).unwrap_or_default();
    let is_png = test_data.len() > 4 && &test_data[0..4] == b"\x89PNG";
    let strip_bytes = if is_png {
        test_data.windows(4).position(|w| w == b"IEND").map(|p| p + 12).unwrap_or(0)
    } else { 0 };

    eprintln!("Found {} segments, PNG wrapper: {}", segments.len(),
        if is_png { format!("yes (strip {}B)", strip_bytes) } else { "no".to_string() });

    // Download to temp dir
    let seg_dir = "/tmp/darkdm_cli_segs";
    let _ = std::fs::create_dir_all(seg_dir);
    let mut concat = String::new();
    let total = segments.len();

    for (i, seg_url) in segments.iter().enumerate() {
        let _ = curl_args().args(["-o", tmp, seg_url]).status();
        if let Ok(d) = std::fs::read(tmp) {
            let payload = if strip_bytes > 0 && d.len() > strip_bytes { &d[strip_bytes..] } else { &d };
            let name = format!("{:05}.ts", i);
            if std::fs::write(format!("{}/{}", seg_dir, name), payload).is_ok() {
                concat.push_str(&format!("file '{}'\n", name));
            }
        }
        if i % 200 == 0 { eprintln!("Downloding: {}/{}", i, total); }
    }

    // Write concat list
    let concat_file = format!("{}/_concat.txt", seg_dir);
    std::fs::write(&concat_file, &concat).unwrap_or_default();

    // Concatenate with ffmpeg (fixes timestamps)
    eprintln!("Concatenating with ffmpeg...");
    let status = Command::new("ffmpeg")
        .args(["-y", "-f", "concat", "-safe", "0", "-fflags", "+genpts",
               "-i", &concat_file, "-c", "copy", output])
        .status();

    let _ = std::fs::remove_dir_all(seg_dir);
    let _ = std::fs::remove_file(tmp);

    match status {
        Ok(s) => eprintln!("Concat: {}", if s.success() { "OK" } else { "FAILED" }),
        Err(e) => eprintln!("Error: {}", e),
    }
}
