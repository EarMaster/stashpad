// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.

//! Requests to an OpenAI-compatible AI provider, made from here rather than the webview.
//!
//! The provider calls used to be `fetch`es in the frontend, which meant the API key had to
//! be handed to the webview - and that any script running there could read it, or send it
//! anywhere, since the page's `connect-src` had to be `*` for providers on any host. Now the
//! key never leaves the backend: [`ai_enhance`] uses the saved configuration as it is, and
//! [`ai_test_connection`] only ever pairs the saved key with the saved endpoint.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::SettingsState;
use crate::uierror::UiError;

const AI_TIMEOUT: Duration = Duration::from_secs(120);
const AI_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    max_tokens: u32,
    stream: bool,
}

fn chat_url(endpoint: &str) -> String {
    format!("{}/chat/completions", endpoint.trim_end_matches('/'))
}

/// Whether two endpoints are the same provider, ignoring a trailing slash.
pub fn same_endpoint(a: &str, b: &str) -> bool {
    a.trim().trim_end_matches('/') == b.trim().trim_end_matches('/')
}

async fn post_chat(
    endpoint: &str,
    api_key: &str,
    request: &ChatRequest<'_>,
) -> Result<serde_json::Value, UiError> {
    let client = reqwest::Client::builder()
        .connect_timeout(AI_CONNECT_TIMEOUT)
        .timeout(AI_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let mut builder = client.post(chat_url(endpoint)).json(request);
    if !api_key.is_empty() {
        builder = builder.bearer_auth(api_key);
    }
    let response = builder
        .send()
        .await
        .map_err(|e| format!("Could not reach the AI provider: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(300).collect();
        return Err(format!("{}: {}", status.as_u16(), snippet).into());
    }
    response
        .json()
        .await
        .map_err(|e| format!("The AI provider sent an unreadable answer: {}", e).into())
}

/// Improve a stash's text with the saved provider. The frontend supplies the prompt and the
/// text; the endpoint, model and key come from the saved settings only.
#[tauri::command]
pub async fn ai_enhance(
    settings_state: State<'_, Arc<SettingsState>>,
    system_prompt: String,
    user_content: String,
) -> Result<String, UiError> {
    let config = settings_state
        .lock_settings()
        .ai_config
        .clone()
        .ok_or("AI configuration is missing")?;
    if !config.enabled {
        return Err("AI configuration is disabled".into());
    }
    if config.endpoint.trim().is_empty() || config.model.trim().is_empty() {
        return Err("AI configuration is incomplete".into());
    }

    let request = ChatRequest {
        model: &config.model,
        messages: vec![
            ChatMessage {
                role: "system",
                content: &system_prompt,
            },
            ChatMessage {
                role: "user",
                content: &user_content,
            },
        ],
        temperature: Some(0.3),
        max_tokens: 1024,
        stream: false,
    };
    let data = post_chat(&config.endpoint, &config.api_key, &request).await?;
    data["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "No content in API response".into())
}

/// The settings a connection test runs against: what the form currently shows, which may not
/// be saved yet.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTest {
    pub endpoint: String,
    pub model: String,
    /// A key typed into the form. Empty means "the saved one" - which is only used when the
    /// endpoint is the saved endpoint, so a test can never carry the saved key elsewhere.
    #[serde(default)]
    pub api_key: String,
}

#[tauri::command]
pub async fn ai_test_connection(
    settings_state: State<'_, Arc<SettingsState>>,
    test: ConnectionTest,
) -> Result<(), UiError> {
    if test.endpoint.trim().is_empty() || test.model.trim().is_empty() {
        return Err("Configuration is incomplete".into());
    }
    let api_key = if test.api_key.is_empty() {
        let saved = settings_state.lock_settings().ai_config.clone();
        match saved {
            Some(saved) if same_endpoint(&saved.endpoint, &test.endpoint) => saved.api_key,
            _ => String::new(),
        }
    } else {
        test.api_key
    };

    let request = ChatRequest {
        model: &test.model,
        messages: vec![
            ChatMessage {
                role: "system",
                content: "You are a connection tester.",
            },
            ChatMessage {
                role: "user",
                content: "Say \"OK\" to confirm the connection works.",
            },
        ],
        temperature: None,
        max_tokens: 10,
        stream: false,
    };
    post_chat(&test.endpoint, &api_key, &request)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_compare_without_a_trailing_slash() {
        assert!(same_endpoint(
            "https://api.openai.com/v1/",
            "https://api.openai.com/v1"
        ));
        assert!(!same_endpoint(
            "https://api.openai.com/v1",
            "https://evil.example/v1"
        ));
    }

    #[test]
    fn the_chat_url_is_built_once() {
        assert_eq!(
            chat_url("http://localhost:11434/v1/"),
            "http://localhost:11434/v1/chat/completions"
        );
    }
}
