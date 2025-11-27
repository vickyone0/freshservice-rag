use crate::models::{ApiEndpoint, ApiParameter, ScrapedDocumentation};
use anyhow::Result;
use scraper::{Html, Selector};
use std::collections::HashMap;

pub struct FreshserviceScraper {
    base_url: String,
    client: reqwest::Client,
}

impl FreshserviceScraper {
    pub fn new() -> Self {
        Self {
            base_url: "https://api.freshservice.com".to_string(),
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }

    pub async fn scrape_ticket_attributes(&self) -> Result<ScrapedDocumentation> {
        let url = "https://api.freshservice.com/v2/#ticket";
        println!("Scraping from: {}", url);
        
        let response = self.client.get(url).send().await?;
        let html_content = response.text().await?;
        
        println!("Successfully fetched HTML content ({} bytes)", html_content.len());
        
        let document = Html::parse_document(&html_content);
        
        // Extract endpoints from the documentation
        let endpoints = self.extract_endpoints(&document).await?;
        
        println!("Found {} endpoints", endpoints.len());
        
        Ok(ScrapedDocumentation {
            base_url: self.base_url.clone(),
            endpoints,
            scraped_at: chrono::Utc::now(),
        })
    }
    
    async fn extract_endpoints(&self, document: &Html) -> Result<Vec<ApiEndpoint>> {
        let mut endpoints = Vec::new();
        
        // Try different selectors based on common documentation structures
        let possible_selectors = [
            ".endpoint",
            ".operation",
            ".api-endpoint",
            "[data-version='v2']",
            "h3", // Often endpoints are in headings
            "h4",
        ];
        
        for selector_str in &possible_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                let elements: Vec<_> = document.select(&selector).collect();
                if !elements.is_empty() {
                    println!("Found {} elements with selector '{}'", elements.len(), selector_str);
                    
                    for element in elements {
                        if let Some(endpoint) = self.parse_endpoint_element(element).await? {
                            endpoints.push(endpoint);
                        }
                    }
                    
                    // If we found endpoints with this selector, break
                    if !endpoints.is_empty() {
                        break;
                    }
                }
            }
        }
        
        // Fallback: If structure is different or scraping fails, use manual parsing
        if endpoints.is_empty() {
            println!("No endpoints found with standard selectors, using fallback data");
            endpoints = self.fallback_endpoint_extraction().await?;
        } else {
            println!("Successfully scraped {} endpoints from website", endpoints.len());
        }
        
