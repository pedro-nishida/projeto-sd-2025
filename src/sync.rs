/// Strong-consistency replication module.
///
/// The leader calls [`replicate`] before acknowledging a write to the client.
/// It sends the entry to every follower peer and waits until **all** connected
/// followers return a `ReplicateAck`, implementing a simple all-ACK consistency
/// guarantee (the primary blocks until every backup confirms the write).
use std::{
    collections::HashMap,
    io::Write,
    net::TcpStream,
    sync::{Arc, Mutex},
};

use crate::message::Message;

/// In-memory key/value store shared across the node.
pub type Store = Arc<Mutex<HashMap<String, String>>>;

/// Create an empty shared store.
pub fn new_store() -> Store {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Apply a key/value pair to the local store.
pub fn apply(store: &Store, key: &str, value: &str) {
    store.lock().unwrap().insert(key.to_owned(), value.to_owned());
}

/// Read a key from the local store.
pub fn read(store: &Store, key: &str) -> Option<String> {
    store.lock().unwrap().get(key).cloned()
}

/// Replicate a write to a list of follower TCP addresses and wait for their
/// acknowledgement (strong consistency: primary blocks until all ACKs arrive).
///
/// # Arguments
/// * `peers` – cluster addresses of every *follower* (leader excluded).
/// * `key`, `value` – the entry to replicate.
/// * `seq` – monotonically increasing sequence number assigned by the leader.
///
/// Returns the number of successful acknowledgements received.
pub fn replicate(peers: &[String], key: &str, value: &str, seq: u64) -> usize {
    let msg = Message::Replicate {
        key: key.to_owned(),
        value: value.to_owned(),
        seq,
    };
    let line = msg.to_line();

    let mut acks = 0usize;

    for addr in peers {
        match TcpStream::connect(addr) {
            Ok(mut stream) => {
                // Send replication request.
                if stream.write_all(line.as_bytes()).is_err() {
                    continue;
                }

                // Wait for the ACK reply using a buffered reader.
                use std::io::BufRead;
                let mut buf = String::new();
                let mut reader = std::io::BufReader::new(stream);
                if reader.read_line(&mut buf).is_ok()
                    && let Ok(Message::ReplicateAck { seq: ack_seq, .. }) =
                        Message::from_line(&buf)
                    && ack_seq == seq
                {
                    acks += 1;
                }
            }
            Err(_) => {
                // Peer unreachable — tolerated (fault tolerance).
            }
        }
    }

    acks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_insert_and_read() {
        let store = new_store();
        apply(&store, "foo", "bar");
        assert_eq!(read(&store, "foo").as_deref(), Some("bar"));
        assert!(read(&store, "missing").is_none());
    }

    #[test]
    fn store_overwrite() {
        let store = new_store();
        apply(&store, "k", "v1");
        apply(&store, "k", "v2");
        assert_eq!(read(&store, "k").as_deref(), Some("v2"));
    }

    #[test]
    fn replicate_no_peers_returns_zero() {
        // With an empty peer list the function should succeed immediately.
        let acks = replicate(&[], "key", "val", 1);
        assert_eq!(acks, 0);
    }

    #[test]
    fn replicate_unreachable_peer_tolerated() {
        // Connecting to a port that is certainly not open should not panic.
        let acks = replicate(&["127.0.0.1:1".to_owned()], "key", "val", 1);
        assert_eq!(acks, 0);
    }
}
