// DarkDM CLI — unified download manager
//
// Usage:
//   darkdm descargar <url>
//   darkdm info <url>

use clap::{Parser, Subcommand};
use app_lib::downloader::{DownloadEngine, DownloadConfig};
use std::path::PathBuf;

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
    },
    
    /// Get information about a URL without downloading
    Info {
        /// URL to probe
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Descargar { url, output, threads, no_resume } => {
            cmd_descargar(url, output, threads, !no_resume).await?;
        }
        Commands::Info { url } => {
            cmd_info(url).await?;
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
    
    let mut engine = DownloadEngine::new(url.clone(), config);
    
    // Probe first
    println!("\n📡 Probing URL...");
    let probe = engine.probe().await?;
    
    println!("✓ Probe complete:");
    println!("  Filename: {}", probe.filename_or_default());
    if let Some(size) = probe.resource_size {
        println!("  Size: {} bytes ({:.2} MB)", size, size as f64 / 1024.0 / 1024.0);
    }
    println!("  Resumable: {}", if probe.resumable { "yes" } else { "no" });
    if let Some(ct) = &probe.content_type {
        println!("  Content-Type: {}", ct);
    }
    
    // Start download
    println!("\n⬇️  Starting download...");
    engine.download().await?;
    
    println!("\n✅ Download complete!");
    
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
