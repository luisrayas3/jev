use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::plan::logs_dir;
use crate::prompt::SYSTEM_PROMPT;

#[derive(Serialize)]
pub(crate) struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

pub(crate) fn parse_response(raw: &str) -> Result<String> {
    extract_fenced(raw, "rust")
        .context("response missing ```rust``` fenced block")
}

fn extract_fenced(text: &str, lang: &str) -> Option<String> {
    let opener = format!("```{lang}");
    let start = text.find(&opener)?;
    let after_opener = start + opener.len();
    let rest = &text[after_opener..];
    let end = rest.find("```")?;
    Some(rest[..end].trim().to_string())
}

pub(crate) fn log_exchange(
    id: &str,
    messages: &[Message],
    label: &str,
) -> Result<()> {
    let dir = logs_dir().join(id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{label}.json"));
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "system": SYSTEM_PROMPT,
        "messages": messages,
    }))?;
    std::fs::write(&path, &json)?;
    eprintln!("  log: {}", path.display());
    Ok(())
}

pub(crate) async fn call_llm_raw(
    client: &reqwest::Client,
    api_key: &str,
    messages: &[Message],
) -> Result<String> {
    let request = ApiRequest {
        model: "claude-sonnet-4-20250514".to_string(),
        max_tokens: 4096,
        system: SYSTEM_PROMPT.to_string(),
        messages: messages.to_vec(),
    };

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .context("API request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("API error {status}: {body}");
    }

    let api_resp: ApiResponse =
        resp.json().await.context("parsing API response")?;
    let code = api_resp
        .content
        .into_iter()
        .filter_map(|b| b.text)
        .collect::<Vec<_>>()
        .join("");

    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_fenced_basic() {
        let text = "before\n```rust\nfn main() {}\n```\nafter";
        let result = extract_fenced(text, "rust").unwrap();
        assert_eq!(result, "fn main() {}");
    }

    #[test]
    fn parse_response_missing_rust() {
        let raw = "no code here";
        let err = parse_response(raw).unwrap_err();
        assert!(
            format!("{err}").contains("rust"),
            "error should mention rust: {err}"
        );
    }

    #[test]
    fn parse_response_single_block() {
        let raw = r#"```rust
use jevs::{File, Labeled};
use jevs::label::*;

#[jevs::needs(
    f: File<Private, Me> = "./data",
)]
pub async fn root(
    needs: &mut Needs,
) -> anyhow::Result<()> {
    Ok(())
}
```
"#;
        let code = parse_response(raw).unwrap();
        assert!(code.contains("#[jevs::needs("));
        assert!(code.contains("pub async fn root"));
    }
}
