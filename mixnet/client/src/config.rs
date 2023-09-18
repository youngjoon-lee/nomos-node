use std::net::SocketAddr;

use futures::{stream, StreamExt};
use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};

use crate::{receiver::Receiver, MessageStream, MixnetClientError};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
    pub connection_pool_size: usize,
    pub max_retries: usize,
    pub retry_delay: std::time::Duration,
}

impl MixnetClientConfig {
    /// Creates a new `MixnetClientConfig` with default values.
    pub fn new(mode: MixnetClientMode, topology: MixnetTopology) -> Self {
        Self {
            mode,
            topology,
            connection_pool_size: 256,
            max_retries: 3,
            retry_delay: std::time::Duration::from_secs(5),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver(SocketAddr),
}

impl MixnetClientMode {
    pub(crate) async fn run(&self) -> Result<MessageStream, MixnetClientError> {
        match self {
            Self::Sender => Ok(stream::empty().boxed()),
            Self::SenderReceiver(node_address) => {
                Ok(Receiver::new(*node_address).run().await?.boxed())
            }
        }
    }
}