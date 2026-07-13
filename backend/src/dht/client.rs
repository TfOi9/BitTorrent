use tonic::transport::Channel;
use crate::core::error::{BError, Result};
use crate::core::types::{InfoHash, PeerAddr};
use std::net::IpAddr;

use crate::dht_proto::dht_service_client::DhtServiceClient;
use crate::dht_proto::*;

pub struct DhtClient {
    inner: DhtServiceClient<Channel>
}

impl DhtClient {
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())
            .map_err(|e| BError::Network(format!("invalid gRPC endpoint: {}", e)))?
            .connect()
            .await
            .map_err(|e| BError::Network(format!("failed to connect to DHT sidecar: {}", e)))?;

        let inner = DhtServiceClient::new(channel);

        Ok(Self{inner})
    }

    pub async fn get_peers(&mut self, info_hash: &InfoHash) -> Result<Vec<PeerAddr>> {
        let request = tonic::Request::new(GetPeersRequest{
            info_hash: info_hash.as_slice().to_vec()
        });

        let response = self
            .inner
            .get_peers(request)
            .await
            .map_err(|e| BError::Grpc(e))?;

        let resp = response.into_inner();

        if !resp.found {
            return Ok(Vec::new());
        }

        let peers: Vec<PeerAddr> = resp
            .peers
            .into_iter()
            .filter_map(|p| {
                let ip: IpAddr = p.ip.parse().ok()?;
                if p.port == 0 || p.port > 65535 {
                    return None;
                }
                Some(PeerAddr::new(ip, p.port as u16))
            })
            .collect();

        Ok(peers)
    }

    pub async fn announce_peer(
        &mut self,
        info_hash: &InfoHash,
        peer: &PeerAddr,
    ) -> Result<bool> {
        let request = tonic::Request::new(AnnouncePeerRequest {
            info_hash: info_hash.as_slice().to_vec(),
            peer: Some(Peer {
                ip: peer.ip.to_string(),
                port: peer.port as u32,
            }),
        });

        let response = self
            .inner
            .announce_peer(request)
            .await
            .map_err(|e| BError::Grpc(e))?;

        Ok(response.into_inner().success)
    }

    pub async fn health_check(&mut self) -> Result<bool> {
        let request = tonic::Request::new(PutRequest {
            key: "__health_check__".into(),
            value: "ping".into(),
        });
        match self.inner.put(request).await {
            Ok(_) => Ok(true),
            Err(e) => Err(BError::Network(format!("health check failed: {}", e))),
        }
    }
}