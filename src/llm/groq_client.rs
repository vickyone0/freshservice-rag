use anyhow::Result;
use serde_json::json;

pub struct GroqClient {
    api_key: String,
    client: reqwest::Client,
}

impl GroqClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
    
    pub async fn generate_answer(&self, query: &str, context: &str) -> Result<String> {
        let prompt = format!(
            "You are a helpful assistant for Freshservice API documentation. \
            Use the following context to answer the user's question. \
            If the context doesn't contain the answer, say so.\n\n\
            CONTEXT:\n{}\n\n\
            QUESTION: {}\n\n\
            Please provide a clear, helpful answer based on the context above:",
            context, query
        );
        
        let response = self.client
            .post("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": "llama-3.3-70b-versatile",
                "messages": [
                    {
                        "role": "system",
                        "content": "You are an expert on Freshservice API documentation. Provide accurate, helpful answers based on the given context."
                    },
                    {
                        "role": "user",
                        "content": prompt
                    }
                ],
                "temperature": 0.1,
                "max_tokens": 1024,
                "top_p": 0.9,
                "stream": false
            }))
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Groq API error: {}", error_text));
        }
        
        let response_json: serde_json::Value = response.json().await?;
        
        let answer = response_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("Sorry, I couldn't generate an answer.")
            .trim()
            .to_string();
        
        if answer.is_empty() {
            return Ok("Sorry, I couldn't generate an answer.".to_string());
        }
        
        Ok(answer)
    }
}