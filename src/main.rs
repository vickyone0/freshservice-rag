mod scraper;
mod rag;
mod llm;
mod models;
mod web;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "freshservice-rag")]
#[command(about = "Freshservice API Documentation RAG System")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scrape Freshservice documentation
    Scrape {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Start the web interface
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Scrape { output } => {
            println!("Scraping Freshservice API documentation...");
            let scraper = scraper::FreshserviceScraper::new();
            let documentation = scraper.scrape_ticket_attributes().await?;
            
            // Save scraped data
            let output_path = output.unwrap_or_else(|| PathBuf::from("data/scraped/documentation.json"));
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, serde_json::to_string_pretty(&documentation)?)?;
            println!("Documentation saved to: {}", output_path.display());
        }
        Commands::Serve { port } => {
            println!("Starting web server on port {}...", port);
            web::run_server(port).await?;
        }
    }
       

    Ok(())
}