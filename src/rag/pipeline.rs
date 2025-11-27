use crate::models::{ApiEndpoint, ScrapedDocumentation};
use std::collections::HashMap;

#[derive(Clone)]
pub struct RagPipeline {
    pub documentation: ScrapedDocumentation,
    embeddings: HashMap<String, Vec<f32>>,
}

impl RagPipeline {
    pub fn new(documentation: ScrapedDocumentation) -> Self {
        Self {
            documentation,
            embeddings: HashMap::new(),
        }
    }
    
    pub fn find_relevant_endpoints(&self, query: &str) -> Vec<&ApiEndpoint> {
        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();
        
        for endpoint in &self.documentation.endpoints {
            let score = self.calculate_relevance_score(endpoint, &query_lower);
            
            if score > 0.1 {
                matches.push(endpoint);
            }
        }
        
        // Sort by relevance
        matches.sort_by(|a, b| {
            let score_a = self.calculate_relevance_score(a, &query_lower);
            let score_b = self.calculate_relevance_score(b, &query_lower);
            score_b.partial_cmp(&score_a).unwrap()
        });
        
        matches
    }
    
    fn calculate_relevance_score(&self, endpoint: &ApiEndpoint, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let mut score = 0.0;

        // Check name (highest weight)
        let name_lower = endpoint.name.to_lowercase();
        if name_lower.contains(&query_lower) {
            score += 2.0;
        }
        for word in &query_words {
            if name_lower.contains(word) {
                score += 0.5;
            }
        }

        // Check description
        let desc_lower = endpoint.description.to_lowercase();
        if desc_lower.contains(&query_lower) {
            score += 1.0;
        }
        for word in &query_words {
            if desc_lower.contains(word) {
                score += 0.3;
            }
        }

        // Check path/URL
        let path_lower = endpoint.path.to_lowercase();
        if path_lower.contains(&query_lower) {
            score += 0.8;
        }

        // Check parameters
        for param in &endpoint.parameters {
            let param_name_lower = param.name.to_lowercase();
            if param_name_lower.contains(&query_lower) {
                score += 0.4;
            }
            if param.description.to_lowercase().contains(&query_lower) {
                score += 0.2;
            }
        }

        // Check for specific keywords
        if query_lower.contains("api") || query_lower.contains("freshservice") {
            score += 0.5;
        }
        if query_lower.contains("curl") && endpoint.curl_example.is_some() {
            score += 1.0;
        }
        if query_lower.contains("create") && name_lower.contains("create") {
            score += 0.8;
        }
        if query_lower.contains("get") && name_lower.contains("get") {
            score += 0.6;
        }
        if query_lower.contains("list") && name_lower.contains("list") {
            score += 0.6;
        }
        if query_lower.contains("update") && name_lower.contains("update") {
            score += 0.6;
        }
        if query_lower.contains("delete") && name_lower.contains("delete") {
            score += 0.6;
        }

        score
    }
    
    pub fn format_context(&self, endpoints: Vec<&ApiEndpoint>) -> String {
        let mut context = String::new();
        
        for endpoint in endpoints {
            context.push_str(&format!("Endpoint: {} ({})\n", endpoint.name, endpoint.method));
            context.push_str(&format!("Description: {}\n", endpoint.description));
            context.push_str(&format!("Path: {}\n", endpoint.path));
            
            if !endpoint.parameters.is_empty() {
                context.push_str("Parameters:\n");
                for param in &endpoint.parameters {
                    context.push_str(&format!("  - {} ({})", param.name, param.param_type));
                    if param.required {
                        context.push_str(" [Required]");
                    }
                    context.push_str(&format!(": {}\n", param.description));
                }
            }
            
            if let Some(curl) = &endpoint.curl_example {
                context.push_str(&format!("cURL Example:\n{}\n", curl));
            }
            
            context.push_str("\n---\n\n");
        }
        
        context
    }
}