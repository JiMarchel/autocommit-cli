//! Minimal blocking client for the Gemini `generateContent` endpoint.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use std::thread::sleep;
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
pub const DEFAULT_MODEL: &str = "gemini-3.1-flash-lite";

pub struct Client {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub retry_delay: Duration,
    agent: ureq::Agent,
}

enum Failure {
    /// Worth one more try (429, 5xx, network).
    Retryable(anyhow::Error),
    Fatal(anyhow::Error),
}

impl Client {
    pub fn new(base_url: String, model: String, api_key: String, retry_delay: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .new_agent();
        Client {
            base_url: base_url.trim_end_matches('/').to_string(),
            model,
            api_key,
            retry_delay,
            agent,
        }
    }

    /// Send one prompt; retry once on rate limit / server / network errors.
    pub fn generate(&self, prompt: &str) -> Result<String> {
        match self.attempt(prompt) {
            Ok(text) => Ok(text),
            Err(Failure::Fatal(e)) => Err(e),
            Err(Failure::Retryable(first)) => {
                eprintln!("acm: {first:#}; retrying in {:?}", self.retry_delay);
                sleep(self.retry_delay);
                self.attempt(prompt).map_err(|f| match f {
                    Failure::Retryable(e) | Failure::Fatal(e) => e,
                })
            }
        }
    }

    fn attempt(&self, prompt: &str) -> Result<String, Failure> {
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
        );
        let body = json!({
            "contents": [{"role": "user", "parts": [{"text": prompt}]}],
            // The cap covers hidden "thinking" tokens too; keep it roomy so a
            // thinking model still has budget left for the actual message.
            // (No thinkingConfig: accepted values differ between model families.)
            "generationConfig": {
                "temperature": 0.2,
                "maxOutputTokens": 2048,
            },
        });
        let mut resp = self
            .agent
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .send_json(&body)
            .map_err(|e| Failure::Retryable(anyhow!("request to Gemini failed: {e}")))?;

        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_to_string()
            .map_err(|e| Failure::Retryable(anyhow!("reading Gemini response failed: {e}")))?;

        if status != 200 {
            let detail = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
                .unwrap_or_else(|| text.chars().take(300).collect());
            let err = anyhow!("Gemini API returned HTTP {status}: {detail}");
            return Err(if status == 429 || status >= 500 {
                Failure::Retryable(err)
            } else {
                Failure::Fatal(err)
            });
        }
        extract_text(&text).map_err(Failure::Fatal)
    }
}

fn extract_text(body: &str) -> Result<String> {
    let v: Value = serde_json::from_str(body).context("Gemini returned invalid JSON")?;
    let candidate = &v["candidates"][0];
    let text: String = candidate["content"]["parts"]
        .as_array()
        .map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect())
        .unwrap_or_default();
    if text.trim().is_empty() {
        let reason = candidate["finishReason"]
            .as_str()
            .or_else(|| v["promptFeedback"]["blockReason"].as_str())
            .unwrap_or("unknown");
        bail!("Gemini returned no text (reason: {reason})");
    }
    Ok(text)
}
