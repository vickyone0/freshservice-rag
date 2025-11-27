use anyhow::Result;
use serde_json::json;

pub struct LlmClient {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl LlmClient {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            client: reqwest::Client::new(),
        }
    }
    
    pub async fn generate_answer(&self, query: &str, context: &str) -> Result<String> {
        let prompt = format!(
            "You are a helpful assistant for Freshservice API documentation. \
            Use the following context to answer the user's question. \
            If the context doesn't contain the answer, say so.\n\n\
            Context:\n{}\n\n\
            Question: {}\n\n\
            Answer:",
            context, query
        );
        
        let response = self.client
            .post(&format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": "gpt-3.5-turbo",
                "messages": [
                    {
                        "role": "user",
                        "content": prompt
                    }
                ],
                "max_tokens": 1000,
                "temperature": 0.1
            }))
            .send()
            .await?;
        
        let response_json: serde_json::Value = response.json().await?;
        
        Ok(response_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("Sorry, I couldn't generate an answer.")
            .to_string())
    }
}