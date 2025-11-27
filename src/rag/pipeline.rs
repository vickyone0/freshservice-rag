use crate::models::{ApiEndpoint, ScrapedDocumentation};


#[derive(Clone)]
pub struct RagPipeline {
    documentation: ScrapedDocumentation,
    
}

impl RagPipeline {
    pub fn new(documentation: ScrapedDocumentation) -> Self {
        Self {
            documentation,
            
        }
    }
    
    pub fn find_relevant_endpoints(&self, query: &str) -> Vec<(&ApiEndpoint, f32)> {
        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();
        
        for endpoint in &self.documentation.endpoints {
            let score = self.calculate_relevance_score(endpoint, &query_lower);
            
            if score > 0.1 {
                matches.push((endpoint, score));
            }
        }
        
        // Sort by relevance score (descending)
        matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        matches
    }
    
    fn calculate_relevance_score(&self, endpoint: &ApiEndpoint, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let mut score:f32 = 0.0;

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

        // Normalize score to be between 0 and 1
        score.min(5.0) / 5.0
    }
    
    pub fn format_context(&self, matches: Vec<(&ApiEndpoint, f32)>) -> (String, f32) {
        let mut context = String::new();
        let mut overall_confidence: f32 = 0.0;
        
        for (endpoint, score) in matches {
            context.push_str(&format!("[Relevance: {:.2}] ", score));
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
            
            // Update overall confidence (weighted average)
            overall_confidence = overall_confidence.max(score);
        }
        
        (context, overall_confidence)
    }
    
    // New method to calculate confidence based on query quality and context match
    pub fn calculate_confidence(&self, query: &str, context: &str, matches: &[(&ApiEndpoint, f32)]) -> f32 {
        if matches.is_empty() {
            return 0.1; // Very low confidence when no matches found
        }
        
        let max_match_score = matches.iter().map(|(_, score)| score).fold(0.0f32, |a: f32, &b| a.max(b));
        
        // Factor in query quality
        let query_quality = self.assess_query_quality(query);
        
        // Factor in context richness
        let context_richness = self.assess_context_richness(context);
        
        // Combine factors
        let confidence = (max_match_score * 0.6) + (query_quality * 0.2) + (context_richness * 0.2);
        
        // Ensure confidence is between 0.1 and 1.0
        confidence.max(0.1).min(1.0)
    }
    
    fn assess_query_quality(&self, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        
        // Check if query contains API-related terms
        let api_terms = ["api", "endpoint", "method", "curl", "request", "ticket", "create", "get", "list", "update", "delete"];
        let mut term_count = 0;
        
        for term in &api_terms {
            if query_lower.contains(term) {
                term_count += 1;
            }
        }
        
        // Check query length and specificity
        let word_count = query_lower.split_whitespace().count();
        
        let specificity_score = if word_count >= 4 { 0.8 } else if word_count >= 2 { 0.5 } else { 0.2 };
        let term_score = (term_count as f32 / api_terms.len() as f32).min(1.0);
        
        (specificity_score * 0.6 + term_score * 0.4).min(1.0)
    }
    
    fn assess_context_richness(&self, context: &str) -> f32 {
        if context.is_empty() {
            return 0.0;
        }
        
        let lines: Vec<&str> = context.lines().collect();
        let non_empty_lines = lines.iter().filter(|line| !line.trim().is_empty()).count();
        
        // Check for presence of key sections
        let has_parameters = context.contains("Parameters:");
        let has_curl_example = context.contains("cURL Example:");
        let has_multiple_endpoints = context.matches("Endpoint:").count() > 1;
        
        let mut richness: f32 = 0.0;
        
        // Base score from line count
        if non_empty_lines >= 10 {
            richness += 0.4;
        } else if non_empty_lines >= 5 {
            richness += 0.2;
        } else {
            richness += 0.1;
        }
        
        // Bonus for having parameters
        if has_parameters {
            richness += 0.3;
        }
        
        // Bonus for having curl examples
        if has_curl_example {
            richness += 0.2;
        }
        
        // Bonus for multiple endpoints
        if has_multiple_endpoints {
            richness += 0.1;
        }
        
        richness.min(1.0)
    }
    
    pub fn get_documentation(&self) -> &ScrapedDocumentation {
        &self.documentation
    }
}