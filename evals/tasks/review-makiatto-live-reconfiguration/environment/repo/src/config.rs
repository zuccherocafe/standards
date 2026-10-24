//! Checked routing configuration at the JSON boundary.

use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf};

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "u16")]
pub struct RouteWeight(u16);

#[derive(Debug, thiserror::Error)]
#[error("route weight must be positive")]
pub struct InvalidWeight;

impl RouteWeight {
    pub fn new(raw: u16) -> Result<Self, InvalidWeight> {
        if raw == 0 {
            Err(InvalidWeight)
        } else {
            Ok(Self(raw))
        }
    }

    pub fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for RouteWeight {
    type Error = InvalidWeight;

    fn try_from(raw: u16) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformConfig {
    pub wasm_path: PathBuf,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub transform: TransformConfig,
    pub weight: RouteWeight,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    pub peers: Vec<Peer>,
}

impl Config {
    pub fn from_json(input: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(input)
    }
}

pub fn select_weighted_peer(peers: &[Peer], ticket: u64) -> Option<&Peer> {
    if peers.is_empty() {
        return None;
    }
    let total: u64 = peers.iter().map(|peer| u64::from(peer.weight.get())).sum();
    let mut slot = ticket % total;
    peers.iter().find(|peer| {
        let weight = u64::from(peer.weight.get());
        if slot < weight {
            true
        } else {
            slot -= weight;
            false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_refuses_zero() {
        assert!(RouteWeight::new(0).is_err());
        assert_eq!(RouteWeight::new(3).unwrap().get(), 3);
    }

    #[test]
    fn authored_config_and_weighted_selection() {
        let config: Config = serde_json::from_str(
            r#"{
            "listen":"127.0.0.1:0",
            "peers":[{"weight":2,"transform":{"wasm_path":"shift.wasm","env":{"OFFSET":"1"}}}]
        }"#,
        )
        .unwrap();
        assert_eq!(
            select_weighted_peer(&config.peers, 5).unwrap().weight.get(),
            2
        );
        assert!(select_weighted_peer(&[], 0).is_none());
    }
}
