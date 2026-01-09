//! HTTP client for the reasoner gateway.

use crate::{ProposeRequest, Reasoner, ReasonerConfig};
use async_trait::async_trait;
use aura_core::ProposalSet;
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, error, instrument, warn};

/// HTTP-based reasoner client.
pub struct HttpReasoner {
    client: Client,
    config: ReasonerConfig,
}

impl HttpReasoner {
    /// Create a new HTTP reasoner client.
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be created.
    pub fn new(config: ReasonerConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()?;

        Ok(Self { client, config })
    }

    /// Create with default config.
    ///
    /// # Errors
    /// Returns error if the HTTP client cannot be created.
    pub fn with_defaults() -> anyhow::Result<Self> {
        Self::new(ReasonerConfig::default())
    }
}

#[async_trait]
impl Reasoner for HttpReasoner {
    #[instrument(skip(self, request), fields(agent_id = %request.agent_id))]
    async fn propose(&self, request: ProposeRequest) -> anyhow::Result<ProposalSet> {
        let url = format!("{}/propose", self.config.gateway_url);
        debug!(%url, "Sending propose request");

        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                warn!(attempt, "Retrying propose request");
            }

            match self.client.post(&url).json(&request).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let proposals: ProposalSet = response.json().await?;
                        debug!(count = proposals.proposals.len(), "Received proposals");
                        return Ok(proposals);
                    }

                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    error!(%status, %body, "Reasoner returned error");
                    last_error = Some(anyhow::anyhow!("Reasoner error: {status} - {body}"));
                }
                Err(e) => {
                    error!(error = %e, "Request failed");
                    last_error = Some(e.into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Reasoner request failed")))
    }

    async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.config.gateway_url);

        self.client
            .get(&url)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }
}
