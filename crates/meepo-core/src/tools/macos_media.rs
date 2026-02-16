//! macOS media tools — photos, audio recording, TTS, OCR

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::{ToolHandler, json_schema};
use crate::platform::{MediaProvider, PhotosProvider};

pub struct SearchPhotosTool {
    provider: Box<dyn PhotosProvider>,
}

impl SearchPhotosTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_photos_provider()
                .expect("Photos provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for SearchPhotosTool {
    fn name(&self) -> &str {
        "search_photos"
    }

    fn description(&self) -> &str {
        "Search Apple Photos by keyword (e.g., 'beach', 'dog', 'sunset')."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "query": {
                    "type": "string",
                    "description": "Search keyword"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum results (default: 10, max: 50)"
                }
            }),
            vec!["query"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
        debug!("Searching photos: {}", query);
        self.provider.search_photos(query, limit).await
    }
}

pub struct ExportPhotosTool {
    provider: Box<dyn PhotosProvider>,
}

impl ExportPhotosTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_photos_provider()
                .expect("Photos provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for ExportPhotosTool {
    fn name(&self) -> &str {
        "export_photos"
    }

    fn description(&self) -> &str {
        "Export photos matching a search query to a destination folder."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "query": {
                    "type": "string",
                    "description": "Search keyword to match photos"
                },
                "destination": {
                    "type": "string",
                    "description": "Absolute path to export destination folder"
                },
                "limit": {
                    "type": "number",
                    "description": "Maximum photos to export (default: 10, max: 50)"
                }
            }),
            vec!["query", "destination"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let destination = input
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'destination' parameter"))?;
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
        debug!("Exporting photos matching '{}' to {}", query, destination);
        self.provider.export_photos(query, destination, limit).await
    }
}

pub struct RecordAudioTool {
    provider: Box<dyn MediaProvider>,
}

impl RecordAudioTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_media_provider()
                .expect("Media provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for RecordAudioTool {
    fn name(&self) -> &str {
        "record_audio"
    }

    fn description(&self) -> &str {
        "Record audio from the microphone for a specified duration."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "duration_secs": {
                    "type": "number",
                    "description": "Recording duration in seconds (max: 300)"
                },
                "output_path": {
                    "type": "string",
                    "description": "Output file path (default: /tmp/meepo-recording-{timestamp}.m4a)"
                }
            }),
            vec!["duration_secs"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let duration = input
            .get("duration_secs")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing 'duration_secs' parameter"))?;
        let output_path = input.get("output_path").and_then(|v| v.as_str());
        debug!("Recording audio for {}s", duration);
        self.provider.record_audio(duration, output_path).await
    }
}

pub struct TextToSpeechTool {
    provider: Box<dyn MediaProvider>,
}

impl TextToSpeechTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_media_provider()
                .expect("Media provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for TextToSpeechTool {
    fn name(&self) -> &str {
        "text_to_speech"
    }

    fn description(&self) -> &str {
        "Speak text aloud using macOS text-to-speech (say command)."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "text": {
                    "type": "string",
                    "description": "Text to speak"
                },
                "voice": {
                    "type": "string",
                    "description": "Voice name (e.g., 'Samantha', 'Alex', 'Daniel'). Omit for default."
                }
            }),
            vec!["text"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let text = input
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;
        let voice = input.get("voice").and_then(|v| v.as_str());
        debug!("Text to speech ({} chars)", text.len());
        self.provider.text_to_speech(text, voice).await
    }
}

pub struct OcrImageTool {
    provider: Box<dyn MediaProvider>,
}

impl OcrImageTool {
    pub fn new() -> Self {
        Self {
            provider: crate::platform::create_media_provider()
                .expect("Media provider not available on this platform"),
        }
    }
}

#[async_trait]
impl ToolHandler for OcrImageTool {
    fn name(&self) -> &str {
        "ocr_image"
    }

    fn description(&self) -> &str {
        "Extract text from an image using macOS Vision framework OCR."
    }

    fn input_schema(&self) -> Value {
        json_schema(
            serde_json::json!({
                "image_path": {
                    "type": "string",
                    "description": "Absolute path to the image file"
                }
            }),
            vec!["image_path"],
        )
    }

    async fn execute(&self, input: Value) -> Result<String> {
        let image_path = input
            .get("image_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'image_path' parameter"))?;
        debug!("OCR on: {}", image_path);
        self.provider.ocr_image(image_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolHandler;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_search_photos_schema() {
        let tool = SearchPhotosTool::new();
        assert_eq!(tool.name(), "search_photos");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_text_to_speech_schema() {
        let tool = TextToSpeechTool::new();
        assert_eq!(tool.name(), "text_to_speech");
        let schema = tool.input_schema();
        let required: Vec<String> = serde_json::from_value(
            schema.get("required").cloned().unwrap_or(serde_json::json!([])),
        )
        .unwrap_or_default();
        assert!(required.contains(&"text".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn test_text_to_speech_missing_text() {
        let tool = TextToSpeechTool::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_ocr_image_schema() {
        let tool = OcrImageTool::new();
        assert_eq!(tool.name(), "ocr_image");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_record_audio_schema() {
        let tool = RecordAudioTool::new();
        assert_eq!(tool.name(), "record_audio");
    }
}
