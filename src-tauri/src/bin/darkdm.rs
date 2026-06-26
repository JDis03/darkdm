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
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize logging based on verbose flag
    match &cli.command {
        Commands::Descargar { verbose, .. } | Commands::Info { verbose, .. } => {
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
