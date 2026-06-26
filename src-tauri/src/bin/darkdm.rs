// DarkDM CLI — unified download manager
//
// Usage:
//   darkdm descargar <url>
//   darkdm info <url>

use clap::{Parser, Subcommand};
use app_lib::downloader::{DownloadEngine, DownloadConfig};
use app_lib::downloader::{content_type, page_analyzer, hls_handler, logger};
use app_lib::downloader::plugins::{ExtractorRegistry, MediaFireExtractor, YouTubeExtractor, NetflixExtractor};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "darkdm")]
#[command(about = "DarkDM — Download Manager for Linux", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download a file from URL
    Descargar {
        /// URL to download
        url: String,
        
        /// Output directory (default: ~/Descargas/DarkDM)
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// Number of parallel workers (default: 8)
        #[arg(short = 't', long, default_value = "8")]
        threads: usize,
        
        /// Disable resume
        #[arg(long)]
        no_resume: bool,
        
        /// Enable verbose logging (debug level)
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Get information about a URL without downloading
    Info {
        /// URL to probe
        url: String,
        
        /// Enable verbose logging (debug level)
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Show log file location and recent entries
    Logs {
        /// Number of recent lines to show (default: 50)
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
        
        /// Follow log file (like tail -f)
        #[arg(short, long)]
        follow: bool,
    },
    
    /// Batch download multiple URLs from a file (one per line)
    /// Useful for Netflix segments, playlists, or any multi-file content
    Batch {
        /// File containing URLs (one per line)
        file: PathBuf,
        
        /// Output directory
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// Number of parallel workers per file (default: 4)
        #[arg(short = 't', long, default_value = "4")]
        threads: usize,
        
        /// Concat all downloaded files into one (uses ffmpeg)
        #[arg(short, long)]
        concat: bool,
        
        /// Output filename for concatenated result (requires --concat)
        #[arg(short = 'n', long)]
        name: Option<String>,
        
        /// Enable verbose logging
        #[arg(short, long)]
        verbose: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize logging based on verbose flag
    match &cli.command {
        Commands::Descargar { verbose, .. } | Commands::Info { verbose, .. } | Commands::Batch { verbose, .. } => {
            if *verbose {
                logger::init_with_level("debug");
            } else {
                logger::init_with_level("info");
            }
        }
        Commands::Logs { .. } => {
            // Don't initialize logging for logs command
        }
    }
    
    tracing::info!("DarkDM CLI started");
    
    match cli.command {
        Commands::Descargar { url, output, threads, no_resume, verbose: _ } => {
            cmd_descargar(url, output, threads, !no_resume).await?;
        }
        Commands::Info { url, verbose: _ } => {
            cmd_info(url).await?;
        }
        Commands::Logs { lines, follow } => {
            cmd_logs(lines, follow)?;
        }
        Commands::Batch { file, output, threads, concat, name, verbose: _ } => {
            cmd_batch(file, output, threads, concat, name).await?;
        }
    }
    
    Ok(())
}

async fn cmd_descargar(
    url: String,
    output: Option<PathBuf>,
    threads: usize,
    resume: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔽 DarkDM — Downloading");
    println!("URL: {}", url);
    
    let mut config = DownloadConfig::default();
    config.max_workers = threads;
    config.resume = resume;
    
    if let Some(output_dir) = output {
        config.output_dir = output_dir;
    }
    
    let config_clone = config.clone();
    let mut engine = DownloadEngine::new(url.clone(), config);
    
    // Probe first
    println!("\n📡 Probing URL...");
    tracing::info!("Starting probe for URL: {}", url);
    let probe = engine.probe().await?;
    
    tracing::debug!("Probe result: filename={}, size={:?}, resumable={}", 
        probe.filename_or_default(), probe.resource_size, probe.resumable);
    
    println!("✓ Probe complete:");
    println!("  Filename: {}", probe.filename_or_default());
    if let Some(size) = probe.resource_size {
        println!("  Size: {} bytes ({:.2} MB)", size, size as f64 / 1024.0 / 1024.0);
    }
    println!("  Resumable: {}", if probe.resumable { "yes" } else { "no" });
    if let Some(ct) = &probe.content_type {
        println!("  Content-Type: {}", ct);
        
        // Check if it's HTML - need to analyze page
        if content_type::needs_extraction(ct) {
            println!("\n⚠️  Detected HTML page, trying extractors...");
            tracing::info!("Content-Type indicates HTML, attempting extraction");
            
            // Setup plugin registry
            let mut registry = ExtractorRegistry::new();
            registry.register(Arc::new(YouTubeExtractor::new()));
            registry.register(Arc::new(MediaFireExtractor::new()));
            registry.register(Arc::new(NetflixExtractor::new()));
            
            let parsed_url = url::Url::parse(&url)?;
            
            // Try site-specific extractors first
            if let Some(extractor) = registry.find_extractor(&parsed_url) {
                println!("🔌 Using {} extractor...", extractor.name());
                tracing::info!("Selected extractor: {}", extractor.name());
                
                match extractor.extract(&parsed_url).await {
                    Ok(links) => {
                        if links.is_empty() {
                            eprintln!("❌ Extractor found no downloadable links");
                            return Err("No videos found".into());
                        }
                        
                        println!("\n📦 Found {} resource(s):", links.len());
                        for (i, link) in links.iter().enumerate() {
                            println!("  [{}] {}", i + 1, link.url);
                            if let Some(title) = &link.filename {
                                println!("      Title: {}", title);
                            }
                        }
                        
                        // Download first resource
                        println!("\n⬇️  Downloading...\n");
                        let target_url = links[0].url.clone();
                        
                        // Check if it's HLS
                        if hls_handler::is_hls(&target_url) {
                            let mut filename = links[0].filename.clone()
                                .unwrap_or_else(|| "video".to_string());
                            
                            // Sanitize filename (remove invalid chars)
                            filename = filename
                                .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
                                .trim()
                                .to_string();
                            
                            // Add .mp4 extension if missing
                            if !filename.ends_with(".mp4") && !filename.ends_with(".mkv") {
                                filename.push_str(".mp4");
                            }
                            
                            let output_path = config_clone.output_dir.join(filename);
                            
                            hls_handler::download_hls(&target_url, &output_path, true).await?;
                            println!("✅ Download complete!");
                            return Ok(());
                        }
                        
                        // Regular download
                        let mut new_engine = DownloadEngine::new(target_url, config_clone);
                        new_engine.probe().await?;
                        new_engine.download(true).await?;
                        
                        println!("✅ Download complete!");
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("⚠️  Extractor failed: {}", e);
                        println!("Falling back to generic page analyzer...");
                    }
                }
            }
            
            // Fallback: generic page analyzer
            println!("🔍 Analyzing page for downloadable resources...");
            let html = reqwest::get(&url).await?.text().await?;
            let resources = page_analyzer::analyze_page(&html, &parsed_url);
            
            if resources.is_empty() {
                eprintln!("❌ No downloadable resources found in page");
                return Err("No videos, audios, or files detected".into());
            }
            
            println!("\n📦 Found {} resource(s):", resources.len());
            for (i, resource) in resources.iter().enumerate() {
                println!("  [{}] {:?} - {}", i + 1, resource.resource_type, resource.url);
                if let Some(title) = &resource.title {
                    println!("      Title: {}", title);
                }
            }
            
            // Download first resource
            println!("\n⬇️  Downloading first resource...\n");
            let target_url = resources[0].url.clone();
            
            let mut new_engine = DownloadEngine::new(target_url, config_clone);
            new_engine.probe().await?;
            new_engine.download(true).await?;
            
            println!("✅ Download complete!");
            return Ok(());
        }
    }
    
    // Direct download (not HTML)
    println!("\n⬇️  Starting download...\n");
    engine.download(true).await?;
    
    println!("✅ Download complete!");
    
    Ok(())
}

async fn cmd_info(url: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("📡 DarkDM — Probing URL");
    println!("URL: {}", url);
    
    let config = DownloadConfig::default();
    let mut engine = DownloadEngine::new(url.clone(), config);
    
    let probe = engine.probe().await?;
    
    println!("\n✓ Probe Result:");
    println!("  Filename: {}", probe.filename_or_default());
    
    if let Some(size) = probe.resource_size {
        println!("  Size: {} bytes ({:.2} MB)", size, size as f64 / 1024.0 / 1024.0);
    } else {
        println!("  Size: unknown");
    }
    
    println!("  Resumable: {}", if probe.resumable { "yes" } else { "no" });
    
    if let Some(ct) = &probe.content_type {
        println!("  Content-Type: {}", ct);
    }
    
    println!("  Final URL: {}", probe.final_url);
    
    if probe.is_text_redirect {
        println!("  ⚠️  Text redirect detected");
        if let Some(redirect) = &probe.redirect_url {
            println!("  Redirect target: {}", redirect);
        }
    }
    
    Ok(())
}

/// Batch download multiple URLs from a file
///
/// Each line in the file should be a URL.
/// Useful for Netflix DASH segments, HLS segments, or any multi-file content.
/// Optional: concatenate all files into one with ffmpeg.
async fn cmd_batch(
    file: PathBuf,
    output: Option<PathBuf>,
    threads: usize,
    concat: bool,
    name: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("📦 DarkDM — Batch Download");
    println!("File: {}", file.display());
    
    // Read URLs from file
    let content = tokio::fs::read_to_string(&file).await
        .map_err(|e| format!("Cannot read URL file: {}", e))?;
    
    let urls: Vec<String> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect();
    
    if urls.is_empty() {
        return Err("No URLs found in file (skip empty lines and # comments)".into());
    }
    
    tracing::info!("Batch download: {} URLs from {}", urls.len(), file.display());
    println!("\n📄 Found {} URL(s)", urls.len());
    println!("Workers per file: {}", threads);
    println!("Concatenate: {}", if concat { "yes" } else { "no" });
    
    // Output directory
    let output_dir = output.unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join("Descargas/DarkDM")
    });
    tokio::fs::create_dir_all(&output_dir).await?;
    
    // Download all URLs sequentially (parallel per URL)
    let mut downloaded: Vec<PathBuf> = Vec::with_capacity(urls.len());
    
    for (i, url) in urls.iter().enumerate() {
        println!("\n[{}/{}] Downloading: {}", i + 1, urls.len(), 
            if url.len() > 60 { format!("{}...", &url[..57]) } else { url.clone() });
        
        let mut config = DownloadConfig::default();
        config.max_workers = threads;
        config.output_dir = output_dir.clone();
        
        let mut engine = DownloadEngine::new(url.clone(), config);
        
        match engine.probe().await {
            Ok(probe) => {
                println!("  → {} ({:.2} MB, resumable: {})", 
                    probe.filename_or_default(),
                    probe.resource_size.unwrap_or(0) as f64 / 1024.0 / 1024.0,
                    if probe.resumable { "yes" } else { "no" });
                
                match engine.download(true).await {
                    Ok(()) => {
                        let path = engine.output_path().to_path_buf();
                        downloaded.push(path);
                        tracing::info!("[{}/{}] Downloaded successfully", i + 1, urls.len());
                        println!("  ✅ Downloaded");
                    }
                    Err(e) => {
                        tracing::error!("[{}/{}] Download failed: {}", i + 1, urls.len(), e);
                        eprintln!("  ❌ Failed: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("[{}/{}] Probe failed: {}", i + 1, urls.len(), e);
                eprintln!("  ❌ Probe failed: {}", e);
            }
        }
    }
    
    println!("\n📊 Batch complete: {}/{} downloaded", downloaded.len(), urls.len());
    
    // Concatenate with ffmpeg if requested
    if concat && downloaded.len() > 1 {
        let output_name = name.unwrap_or_else(|| "output.mp4".to_string());
        let output_path = output_dir.join(&output_name);
        
        println!("\n🔗 Concatenating with ffmpeg...");
        tracing::info!("Concatenating {} files into {}", downloaded.len(), output_path.display());
        
        // Create concat file list
        let concat_file = output_dir.join("concat_list.txt");
        let mut concat_content = String::new();
        for path in &downloaded {
            concat_content.push_str(&format!("file '{}'\n", path.display()));
        }
        tokio::fs::write(&concat_file, concat_content).await?;
        
        // Run ffmpeg
        let status = tokio::process::Command::new("ffmpeg")
            .args(["-f", "concat", "-safe", "0"])
            .args(["-i", &concat_file.to_string_lossy()])
            .args(["-c", "copy"])
            .args(["-y", &output_path.to_string_lossy()])
            .status().await
            .map_err(|e| format!("ffmpeg not found: {}. Install with: sudo pacman -S ffmpeg", e))?;
        
        if status.success() {
            println!("  ✅ Concatenated: {}", output_path.display());
            tracing::info!("Concatenation complete: {}", output_path.display());
            
            // Clean up concat file
            tokio::fs::remove_file(&concat_file).await.ok();
            
            // Optional: remove segment files
            println!("  💡 Run 'rm {}' to clean up segments", 
                downloaded.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" "));
        } else {
            tracing::error!("ffmpeg concat failed");
            eprintln!("  ❌ ffmpeg concatenation failed");
        }
    } else if concat && downloaded.len() <= 1 {
        println!("\n⚠️  Need at least 2 files to concatenate");
    }
    
    Ok(())
}

fn cmd_logs(lines: usize, follow: bool) -> Result<(), Box<dyn std::error::Error>> {
    let log_file = logger::get_log_file();
    
    println!("📋 DarkDM Logs");
    println!("Location: {}", log_file.display());
    
    if !log_file.exists() {
        println!("\n⚠️  No log file found yet. Run a download first.");
        return Ok(());
    }
    
    if follow {
        println!("\n🔄 Following log file (Ctrl+C to stop)...\n");
        // Use tail -f
        let status = std::process::Command::new("tail")
            .arg("-f")
            .arg(&log_file)
            .status()?;
        
        if !status.success() {
            return Err("tail command failed".into());
        }
    } else {
        println!("\n📄 Last {} lines:\n", lines);
        // Use tail -n
        let output = std::process::Command::new("tail")
            .arg("-n")
            .arg(lines.to_string())
            .arg(&log_file)
            .output()?;
        
        if !output.status.success() {
            return Err("tail command failed".into());
        }
        
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
    
    Ok(())
}
