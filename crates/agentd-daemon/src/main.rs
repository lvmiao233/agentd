use clap::Parser;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "agentd")]
#[command(about = "Agentd daemon - System-level AI Agent runtime")]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value = "8080")]
    port: u16,

    #[arg(long, short)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_max_level(if args.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .init();

    info!("Starting agentd daemon");
    info!("Listening on {}:{}", args.host, args.port);

    Ok(())
}
