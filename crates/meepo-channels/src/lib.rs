//! Channel adapters and message bus for meepo
//!
//! This crate provides the message routing infrastructure and channel-specific
//! adapters for Discord, iMessage, and Slack.

pub mod alexa;
pub mod bus;
#[cfg(target_os = "macos")]
pub mod contacts;
pub mod discord;
#[cfg(target_os = "macos")]
pub mod email;
#[cfg(target_os = "macos")]
pub mod imessage;
#[cfg(target_os = "macos")]
pub mod notes;
pub mod rate_limit;
#[cfg(target_os = "macos")]
pub mod reminders;
pub mod slack;

// Re-export main types
pub use alexa::AlexaChannel;
pub use bus::{MessageBus, MessageChannel};
#[cfg(target_os = "macos")]
pub use contacts::ContactsChannel;
pub use discord::DiscordChannel;
#[cfg(target_os = "macos")]
pub use email::EmailChannel;
#[cfg(target_os = "macos")]
pub use imessage::IMessageChannel;
#[cfg(target_os = "macos")]
pub use notes::NotesChannel;
pub use rate_limit::RateLimiter;
#[cfg(target_os = "macos")]
pub use reminders::RemindersChannel;
pub use slack::SlackChannel;
