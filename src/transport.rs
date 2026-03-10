use std::pin::Pin;

use async_stream::try_stream;
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

pub type BridgeEventStream = Pin<Box<dyn Stream<Item = Result<BridgeEvent, BridgeError>> + Send>>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CrewStreamRequest {
    pub session_id: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BridgeEvent {
    Thinking {
        iteration: u32,
    },
    TextDelta {
        text: String,
    },
    ToolStart {
        name: String,
    },
    ToolEnd {
        name: String,
        success: bool,
    },
    Response {
        iteration: u32,
    },
    CostUpdate {
        input_tokens: u32,
        output_tokens: u32,
        session_cost: Option<f64>,
    },
    StreamEnd,
    Done {
        content: String,
        input_tokens: u32,
        output_tokens: u32,
    },
    Error {
        message: String,
    },
    Raw {
        payload: Value,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("http request failed")]
    Request(#[from] reqwest::Error),
    #[error("crew status {status}: {body}")]
    HttpStatus { status: StatusCode, body: String },
    #[error("invalid SSE payload: {0}")]
    InvalidEventJson(String),
    #[error("invalid bridge event payload: {0}")]
    InvalidEventPayload(String),
}

#[async_trait]
pub trait CrewTransport: Send + Sync {
    async fn submit_stream(&self, req: CrewStreamRequest)
    -> Result<BridgeEventStream, BridgeError>;
    async fn healthcheck(&self) -> Result<(), BridgeError>;
}

/// HTTP transport for Crew's `POST /api/chat` endpoint with `stream: true`.
#[derive(Clone, Debug)]
pub struct SseHttpTransport {
    client: Client,
    base_url: Url,
    auth_token: Option<String>,
}

impl SseHttpTransport {
    pub fn new(base_url: Url) -> Self {
        Self {
            client: Client::new(),
            base_url,
            auth_token: None,
        }
    }

    pub fn with_auth_token(mut self, auth_token: impl Into<String>) -> Self {
        self.auth_token = Some(auth_token.into());
        self
    }

    fn chat_url(&self) -> Result<Url, BridgeError> {
        self.base_url
            .join("/api/chat")
            .map_err(|error| BridgeError::InvalidEventPayload(error.to_string()))
    }

    fn status_url(&self) -> Result<Url, BridgeError> {
        self.base_url
            .join("/api/status")
            .map_err(|error| BridgeError::InvalidEventPayload(error.to_string()))
    }
}

#[async_trait]
impl CrewTransport for SseHttpTransport {
    async fn submit_stream(
        &self,
        req: CrewStreamRequest,
    ) -> Result<BridgeEventStream, BridgeError> {
        let url = self.chat_url()?;
        let mut request = self.client.post(url).json(&serde_json::json!({
            "message": req.message,
            "session_id": req.session_id,
            "stream": true,
        }));
        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BridgeError::HttpStatus { status, body });
        }

        let stream = try_stream! {
            let mut bytes = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = bytes.next().await {
                let chunk = chunk?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(raw_event) = extract_next_event(&mut buffer) {
                    if let Some(data) = parse_sse_data_block(&raw_event) {
                        let payload: Value = serde_json::from_str(&data)
                            .map_err(|_| BridgeError::InvalidEventJson(data.clone()))?;
                        yield map_event(payload)?;
                    }
                }
            }

            if !buffer.trim().is_empty() {
                if let Some(data) = parse_sse_data_block(&buffer) {
                    let payload: Value = serde_json::from_str(&data)
                        .map_err(|_| BridgeError::InvalidEventJson(data.clone()))?;
                    yield map_event(payload)?;
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn healthcheck(&self) -> Result<(), BridgeError> {
        let url = self.status_url()?;
        let mut request = self.client.get(url);
        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(BridgeError::HttpStatus { status, body })
        }
    }
}

fn extract_next_event(buffer: &mut String) -> Option<String> {
    let lf = buffer.find("\n\n");
    let crlf = buffer.find("\r\n\r\n");
    let idx = match (lf, crlf) {
        (Some(lhs), Some(rhs)) => lhs.min(rhs),
        (Some(lhs), None) => lhs,
        (None, Some(rhs)) => rhs,
        (None, None) => return None,
    };

    let delimiter_len = if buffer[idx..].starts_with("\r\n\r\n") {
        4
    } else {
        2
    };
    let event = buffer[..idx].to_string();
    buffer.drain(..idx + delimiter_len);
    Some(event)
}

fn parse_sse_data_block(raw_event: &str) -> Option<String> {
    let mut data_lines = Vec::new();
    for line in raw_event.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start().to_string());
        }
    }
    if data_lines.is_empty() {
        None
    } else {
        Some(data_lines.join("\n"))
    }
}

fn map_event(payload: Value) -> Result<BridgeEvent, BridgeError> {
    let event_type = payload
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| BridgeError::InvalidEventPayload(payload.to_string()))?;

    let event = match event_type {
        "thinking" => BridgeEvent::Thinking {
            iteration: as_u32(payload.get("iteration")),
        },
        "token" => BridgeEvent::TextDelta {
            text: payload
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        "tool_start" => BridgeEvent::ToolStart {
            name: payload
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        "tool_end" => BridgeEvent::ToolEnd {
            name: payload
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            success: payload
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "response" => BridgeEvent::Response {
            iteration: as_u32(payload.get("iteration")),
        },
        "cost_update" => BridgeEvent::CostUpdate {
            input_tokens: as_u32(payload.get("input_tokens")),
            output_tokens: as_u32(payload.get("output_tokens")),
            session_cost: payload.get("session_cost").and_then(Value::as_f64),
        },
        "stream_end" => BridgeEvent::StreamEnd,
        "done" => BridgeEvent::Done {
            content: payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            input_tokens: as_u32(payload.get("input_tokens")),
            output_tokens: as_u32(payload.get("output_tokens")),
        },
        "error" => BridgeEvent::Error {
            message: payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        },
        _ => BridgeEvent::Raw { payload },
    };

    Ok(event)
}

fn as_u32(value: Option<&Value>) -> u32 {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{BridgeEvent, extract_next_event, map_event, parse_sse_data_block};

    #[test]
    fn extracts_event_blocks() {
        let mut buffer =
            "data: {\"type\":\"thinking\"}\n\ndata: {\"type\":\"stream_end\"}\n\n".to_string();
        let first = extract_next_event(&mut buffer).unwrap();
        let second = extract_next_event(&mut buffer).unwrap();

        assert_eq!(
            parse_sse_data_block(&first).unwrap(),
            "{\"type\":\"thinking\"}"
        );
        assert_eq!(
            parse_sse_data_block(&second).unwrap(),
            "{\"type\":\"stream_end\"}"
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn maps_known_event() {
        let payload = serde_json::json!({"type": "token", "text": "hello"});
        let event = map_event(payload).unwrap();
        assert_eq!(
            event,
            BridgeEvent::TextDelta {
                text: "hello".into()
            }
        );
    }
}
