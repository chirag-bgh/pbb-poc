use futures_util::stream::StreamExt;
use log::{info, warn};
use mev_share_sse::{
    client::{EventStream, SseError},
    EventClient,
};
use reth_rpc_types::beacon::events::PayloadAttributesEvent;
use std::net::{IpAddr, Ipv4Addr};

#[derive(Debug, Clone, clap::Parser)]
pub struct BeaconEventsConfig {
    /// Beacon Node http server address
    #[arg(long = "cl.addr", default_value_t = IpAddr::V4(Ipv4Addr::LOCALHOST))]
    pub cl_addr: IpAddr,
    /// Beacon Node http server port to listen on
    #[arg(long = "cl.port", default_value_t = 5052)]
    pub cl_port: u16,
}

impl BeaconEventsConfig {
    /// Creates a new instance of the beacon events service
    pub fn new() -> Self {
        Self {
            cl_addr: Ipv4Addr::LOCALHOST.into(),
            cl_port: 5052,
        }
    }

    /// Returns the http url of the beacon node
    pub fn http_base_url(&self) -> String {
        format!("http://{}:{}", self.cl_addr, self.cl_port)
    }

    /// Returns the URL to the events endpoint
    pub fn events_url(&self) -> String {
        format!("{}/eth/v1/events", self.http_base_url())
    }

    /// Service that subscribes to beacon chain payload attributes events
    pub async fn run(self) -> Result<PayloadAttributesEvent, SseError> {
        let client = EventClient::default();
        let mut subscription = self.new_payload_attributes_subscription(&client).await;
        let event = subscription.next().await.unwrap();
        event
    }

    // It can take a bit until the CL endpoint is live so we retry a few times
    pub async fn new_payload_attributes_subscription(
        &self,
        client: &EventClient,
    ) -> EventStream<PayloadAttributesEvent> {
        let payloads_url = format!("{}?topics=payload_attributes", self.events_url());
        loop {
            match client.subscribe(&payloads_url).await {
                Ok(subscription) => return subscription,
                Err(err) => {
                    warn!("Failed to subscribe to payload attributes events: {:?}\nRetrying in 5 seconds...", err);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }
}
