pub fn normalize_peer(peer: &str, default_port: &str) -> String {
    let peer = peer
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    if peer.is_empty() {
        return String::new();
    }
    if peer.contains(':') {
        peer.to_string()
    } else {
        format!("{peer}:{default_port}")
    }
}

pub fn push_peer_values(peers: &mut Vec<String>, value: &str, default_port: &str) {
    for peer in value.split(',') {
        let peer = normalize_peer(peer, default_port);
        if !peer.is_empty() {
            peers.push(peer);
        }
    }
}

pub fn sort_dedup(peers: &mut Vec<String>) {
    peers.sort();
    peers.dedup();
}
