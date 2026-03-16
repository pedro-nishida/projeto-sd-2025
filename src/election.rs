/// Bully leader-election algorithm.
///
/// Rules:
/// 1. Any node that detects the absence of a leader (heartbeat timeout) starts
///    an election by calling [`start_election`].
/// 2. The candidate sends `Election` messages to every peer with a *higher* ID.
///    * If none respond with `Ok` within [`ELECTION_TIMEOUT`], the candidate
///      promotes itself to coordinator and broadcasts `Coordinator`.
///    * If a higher-ID peer responds, the candidate yields and waits for a
///      `Coordinator` announcement.
/// 3. A node that receives an `Election` message from a lower-ID peer always
///    replies with `Ok` and starts its own election (if not already running).
use std::{
    io::{BufRead, Write},
    net::TcpStream,
    time::Duration,
};

use crate::message::Message;

/// Time to wait for an `Ok` reply after sending `Election` messages.
pub const ELECTION_TIMEOUT: Duration = Duration::from_secs(3);

/// Send an `Election` message to a single peer and return `true` if that peer
/// replied with `Ok` (meaning a higher-ID node is alive and will take over).
pub fn send_election(peer_addr: &str, candidate_id: u64) -> bool {
    let Ok(mut stream) = TcpStream::connect(peer_addr) else {
        return false;
    };
    stream
        .set_read_timeout(Some(ELECTION_TIMEOUT))
        .unwrap_or(());

    let msg = Message::Election { candidate_id };
    if stream.write_all(msg.to_line().as_bytes()).is_err() {
        return false;
    }

    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(stream);
    if reader.read_line(&mut buf).is_ok() {
        matches!(Message::from_line(&buf), Ok(Message::Ok { .. }))
    } else {
        false
    }
}

/// Broadcast a `Coordinator` announcement to all peers (best-effort).
pub fn broadcast_coordinator(peers: &[String], leader_id: u64) {
    let line = Message::Coordinator { leader_id }.to_line();
    for addr in peers {
        if let Ok(mut stream) = TcpStream::connect(addr) {
            let _ = stream.write_all(line.as_bytes());
        }
    }
}

/// Run the bully election algorithm from the perspective of `my_id`.
///
/// * `peers` — `(id, cluster_addr)` pairs for *every* peer (all IDs).
///
/// Returns the ID of the elected leader (may be `my_id` if this node wins).
pub fn start_election(my_id: u64, peers: &[(u64, String)]) -> u64 {
    // Phase 1: send Election to all peers with a higher ID.
    let higher_peers: Vec<&(u64, String)> = peers.iter().filter(|(id, _)| *id > my_id).collect();

    let got_ok = if higher_peers.is_empty() {
        false
    } else {
        higher_peers
            .iter()
            .any(|(_, addr)| send_election(addr, my_id))
    };

    if got_ok {
        // A higher-ID peer is alive; it will eventually announce itself.
        // Return 0 to signal "winner not determined yet by us".
        0
    } else {
        // No higher-ID peer responded — this node becomes coordinator.
        let all_addrs: Vec<String> = peers.iter().map(|(_, a)| a.clone()).collect();
        broadcast_coordinator(&all_addrs, my_id);
        my_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highest_id_wins_when_no_peers() {
        // Node 5 with no peers should immediately win.
        let winner = start_election(5, &[]);
        assert_eq!(winner, 5);
    }

    #[test]
    fn no_higher_peers_reachable_self_wins() {
        // Higher-ID peer at an unreachable address — node should self-elect.
        let peers = vec![(10u64, "127.0.0.1:1".to_owned())];
        let winner = start_election(3, &peers);
        assert_eq!(winner, 3);
    }
}
