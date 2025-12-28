//! DHT-based peer discovery for session collaboration
//!
//! Each peer publishes their EndpointId to the DHT using a session-derived pkarr key.
//! Other peers query the DHT to discover all peers in the session.

use anyhow::Result;
use iroh::EndpointId;
use std::time::Duration;

use crate::networking::SessionId;

pub async fn publish_peer_to_dht(
    session_id: &SessionId,
    our_endpoint_id: EndpointId,
    dht_client: &pkarr::Client,
) -> Result<()> {
    use pkarr::dns::{self, rdata};
    use pkarr::dns::rdata::RData;

    let keypair = session_id.to_pkarr_keypair();
    let public_key = keypair.public_key();

    // Query DHT for existing peers in this session
    let existing_peers = match dht_client.resolve(&public_key).await {
        Some(packet) => {
            let mut peers = Vec::new();
            for rr in packet.all_resource_records() {
                if let RData::TXT(txt) = &rr.rdata {
                    if let Ok(txt_str) = String::try_from(txt.clone()) {
                        if let Some(hex) = txt_str.strip_prefix("peer=") {
                            if let Ok(bytes) = hex::decode(hex) {
                                if bytes.len() == 32 {
                                    if let Ok(endpoint_id) = EndpointId::from_bytes(&bytes.try_into().unwrap()) {
                                        // Don't include ourselves if we're already in the list
                                        if endpoint_id != our_endpoint_id {
                                            peers.push(endpoint_id);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            peers
        }
        None => Vec::new(),
    };

    // Build packet with all peers (existing + ourselves)
    let name = dns::Name::new("_peers").expect("constant");
    let mut builder = pkarr::SignedPacket::builder();

    // Add TXT record for each existing peer
    for peer in existing_peers {
        let peer_hex = hex::encode(peer.as_bytes());
        let peer_str = format!("peer={}", peer_hex);
        let mut txt = rdata::TXT::new();
        txt.add_string(&peer_str)?;
        builder = builder.txt(name.clone(), txt.into_owned(), 3600);
    }

    // Add TXT record for ourselves
    let our_hex = hex::encode(our_endpoint_id.as_bytes());
    let our_str = format!("peer={}", our_hex);
    let mut our_txt = rdata::TXT::new();
    our_txt.add_string(&our_str)?;
    builder = builder.txt(name, our_txt.into_owned(), 3600);

    // Build and sign the packet
    let signed_packet = builder.build(&keypair)?;

    // Publish to DHT
    dht_client.publish(&signed_packet, None).await?;

    tracing::info!(
        "Published peer {} to DHT for session {}",
        our_endpoint_id.fmt_short(),
        session_id.to_code()
    );

    Ok(())
}

pub async fn discover_peers_from_dht(
    session_id: &SessionId,
    dht_client: &pkarr::Client,
) -> Result<Vec<EndpointId>> {
    use pkarr::dns::rdata::RData;
    
    let keypair = session_id.to_pkarr_keypair();
    let public_key = keypair.public_key();
    
    // Query DHT for the session's public key
    let signed_packet = match dht_client.resolve(&public_key).await {
        Some(packet) => packet,
        None => {
            tracing::debug!("No peers found in DHT for session {}", session_id.to_code());
            return Ok(vec![]);
        }
    };
    
    // Parse TXT records to extract peer endpoint IDs
    let mut peers = Vec::new();
    
    for rr in signed_packet.all_resource_records() {
        if let RData::TXT(txt) = &rr.rdata {
            // Try to parse as a String
            if let Ok(txt_str) = String::try_from(txt.clone()) {
                // Parse "peer=<hex_endpoint_id>"
                if let Some(hex) = txt_str.strip_prefix("peer=") {
                    if let Ok(bytes) = hex::decode(hex) {
                        if bytes.len() == 32 {
                            if let Ok(endpoint_id) = EndpointId::from_bytes(&bytes.try_into().unwrap()) {
                                peers.push(endpoint_id);
                            }
                        }
                    }
                }
            }
        }
    }
    
    tracing::info!(
        "Discovered {} peers from DHT for session {}",
        peers.len(),
        session_id.to_code()
    );
    
    Ok(peers)
}

/// Periodically republishes our presence to the DHT
///
/// Should be called in a background task to maintain our DHT presence.
/// Republishes every 30 minutes (well before the 1-hour TTL expires).
pub async fn maintain_dht_presence(
    session_id: SessionId,
    our_endpoint_id: EndpointId,
    dht_client: pkarr::Client,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(30 * 60)); // 30 minutes

    loop {
        interval.tick().await;

        if let Err(e) = publish_peer_to_dht(&session_id, our_endpoint_id, &dht_client).await {
            tracing::warn!("Failed to republish to DHT: {}", e);
        }
    }
}
