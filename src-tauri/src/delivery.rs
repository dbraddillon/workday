//! Delivery layer. `SlackDeliveryService` accepts *final rendered text* and owns
//! no content-selection logic (the doc's rule). v1 = copy-to-clipboard, which is
//! handled on the frontend via the clipboard plugin; direct posting is stubbed
//! behind this seam so a webhook or bot token can be added later without callers
//! changing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeliveryMethod {
    /// Always available. The actual clipboard write happens in the webview via
    /// the clipboard-manager plugin; the backend just records the outcome.
    Clipboard,
    /// Not implemented in v1 — reserved so the interface is stable.
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryResult {
    pub status: String,      // "copied" | "sent" | "unsupported"
    pub destination: Option<String>,
    pub message: Option<String>,
}

/// The one seam for getting text into Slack. Deliberately tiny.
pub trait SlackDeliveryService {
    fn deliver(&self, text: &str, method: DeliveryMethod) -> DeliveryResult;
}

/// v1 service. Clipboard is a no-op here (frontend does the write); webhook is
/// explicitly unsupported until implemented.
pub struct V1DeliveryService;

impl SlackDeliveryService for V1DeliveryService {
    fn deliver(&self, _text: &str, method: DeliveryMethod) -> DeliveryResult {
        match method {
            DeliveryMethod::Clipboard => DeliveryResult {
                status: "copied".into(),
                destination: Some("clipboard".into()),
                message: None,
            },
            DeliveryMethod::Webhook => DeliveryResult {
                status: "unsupported".into(),
                destination: None,
                message: Some("Direct Slack posting is not enabled in v1 — use Copy.".into()),
            },
        }
    }
}