        Ok(endpoints)
    }
    
    async fn parse_endpoint_element(&self, element: scraper::ElementRef<'_>) -> Result<Option<ApiEndpoint>> {
        let text = element.text().collect::<String>();
        let html = element.html();
        
        println!("Parsing element with text: {}", text.trim());
        println!("HTML: {}", html);
        
        // Look for common patterns in API documentation
        if text.contains("POST") || text.contains("GET") || text.contains("PUT") || text.contains("DELETE") {
            // This might be an endpoint element
            if let Some(endpoint) = self.parse_method_element(element).await? {
                return Ok(Some(endpoint));
            }
        }
        
        // Try to find endpoint information in nearby elements
        if let Some(parent) = element.parent() {
            let parent_html = parent.value().as_element().map(|e| e.name());
            println!("Parent element: {:?}", parent_html);
        }
        
        Ok(None)
    }
    
    async fn parse_method_element(&self, element: scraper::ElementRef<'_>) -> Result<Option<ApiEndpoint>> {
        let text = element.text().collect::<String>();
        
        // Extract HTTP method
        let method = if text.contains("POST") {
            "POST"
        } else if text.contains("GET") {
            "GET" 
        } else if text.contains("PUT") {
            "PUT"
        } else if text.contains("DELETE") {
            "DELETE"
        } else {
            return Ok(None);
        };
        
        // Extract path (this is simplified - you'll need to adjust based on actual HTML structure)
        let path = self.extract_path_from_text(&text).await;
        
        // Create a basic endpoint
        let endpoint = ApiEndpoint {
            name: format!("{} {}", method, path),
            description: "Endpoint extracted from documentation".to_string(),
            method: method.to_string(),
            path: path.clone(),
            parameters: vec![],
            curl_example: None,
        };
        
        println!("Parsed endpoint: {} {}", method, path);
        
        Ok(Some(endpoint))
    }
    
    async fn extract_path_from_text(&self, text: &str) -> String {
        // Look for URL patterns in the text
        let patterns = [
            "/api/v2/tickets",
            "/api/v2/tickets/",
            "/tickets",
            "api/v2",
        ];
        
        for pattern in &patterns {
            if let Some(start) = text.find(pattern) {
                // Extract the path starting from the pattern
                let remaining = &text[start..];
                // Take until whitespace or end
                if let Some(end) = remaining.find(|c: char| c.is_whitespace() || c == '<' || c == '>') {
                    return remaining[..end].to_string();
                } else {
                    return remaining.to_string();
                }
            }
        }
        
        // Fallback path
        "/api/v2/unknown".to_string()
    }
    
    // Debug method to see the actual HTML structure
    pub async fn debug_html_structure(&self) -> Result<()> {
        let url = "https://api.freshservice.com/v2/#ticket";
        let response = self.client.get(url).send().await?;
        let html_content = response.text().await?;
        let document = Html::parse_document(&html_content);
        
        println!("=== HTML STRUCTURE DEBUG ===");
        
        // Print all headings
        let heading_selectors = ["h1", "h2", "h3", "h4", "h5", "h6"];
        for selector_str in &heading_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                let headings: Vec<_> = document.select(&selector).collect();
                if !headings.is_empty() {
                    println!("\n{} {} headings found:", headings.len(), selector_str);
                    for heading in headings.iter().take(5) { // Limit to first 5
                        let text = heading.text().collect::<String>();
                        println!("  - {}", text.trim());
                    }
                }
            }
        }
        
        // Print elements with common API documentation classes
        let api_classes = ["endpoint", "operation", "method", "path", "api"];
        for class in &api_classes {
            let selector_str = &format!(".{}", class);
            if let Ok(selector) = Selector::parse(selector_str) {
                let elements: Vec<_> = document.select(&selector).collect();
                if !elements.is_empty() {
                    println!("\nElements with class '{}':", class);
                    for element in elements.iter().take(3) {
                        let text = element.text().collect::<String>();
                        println!("  - {}", text.trim());
                    }
                }
            }
        }
        
        // Look for specific text patterns
        let patterns = ["POST", "GET", "PUT", "DELETE", "/api/v2"];
        for pattern in &patterns {
            // This is a simplified search - in practice you'd want more sophisticated parsing
            if html_content.contains(pattern) {
                println!("\nFound pattern '{}' in HTML", pattern);
            }
        }
        
        Ok(())
    }
    
    async fn fallback_endpoint_extraction(&self) -> Result<Vec<ApiEndpoint>> {
        // ... keep your existing fallback implementation ...
        let mut endpoints = Vec::new();
        
        endpoints.push(ApiEndpoint {
            name: "Create Ticket".to_string(),
            description: "Create a new ticket in Freshservice".to_string(),
            method: "POST".to_string(),
            path: "/api/v2/tickets".to_string(),
            parameters: vec![
                ApiParameter {
                    name: "subject".to_string(),
                    param_type: "string".to_string(),
                    description: "Subject of the ticket".to_string(),
                    required: true,
                    default: None,
                },
                ApiParameter {
                    name: "description".to_string(),
                    param_type: "string".to_string(),
                    description: "HTML content of the ticket".to_string(),
                    required: true,
                    default: None,
                },
                ApiParameter {
                    name: "email".to_string(),
                    param_type: "string".to_string(),
                    description: "Email address of the requester".to_string(),
                    required: true,
                    default: None,
                },
            ],
            curl_example: Some(r#"curl -v -u yourapikey:X -H "Content-Type: application/json" -d '{"subject":"Ticket Title","description":"<h2>Ticket content</h2>","email":"user@example.com"}' -X POST "https://domain.freshservice.com/api/v2/tickets""#.to_string()),
        });

        // ... include all your other fallback endpoints ...
        endpoints.push(ApiEndpoint {
            name: "Get Ticket".to_string(),
            description: "Retrieve a specific ticket by ID".to_string(),
            method: "GET".to_string(),
            path: "/api/v2/tickets/{id}".to_string(),
            parameters: vec![
                ApiParameter {
                    name: "id".to_string(),
                    param_type: "integer".to_string(),
                    description: "Ticket ID".to_string(),
                    required: true,
                    default: None,
                },
            ],
            curl_example: Some(r#"curl -v -u yourapikey:X -X GET "https://domain.freshservice.com/api/v2/tickets/1""#.to_string()),
        });

        endpoints.push(ApiEndpoint {
            name: "List Tickets".to_string(),
            description: "Get a list of all tickets with optional filtering".to_string(),
            method: "GET".to_string(),
            path: "/api/v2/tickets".to_string(),
            parameters: vec![
                ApiParameter {
                    name: "page".to_string(),
                    param_type: "integer".to_string(),
                    description: "Page number for pagination".to_string(),
                    required: false,
                    default: Some("1".to_string()),
                },
                ApiParameter {
                    name: "per_page".to_string(),
                    param_type: "integer".to_string(),
                    description: "Number of records per page".to_string(),
                    required: false,
                    default: Some("30".to_string()),
                },
            ],
            curl_example: Some(r#"curl -v -u yourapikey:X -X GET "https://domain.freshservice.com/api/v2/tickets?page=1&per_page=30""#.to_string()),
        });

        Ok(endpoints)
    }
}