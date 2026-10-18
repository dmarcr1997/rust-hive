use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct OllamaClient {
    client: reqwest::Client,
    generate_url: String,
    model: String,
    num_ctx: u32,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    think: bool,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    num_ctx: u32,
    num_predict: u32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

impl OllamaClient {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let base_url = std::env::var("HIVE_OLLAMA_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());

        let model = std::env::var("HIVE_OLLAMA_MODEL")
            .unwrap_or_else(|_| "gemma4:e2b".to_string());

        let num_ctx = std::env::var("HIVE_OLLAMA_NUM_CTX")
            .unwrap_or_else(|_| "2048".to_string())
            .parse::<u32>()?;

        if num_ctx == 0 || model.trim().is_empty() {
            return Err("Ollama model must be nonempty and context must be positive".into());
        }

        let generate_url = format!(
            "{}/api/generate",
            base_url.trim_end_matches('/')
        );

        let parsed = reqwest::Url::parse(&generate_url)?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("HIVE_OLLAMA_URL must use http or https".into());
        }

        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(180))
                .build()?,
            generate_url,
            model,
            num_ctx,
        })
    }

    pub async fn generate(&self, prompt: &str) -> Result<String, reqwest::Error> {
        let response = self.client
            .post(&self.generate_url)
            .json(&GenerateRequest {
                model: &self.model,
                prompt,
                stream: false,
                think: false,
                options: GenerateOptions {
                    num_ctx: self.num_ctx,
                    num_predict: 256,
                },
            })
            .send()
            .await?
            .error_for_status()?
            .json::<GenerateResponse>()
            .await?;

        Ok(response.response)
    }
}