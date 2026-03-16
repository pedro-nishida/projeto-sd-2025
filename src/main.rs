/// Entry point for a cluster node.
///
/// Usage:
/// ```
/// node --id 1 --http-port 8001 --cluster-port 9001 \
///      --peers 2:node2:9002,3:node3:9003            \
///      --peer-http 2:node2:8002,3:node3:8003
/// ```
///
/// Each `--peers` entry is `<id>:<host>:<cluster-port>`.
/// Each `--peer-http` entry is `<id>:<host>:<http-port>`.
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use std::sync::Mutex;

use clap::Parser;
use tokio::time;

mod election;
mod message;
mod node;
mod sync;

#[derive(Parser, Debug)]
#[command(name = "node", about = "Distributed cluster node with heartbeat leader election")]
struct Args {
    /// Unique integer ID for this node (higher ID wins elections).
    #[arg(long)]
    id: u64,

    /// TCP port for the HTTP API (proxied by Nginx).
    #[arg(long, default_value_t = 8000)]
    http_port: u16,

    /// TCP port for inter-node cluster communication.
    #[arg(long, default_value_t = 9000)]
    cluster_port: u16,

    /// Comma-separated peer descriptors: `<id>:<host>:<cluster-port>`.
    /// Example: `2:node2:9002,3:node3:9003`
    #[arg(long, default_value = "")]
    peers: String,

    /// Comma-separated peer HTTP descriptors: `<id>:<host>:<http-port>`.
    /// Example: `2:node2:8002,3:node3:8003`
    #[arg(long, default_value = "")]
    peer_http: String,
}

/// Parse `"<id>:<host>:<port>"` entries from a comma-separated string.
fn parse_peer_list(s: &str) -> Vec<(u64, String)> {
    if s.is_empty() {
        return vec![];
    }
    s.split(',')
        .filter_map(|entry| {
            let parts: Vec<&str> = entry.trim().splitn(3, ':').collect();
            if parts.len() == 3 {
                let id: u64 = parts[0].parse().ok()?;
                let addr = format!("{}:{}", parts[1], parts[2]);
                Some((id, addr))
            } else {
                None
            }
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let peers = parse_peer_list(&args.peers);
    let peer_http_vec = parse_peer_list(&args.peer_http);
    let peer_http_addrs: HashMap<u64, String> = peer_http_vec.into_iter().collect();

    let state = Arc::new(Mutex::new(node::NodeState::new(
        args.id,
        peers,
        peer_http_addrs,
    )));

    eprintln!(
        "[node {}] starting — http:{} cluster:{}",
        args.id, args.http_port, args.cluster_port
    );

    // Give other nodes a moment to start before triggering the first election.
    time::sleep(std::time::Duration::from_secs(2)).await;

    // Spawn cluster TCP server.
    tokio::spawn(node::cluster_server(
        Arc::clone(&state),
        args.cluster_port,
    ));

    // Spawn heartbeat monitor (watches for leader silence → triggers election).
    tokio::spawn(node::heartbeat_monitor(Arc::clone(&state)));

    // Start HTTP server.
    let http_addr = SocketAddr::from(([0, 0, 0, 0], args.http_port));
    let router = node::http_router(Arc::clone(&state));
    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .expect("failed to bind HTTP port");

    eprintln!("[node {}] HTTP listening on {}", args.id, http_addr);
    axum::serve(listener, router).await.expect("HTTP server error");
}
