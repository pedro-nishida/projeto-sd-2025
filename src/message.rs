use serde::{Deserialize, Serialize};

/// Messages exchanged between cluster nodes over TCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// Periodic liveness signal broadcast by the current leader.
    Heartbeat { leader_id: u64, term: u64 },

    /// Bully-election step 1: "I want to be leader; higher IDs respond."
    Election { candidate_id: u64 },

    /// Bully-election step 2: "I am alive and have a higher ID than you."
    Ok { from_id: u64 },

    /// Bully-election step 3: announce the new leader to every node.
    Coordinator { leader_id: u64 },

    /// Replication request sent from leader to followers (strong consistency).
    Replicate { key: String, value: String, seq: u64 },

    /// Acknowledgement sent by a follower after persisting a replicated entry.
    ReplicateAck { seq: u64, from_id: u64 },
}

impl Message {
    /// Serialize the message to a newline-terminated JSON string.
    pub fn to_line(&self) -> String {
        let mut s = serde_json::to_string(self).expect("serialization never fails");
        s.push('\n');
        s
    }

    /// Deserialize a message from a JSON line (trailing newline is ignored).
    pub fn from_line(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_heartbeat() {
        let msg = Message::Heartbeat { leader_id: 3, term: 7 };
        let line = msg.to_line();
        let decoded = Message::from_line(&line).unwrap();
        assert!(matches!(decoded, Message::Heartbeat { leader_id: 3, term: 7 }));
    }

    #[test]
    fn roundtrip_election() {
        let msg = Message::Election { candidate_id: 2 };
        let decoded = Message::from_line(&msg.to_line()).unwrap();
        assert!(matches!(decoded, Message::Election { candidate_id: 2 }));
    }

    #[test]
    fn roundtrip_coordinator() {
        let msg = Message::Coordinator { leader_id: 5 };
        let decoded = Message::from_line(&msg.to_line()).unwrap();
        assert!(matches!(decoded, Message::Coordinator { leader_id: 5 }));
    }

    #[test]
    fn roundtrip_replicate_ack() {
        let msg = Message::ReplicateAck { seq: 42, from_id: 1 };
        let decoded = Message::from_line(&msg.to_line()).unwrap();
        assert!(matches!(decoded, Message::ReplicateAck { seq: 42, from_id: 1 }));
    }
}
