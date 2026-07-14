use std::net::IpAddr;
use std::path::PathBuf;

use backend::core::metainfo::Metainfo;
use backend::core::net_util::detect_local_ip;
use backend::session::{Session, SessionConfig};

const USAGE: &str = "\
bittorrent <command> [OPTIONS]

Commands:
  download  Download a torrent
  seed      Seed a torrent

Options:
  --output <dir>     Output directory (default: ./downloads)
  --data <dir>       Data directory for seeding
  --dht <endpoint>   DHT Sidecar gRPC address (default: http://127.0.0.1:50051)
  --bind <ip>        Local IP to announce to DHT (default: auto-detect)
  --port <port>      Peer listening port (default: 6881)
  --max-peers <n>    Maximum peer connections (default: 50)
  --pipeline <n>     Pipeline depth (default: 5)
  --tracker <url>    Tracker URL (reserved, not yet implemented)

Examples:
  bittorrent download file.torrent --output ./out --bind 192.168.1.5
  bittorrent seed file.torrent --data ./data --bind 192.168.1.5
";

#[derive(Default)]
struct CliArgs {
    command: Option<String>,
    torrent_file: Option<PathBuf>,
    output: PathBuf,
    data_dir: Option<PathBuf>,
    dht_endpoint: String,
    bind_addr: Option<IpAddr>,
    peer_port: u16,
    max_peers: usize,
    pipeline_depth: usize,
    tracker: Option<String>,
}

fn parse_args() -> Result<CliArgs, String> {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs::default();

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            "--output" => {
                i += 1;
                cli.output = PathBuf::from(next_arg(&args, i, "--output")?);
            }
            "--data" => {
                i += 1;
                cli.data_dir = Some(PathBuf::from(next_arg(&args, i, "--data")?));
            }
            "--dht" => {
                i += 1;
                cli.dht_endpoint = next_arg(&args, i, "--dht")?.to_string();
            }
            "--bind" => {
                i += 1;
                let ip_str = next_arg(&args, i, "--bind")?;
                cli.bind_addr = Some(
                    ip_str
                        .parse()
                        .map_err(|_| format!("invalid IP address: {}", ip_str))?,
                );
            }
            "--port" => {
                i += 1;
                cli.peer_port = next_arg(&args, i, "--port")?
                    .parse()
                    .map_err(|_| String::from("invalid port"))?;
            }
            "--max-peers" => {
                i += 1;
                cli.max_peers = next_arg(&args, i, "--max-peers")?
                    .parse()
                    .map_err(|_| String::from("invalid max-peers"))?;
            }
            "--pipeline" => {
                i += 1;
                cli.pipeline_depth = next_arg(&args, i, "--pipeline")?
                    .parse()
                    .map_err(|_| String::from("invalid pipeline depth"))?;
            }
            "--tracker" => {
                i += 1;
                cli.tracker = Some(next_arg(&args, i, "--tracker")?.to_string());
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown flag: {}", other));
            }
            other => {
                if cli.command.is_none() {
                    cli.command = Some(other.to_string());
                } else if cli.torrent_file.is_none() {
                    cli.torrent_file = Some(PathBuf::from(other));
                } else {
                    return Err(format!("unexpected argument: {}", other));
                }
            }
        }
        i += 1;
    }

    if cli.command.is_none() {
        return Err("usage: bittorrent <command> <torrent_file> [OPTIONS]".into());
    }
    if cli.torrent_file.is_none() {
        return Err("missing torrent file".into());
    }

    Ok(cli)
}

fn next_arg<'a>(args: &'a [String], i: usize, name: &str) -> Result<&'a str, String> {
    args.get(i)
        .map(|a| a.as_str())
        .ok_or_else(|| format!("missing value for {}", name))
}

fn build_config(cli: &CliArgs) -> SessionConfig {
    SessionConfig {
        dht_endpoint: cli.dht_endpoint.clone(),
        bind_addr: cli.bind_addr.unwrap_or_else(detect_local_ip),
        peer_port: cli.peer_port,
        max_peers: cli.max_peers,
        pipeline_depth: cli.pipeline_depth,
        dht_refresh_interval_secs: 300,
        upload_slots: 4,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("use --help for usage");
            std::process::exit(1);
        }
    };

    let result = match cli.command.as_deref() {
        Some("download") => run_download(&cli).await,
        Some("seed") => run_seed(&cli).await,
        _ => {
            eprintln!("error: unknown command {:?}", cli.command);
            eprintln!("use --help for usage");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("fatal: {}", e);
        std::process::exit(1);
    }
}

async fn run_download(cli: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let torrent_bytes = std::fs::read(cli.torrent_file.as_ref().unwrap())?;
    let metainfo = Metainfo::from_bytes(&torrent_bytes)?;

    if cli.tracker.is_some() {
        tracing::info!("tracker specified but not yet implemented, using DHT only");
    }

    tracing::info!("Torrent: {}", metainfo.info.name);
    tracing::info!(
        "Size: {} bytes ({:.2} MiB)",
        metainfo.info.total_length,
        metainfo.info.total_length as f64 / (1024.0 * 1024.0)
    );
    tracing::info!("Pieces: {}", metainfo.piece_count());
    tracing::info!("InfoHash: {}", metainfo.info_hash);

    let output_dir = &cli.output;
    std::fs::create_dir_all(output_dir)?;
    tracing::info!("Output directory: {}", output_dir.display());

    let config = build_config(cli);
    let mut session = Session::new(config, metainfo.clone(), Some(output_dir)).await?;

    let progress_task = tokio::spawn({
        let info_hash = metainfo.info_hash;
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                tracing::info!("[{}] downloading...", info_hash.short_hex());
            }
        }
    });

    session.download().await?;
    progress_task.abort();

    tracing::info!("Download complete!");
    tracing::info!("Files saved to: {}", output_dir.display());

    Ok(())
}

async fn run_seed(cli: &CliArgs) -> Result<(), Box<dyn std::error::Error>> {
    let torrent_bytes = std::fs::read(cli.torrent_file.as_ref().unwrap())?;
    let metainfo = Metainfo::from_bytes(&torrent_bytes)?;

    let data_dir = cli
        .data_dir
        .as_ref()
        .ok_or("--data <dir> is required for seed mode")?;

    if cli.tracker.is_some() {
        tracing::info!("tracker specified but not yet implemented, using DHT only");
    }

    tracing::info!("Torrent: {}", metainfo.info.name);
    tracing::info!(
        "Size: {} bytes ({:.2} MiB)",
        metainfo.info.total_length,
        metainfo.info.total_length as f64 / (1024.0 * 1024.0)
    );
    tracing::info!("Pieces: {}", metainfo.piece_count());
    tracing::info!("InfoHash: {}", metainfo.info_hash);
    tracing::info!("Data directory: {}", data_dir.display());

    let config = build_config(cli);
    let mut session = Session::new(config, metainfo.clone(), None).await?;

    session.seed(data_dir).await?;

    Ok(())
}
