use clap::{Parser, Subcommand};
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "agentctl")]
#[command(about = "CLI for agentd daemon")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    List,
    Get { id: String },
    Create { name: String },
    Delete { id: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    match cli.command {
        Commands::List => {
            info!("Listing agents from {}", cli.url);
        }
        Commands::Get { id } => {
            info!("Getting agent {} from {}", id, cli.url);
        }
        Commands::Create { name } => {
            info!("Creating agent {} at {}", name, cli.url);
        }
        Commands::Delete { id } => {
            info!("Deleting agent {} from {}", id, cli.url);
        }
    }

    Ok(())
}
