use clap::Parser;
use serde::Deserialize;
use std::net::SocketAddr;
use std::os::unix::net::UnixDatagram;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::time::{timeout, Duration};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "agentd")]
#[command(about = "Agentd daemon - System-level AI Agent runtime")]
struct Args {
    #[arg(long, default_value = "configs/agentd.toml")]
    config: String,

    #[arg(long)]
    health_host: Option<String>,

    #[arg(long)]
    health_port: Option<u16>,

    #[arg(long, short)]
    verbose: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct AppConfig {
    #[serde(default)]
    daemon: DaemonConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct DaemonConfig {
    #[serde(default = "default_health_host")]
    health_host: String,
    #[serde(default = "default_health_port")]
    health_port: u16,
    #[serde(default = "default_shutdown_timeout_secs")]
    shutdown_timeout_secs: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            health_host: default_health_host(),
            health_port: default_health_port(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            daemon: DaemonConfig::default(),
        }
    }
}

fn default_health_host() -> String {
    "127.0.0.1".to_string()
}

fn default_health_port() -> u16 {
    7000
}

fn default_shutdown_timeout_secs() -> u64 {
    5
}

fn load_config(path: &str) -> Result<AppConfig, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        info!(config_path = path, "Config file not found, using defaults");
        return Ok(AppConfig::default());
    }

    let content = std::fs::read_to_string(path)?;
    let config = toml::from_str::<AppConfig>(&content)?;
    Ok(config)
}

fn notify_systemd(state: &str) {
    let Some(socket_path) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };

    let socket_path = socket_path.to_string_lossy();
    let target = if socket_path.starts_with('@') {
        format!("\0{}", &socket_path[1..])
    } else {
        socket_path.to_string()
    };

    let send_result = (|| -> std::io::Result<()> {
        let sock = UnixDatagram::unbound()?;
        sock.connect(target)?;
        let _ = sock.send(state.as_bytes())?;
        Ok(())
    })();

    if let Err(err) = send_result {
        warn!(%err, state, "Failed to send systemd notification");
    }
}

async fn health_server(
    bind_addr: SocketAddr,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(bind_addr).await?;
    info!(%bind_addr, "Health endpoint listening");

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    info!("Health server received shutdown signal");
                    break;
                }
            }
            accepted = listener.accept() => {
                let (mut stream, _) = accepted?;

                let mut buf = [0_u8; 1024];
                let read = stream.read(&mut buf).await?;
                let request = String::from_utf8_lossy(&buf[..read]);
                let is_health = request.starts_with("GET /health ") || request.starts_with("GET /health?");

                let response = if is_health {
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "Content-Type: application/json\r\n",
                        "Content-Length: 16\r\n",
                        "Connection: close\r\n",
                        "\r\n",
                        "{\"status\":\"ok\"}"
                    )
                } else {
                    concat!(
                        "HTTP/1.1 404 Not Found\r\n",
                        "Content-Type: application/json\r\n",
                        "Content-Length: 21\r\n",
                        "Connection: close\r\n",
                        "\r\n",
                        "{\"error\":\"not found\"}"
                    )
                };

                stream.write_all(response.as_bytes()).await?;
                let _ = stream.shutdown().await;
            }
        }
    }

    Ok(())
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

    let mut config = load_config(&args.config)?;
    if let Some(health_host) = args.health_host {
        config.daemon.health_host = health_host;
    }
    if let Some(health_port) = args.health_port {
        config.daemon.health_port = health_port;
    }

    let bind_addr: SocketAddr = format!(
        "{}:{}",
        config.daemon.health_host, config.daemon.health_port
    )
    .parse()?;

    info!("Starting agentd daemon");
    info!(config_path = %args.config, "Loaded daemon config");

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let health_task = tokio::spawn(health_server(bind_addr, shutdown_rx));

    notify_systemd("READY=1");
    info!("Systemd READY=1 notification sent");

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, initiating graceful shutdown");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM, initiating graceful shutdown");
        }
    }

    let _ = shutdown_tx.send(true);

    let timeout_secs = config.daemon.shutdown_timeout_secs;
    let shutdown_result = timeout(Duration::from_secs(timeout_secs), health_task).await;
    match shutdown_result {
        Ok(join_result) => match join_result {
            Ok(Ok(())) => {
                info!(timeout_secs, "Graceful shutdown completed successfully");
            }
            Ok(Err(err)) => {
                error!(%err, timeout_secs, "Health server exited with error during shutdown");
            }
            Err(err) => {
                error!(%err, timeout_secs, "Health server task join failed during shutdown");
            }
        },
        Err(_) => {
            warn!(timeout_secs, "Graceful shutdown timed out");
        }
    }

    notify_systemd("STOPPING=1");
    info!("Daemon shutdown sequence finished");

    Ok(())
}
