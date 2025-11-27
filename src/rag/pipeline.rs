use crate::models::{ApiEndpoint, ScrapedDocumentation};

#[derive(Clone)]
pub struct RagPipeline {
    documentation: ScrapedDocumentation,
}

impl RagPipeline {
    pub fn new(documentation: ScrapedDocumentation) -> Self {
        Self { documentation }
    }
    
    pub fn find_relevant_endpoints(&self, query: &str) -> Vec<(&ApiEndpoint, f32)> {
        let query_lower = query.to_lowercase();
        
        let mut matches: Vec<_> = self.documentation.endpoints
            .iter()
            .filter_map(|endpoint| {
                let score = self.calculate_relevance_score(endpoint, &query_lower);
                if score > 0.1 {
                    Some((endpoint, score))
                } else {
                    None
                }
            })
            .collect();
        
        // Sort by relevance score (descending)
        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        matches
    }
    
    fn calculate_relevance_score(&self, endpoint: &ApiEndpoint, query_lower: &str) -> f32 {
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let mut score = 0.0f32;

        // Check name (highest weight: 2.5 max)
        let name_lower = endpoint.name.to_lowercase();
        if name_lower.contains(query_lower) {
            score += 2.0;
        }
        score += query_words.iter()
            .filter(|word| name_lower.contains(*word))
            .count() as f32 * 0.5;

        // Check description (1.3 max)
        let desc_lower = endpoint.description.to_lowercase();
        if desc_lower.contains(query_lower) {
            score += 1.0;
        }
        score += query_words.iter()
            .filter(|word| desc_lower.contains(*word))
            .count() as f32 * 0.3;

        // Check path (0.8 max)
        if endpoint.path.to_lowercase().contains(query_lower) {
            score += 0.8;
        }

        // Check HTTP method match (0.8 max)
        let method_lower = endpoint.method.to_lowercase();
        if query_words.iter().any(|word| {
            method_lower == *word || 
            (method_lower == "get" && *word == "list") ||
            (method_lower == "get" && *word == "view") ||
            (method_lower == "get" && *word == "fetch")
        }) {
            score += 0.8;
        }

        // Check parameters (0.6 max)
        for param in &endpoint.parameters {
            if param.name.to_lowercase().contains(query_lower) {
                score += 0.4;
                break;
            }
            if param.description.to_lowercase().contains(query_lower) {
                score += 0.2;
                break;
            }
        }

        // Check for curl example (1.0 if query mentions curl)
        if query_lower.contains("curl") && endpoint.curl_example.is_some() {
            score += 1.0;
        }

        // Normalize score to 0-1 range (max theoretical: ~7)
        (score / 7.0).min(1.0)
    }
    
    pub fn format_context(&self, matches: &[(&ApiEndpoint, f32)]) -> (String, f32) {
        if matches.is_empty() {
            return (String::from("No relevant endpoints found."), 0.0);
        }

        let mut context = String::with_capacity(matches.len() * 200);
        let max_score = matches.first().map(|(_, s)| *s).unwrap_or(0.0);
        
        for (endpoint, score) in matches.iter().take(5) {  // Limit to top 5
            context.push_str(&format!(
                "[Relevance: {:.2}] {} ({})\n\
                 Description: {}\n\
                 Path: {}\n",
                score, endpoint.name, endpoint.method,
                endpoint.description, endpoint.path
            ));
            
            if !endpoint.parameters.is_empty() {
                context.push_str("Parameters:\n");
                for param in &endpoint.parameters {
                    context.push_str(&format!(
                        "  - {} ({}){}: {}\n",
                        param.name, param.param_type,
                        if param.required { " [Required]" } else { "" },
                        param.description
                    ));
                }
            }
            
            if let Some(curl) = &endpoint.curl_example {
                context.push_str(&format!("cURL Example:\n{}\n", curl));
            }
            
            context.push_str("\n---\n\n");
        }
        
        (context, max_score)
    }
    
    pub fn calculate_confidence(&self, query: &str, matches: &[(&ApiEndpoint, f32)]) -> f32 {
        if matches.is_empty() {
            return 0.1;
        }
        
        let max_score = matches.first().map(|(_, s)| *s).unwrap_or(0.0);
        let query_quality = self.assess_query_quality(query);
        
        // Weight: 70% match score, 30% query quality
        let confidence = (max_score * 0.7) + (query_quality * 0.3);
        
        confidence.clamp(0.1, 1.0)
    }
    
    fn assess_query_quality(&self, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        let words: Vec<&str> = query_lower.split_whitespace().collect();
        
        if words.is_empty() {
            return 0.1;
        }
        
        // Check for API-related terms
        let api_terms = [
            "api", "endpoint", "method", "curl", "request", "response",
            "ticket", "create", "get", "list", "update", "delete", "view",
            "post", "put", "patch", "fetch", "retrieve"
        ];
        
        let term_matches = api_terms.iter()
            .filter(|term| query_lower.contains(*term))
            .count();
        
        let term_score = (term_matches as f32 / 3.0).min(1.0);  // Cap at 3 terms
        
        // Length/specificity score
        let length_score = match words.len() {
            0 => 0.1,
            1 => 0.3,
            2..=3 => 0.6,
            _ => 0.9,
        };
        
        // Combine: 60% length, 40% terms
        (length_score * 0.6 + term_score * 0.4).min(1.0)
    }
    
    pub fn get_documentation(&self) -> &ScrapedDocumentation {
        &self.documentation
    }
    
    // Helper method to get top N endpoints
    pub fn get_top_matches(&self, query: &str, limit: usize) -> Vec<(&ApiEndpoint, f32)> {
        let mut matches = self.find_relevant_endpoints(query);
        matches.truncate(limit);
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ApiParameter};
    
    #[test]
    fn test_calculate_relevance_score() {
        let pipeline = create_test_pipeline();
        let endpoint = &pipeline.documentation.endpoints[0];
        
        // Should match "create ticket"
        let score = pipeline.calculate_relevance_score(endpoint, "create ticket");
        assert!(score > 0.5);
        
        // Should not match unrelated query
        let score = pipeline.calculate_relevance_score(endpoint, "delete user");
        assert!(score < 0.3);
    }
    
    #[test]
    fn test_find_relevant_endpoints() {
        let pipeline = create_test_pipeline();
        let matches = pipeline.find_relevant_endpoints("create ticket");
        
        assert!(!matches.is_empty());
        assert!(matches[0].1 > 0.0);
    }
    
    fn create_test_pipeline() -> RagPipeline {
        let endpoints = vec![
            ApiEndpoint {
                name: "Create Ticket".to_string(),
                description: "Create a new ticket".to_string(),
                method: "POST".to_string(),
                path: "/api/v2/tickets".to_string(),
                parameters: vec![
                    ApiParameter {
                        name: "subject".to_string(),
                        param_type: "string".to_string(),
                        description: "Ticket subject".to_string(),
                        required: true,
                        default: None,
                    }
                ],
                curl_example: Some("curl -X POST ...".to_string()),
            }
        ];
        
        RagPipeline::new(ScrapedDocumentation {
            base_url: "https://api.freshservice.com".to_string(),
            endpoints,
            scraped_at: chrono::Utc::now(),
        })
    }
}