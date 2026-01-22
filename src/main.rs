//! Claude Terminal - A fast, responsive terminal interface for Claude Code

mod app;
mod bash;
mod claude;
mod sessions;
mod ui;
mod voice;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser, Debug)]
#[command(name = "claude-terminal")]
#[command(about = "A fast, responsive terminal interface for Claude Code")]
#[command(version)]
struct Args {
    /// Model to use (e.g., sonnet, opus, haiku)
    #[arg(short, long, default_value = "sonnet")]
    model: String,

    /// Working directory
    #[arg(short = 'd', long)]
    directory: Option<String>,

    /// Continue the most recent conversation
    #[arg(short, long)]
    continue_session: bool,

    /// Resume a specific session by ID
    #[arg(short, long)]
    resume: Option<String>,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Set up logging
    let filter = if args.debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();

    // Change to specified directory if provided
    if let Some(dir) = &args.directory {
        std::env::set_current_dir(dir)?;
    }

    // Run the app
    let mut app = app::App::new(args.model, args.continue_session, args.resume)?;
    app.run().await
}
