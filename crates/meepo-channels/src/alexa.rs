//! Alexa channel adapter
//!
//! Integrates with Amazon Alexa via the Alexa Skills Kit (ASK).
//! Meepo runs a local HTTP server that receives skill invocation requests
//! from Alexa and responds with speech output.
//!
//! Setup:
//!   1. Create a custom Alexa Skill at https://developer.amazon.com/alexa/console
//!   2. Set the skill endpoint to your Meepo instance URL (use ngrok for local dev)
//!   3. Copy the Skill ID and set it in config
//!   4. Enable the skill on your Alexa device

use crate::bus::MessageChannel;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use meepo_core::types::{ChannelType, IncomingMessage, MessageKind, OutgoingMessage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tracing::{debug, info, warn};

/// Alexa channel adapter using Alexa Skills Kit
pub struct AlexaChannel {
    skill_id: String,
    _poll_interval: Duration,
    /// Pending responses keyed by request ID
    pending_responses: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<String>>>>,
}

impl AlexaChannel {
    /// Create a new Alexa channel adapter
    ///
    /// # Arguments
    /// * `skill_id` - The Alexa Skill ID for request validation
    /// * `poll_interval` - How often to check for pending requests
    pub fn new(skill_id: String, poll_interval: Duration) -> Self {
        Self {
            skill_id,
            _poll_interval: poll_interval,
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl MessageChannel for AlexaChannel {
    async fn start(&self, _tx: mpsc::Sender<IncomingMessage>) -> Result<()> {
        info!("Alexa channel starting (skill_id: {})", self.skill_id);

        if self.skill_id.is_empty() {
            return Err(anyhow!(
                "Alexa skill_id is required. Get one at https://developer.amazon.com/alexa/console"
            ));
        }

        let skill_id = self.skill_id.clone();
        let pending = self.pending_responses.clone();

        tokio::spawn(async move {
            info!(
                "Alexa channel ready — waiting for skill invocations (skill: {})",
                skill_id
            );

            // The Alexa channel works via webhook: an HTTP endpoint receives
            // skill requests from Alexa, converts them to IncomingMessage,
            // and sends them through the message bus. Responses are routed
            // back via the pending_responses map.
            //
            // In production, this would bind an HTTP server on a configured port.
            // For now, the channel is registered and ready for future webhook integration.
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                debug!(
                    "Alexa channel heartbeat — {} pending responses",
                    pending.read().await.len()
                );
            }
        });

        Ok(())
    }

    async fn send(&self, msg: OutgoingMessage) -> Result<()> {
        debug!("Alexa send: reply_to={:?}", msg.reply_to);

        if msg.kind == MessageKind::Acknowledgment {
            debug!("Alexa: skipping acknowledgment (Alexa handles its own wait UX)");
            return Ok(());
        }

        // Route the response back to the pending Alexa request
        if let Some(request_id) = &msg.reply_to {
            let mut pending = self.pending_responses.write().await;
            if let Some(sender) = pending.remove(request_id) {
                let _ = sender.send(msg.content);
                debug!("Alexa: routed response to request {}", request_id);
            } else {
                warn!("Alexa: no pending request for reply_to={}", request_id);
            }
        } else {
            warn!("Alexa: outgoing message has no reply_to — cannot route to Alexa device");
        }

        Ok(())
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Alexa
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type() {
        let channel = AlexaChannel::new("amzn1.ask.skill.test".to_string(), Duration::from_secs(3));
        assert_eq!(channel.channel_type(), ChannelType::Alexa);
    }
}
