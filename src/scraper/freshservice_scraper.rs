use crate::models::{ApiEndpoint, ApiParameter, ScrapedDocumentation};
use anyhow::{Result, Context};
use scraper::{Html, Selector, ElementRef};
use regex::Regex;

// Helper function to extract default values from description text
fn extract_default_value(description: &str) -> String {
    let re = Regex::new(r"[Dd]efault[:\s]+([^\s,\.]+)").unwrap();
    if let Some(cap) = re.captures(description) {
        cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default()
    } else {
        String::new()
    }
}

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
        
        let response = self.client.get(url).send().await
            .context("Failed to fetch documentation page")?;
        let html_content = response.text().await?;
        
        println!("Successfully fetched HTML content ({} bytes)", html_content.len());
        println!("════════════════════════════════════════════════════════════════");
        
        let document = Html::parse_document(&html_content);
        
        // Extract endpoints from the documentation
        let endpoints = self.extract_endpoints(&document).await?;
        
        println!("════════════════════════════════════════════════════════════════");
        println!("✓ Total endpoints extracted: {}", endpoints.len());
        
        // Print summary
        if !endpoints.is_empty() {
            println!("\nEndpoints Summary:");
            for (i, endpoint) in endpoints.iter().enumerate() {
                println!("  {}. {} {} - {}", 
                    i + 1, 
                    endpoint.method, 
                    endpoint.path,
                    endpoint.description.chars().take(50).collect::<String>()
                );
            }
        }
        
        Ok(ScrapedDocumentation {
            base_url: self.base_url.clone(),
            endpoints,
            scraped_at: chrono::Utc::now(),
        })
    }
    
    async fn extract_endpoints(&self, document: &Html) -> Result<Vec<ApiEndpoint>> {
        let mut endpoints = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        
        println!("Strategy 1: Extracting from ticket-specific divs...");
        
        // Dynamically find all divs with IDs containing 'ticket'
        if let Ok(selector) = Selector::parse("div[id*='ticket']") {
            let ticket_divs: Vec<_> = document.select(&selector).collect();
            println!("Found {} divs with 'ticket' in their ID", ticket_divs.len());
            
            for div in ticket_divs {
                if let Some(id) = div.value().id() {
                    // Skip the main container divs
                    if id == "tickets" || id == "tickets-panel" || id == "ticket_attributes" {
                        continue;
                    }
                    
                    println!("  Processing: {}", id);
                    if let Some(endpoint) = self.parse_endpoint_section(div).await {
                        let key = format!("{} {}", endpoint.method, endpoint.path);
                        if !seen_paths.contains(&key) {
                            seen_paths.insert(key);
                            endpoints.push(endpoint);
                        }
                    }
                }
            }
        }
        
        println!("\nStrategy 2: Extracting from ticket section code blocks...");
        
        // Find the main tickets section and extract all code blocks from it
        if let Ok(tickets_selector) = Selector::parse("div#tickets") {
            if let Some(tickets_section) = document.select(&tickets_selector).next() {
                let code_endpoints = self.extract_code_blocks_from_section(tickets_section).await?;
                println!("Found {} endpoints from ticket code blocks", code_endpoints.len());
                
                for endpoint in code_endpoints {
                    let key = format!("{} {}", endpoint.method, endpoint.path);
                    if !seen_paths.contains(&key) {
                        seen_paths.insert(key);
                        endpoints.push(endpoint);
                    }
                }
            }
        }
        
        // Fallback to predefined data if scraping fails
        if endpoints.is_empty() {
            println!("\n⚠ No endpoints found, using fallback data");
            endpoints = self.fallback_endpoint_extraction().await?;
        } else {
            println!("\n✓ Successfully scraped {} unique ticket endpoints", endpoints.len());
        }
        
        Ok(endpoints)
    }
    
    async fn extract_code_blocks_from_section(&self, section: ElementRef<'_>) -> Result<Vec<ApiEndpoint>> {
        let mut endpoints = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        
        // Select all code blocks within this section
        if let Ok(selector) = Selector::parse("pre, .highlight") {
            for code_elem in section.select(&selector) {
                let code_text = code_elem.text().collect::<String>();
                
                // Only process curl commands that contain /tickets
                if !code_text.contains("curl") || !code_text.contains("/tickets") {
                    continue;
                }
                
                // Extract method
                let method = if code_text.contains("-X POST") {
                    "POST"
                } else if code_text.contains("-X PUT") {
                    "PUT"
                } else if code_text.contains("-X DELETE") {
                    "DELETE"
                } else if code_text.contains("-X PATCH") {
                    "PATCH"
                } else {
                    "GET"
                };
                
                // Extract path
                let path = self.extract_path_from_text(&code_text);
                
                // Skip if not a ticket endpoint or already seen
                if !path.contains("/tickets") {
                    continue;
                }
                
                let key = format!("{} {}", method, path);
                if seen_paths.contains(&key) {
                    continue;
                }
                seen_paths.insert(key.clone());
                
                // Find description
                let description = self.find_description_for_code_block(code_elem)
                    .unwrap_or_else(|| self.infer_description_from_path(&path, &method));
                
                println!("    ✓ {} {}", method, path);
                
                endpoints.push(ApiEndpoint {
                    name: key,
                    description,
                    method: method.to_string(),
                    path: path.clone(),
                    parameters: vec![],
                    curl_example: Some(code_text.trim().to_string()),
                });
            }
        }
        
        Ok(endpoints)
    }
    
    fn infer_description_from_path(&self, path: &str, method: &str) -> String {
        // Infer description from path patterns
        match (method, path) {
            ("POST", p) if p.ends_with("/tickets") => "Create a Ticket".to_string(),
            ("GET", p) if p.contains("/tickets/{id}") || p.matches('/').count() == 4 => "View a Ticket".to_string(),
            ("GET", p) if p.ends_with("/tickets") => "List All Tickets".to_string(),
            ("PUT", p) if p.contains("/tickets/{id}") || p.matches('/').count() == 4 => "Update a Ticket".to_string(),
            ("DELETE", p) if p.contains("/tickets/{id}") || p.matches('/').count() == 4 => "Delete a Ticket".to_string(),
            ("PUT", p) if p.contains("/restore") => "Restore a Ticket".to_string(),
            ("POST", p) if p.contains("/notes") => "Create Ticket Note".to_string(),
            ("GET", p) if p.contains("/notes") && p.matches('/').count() > 4 => "View a Ticket Note".to_string(),
            ("GET", p) if p.contains("/notes") => "List Ticket Notes".to_string(),
            ("PUT", p) if p.contains("/notes") => "Update Ticket Note".to_string(),
            ("DELETE", p) if p.contains("/notes") => "Delete Ticket Note".to_string(),
            ("POST", p) if p.contains("/tasks") => "Create Ticket Task".to_string(),
            ("GET", p) if p.contains("/tasks") => "View Ticket Tasks".to_string(),
            ("PUT", p) if p.contains("/tasks") => "Update Ticket Task".to_string(),
            ("DELETE", p) if p.contains("/tasks") => "Delete Ticket Task".to_string(),
            ("POST", p) if p.contains("/time_entries") => "Create Time Entry".to_string(),
            ("GET", p) if p.contains("/time_entries") => "View Time Entries".to_string(),
            ("PUT", p) if p.contains("/time_entries") => "Update Time Entry".to_string(),
            ("DELETE", p) if p.contains("/time_entries") => "Delete Time Entry".to_string(),
            _ => format!("{} Ticket Operation", method),
        }
    }
    
    async fn extract_from_code_blocks(&self, document: &Html) -> Result<Vec<ApiEndpoint>> {
        // This function is no longer used - replaced by extract_code_blocks_from_section
        Ok(vec![])
    }
    
    fn find_description_for_code_block(&self, code_elem: ElementRef<'_>) -> Option<String> {
        // Try to find the parent div with an ID (like "create_ticket")
        let mut current = code_elem;
        
        // Walk up the tree looking for a div with an ID
        for _ in 0..5 {
            if let Some(parent) = current.parent() {
                if let Some(parent_elem) = ElementRef::wrap(parent) {
                    // Check if this is a div with an ID
                    if let Some(id) = parent_elem.value().id() {
                        if !id.is_empty() {
                            // Convert ID to readable name
                            let name = id.replace('_', " ")
                                .split_whitespace()
                                .map(|word| {
                                    let mut chars = word.chars();
                                    match chars.next() {
                                        None => String::new(),
                                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            return Some(name);
                        }
                    }
                    
                    // Also look for h2 or h3 in this parent
                    if let Ok(h2_selector) = Selector::parse("h2") {
                        if let Some(h2) = parent_elem.select(&h2_selector).next() {
                            let text = h2.text().collect::<String>().trim().to_string();
                            if !text.is_empty() && text.len() < 100 {
                                return Some(text);
                            }
                        }
                    }
                    
                    current = parent_elem;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        
        None
    }
    
    async fn extract_by_http_methods(&self, document: &Html) -> Result<Vec<ApiEndpoint>> {
        let mut endpoints = Vec::new();
        
        // Look for elements containing HTTP method indicators
        let method_selectors = vec![
            ".method", ".http-method", "span.verb", 
            "[class*='method']", "[class*='verb']"
        ];
        
        for selector_str in &method_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                for method_elem in document.select(&selector) {
                    let method_text = method_elem.text().collect::<String>().trim().to_uppercase();
                    
                    if ["GET", "POST", "PUT", "DELETE", "PATCH"].contains(&method_text.as_str()) {
                        // Found an HTTP method, now look for related path and details
                        if let Some(endpoint) = self.extract_endpoint_from_method(method_elem, &method_text).await {
                            endpoints.push(endpoint);
                        }
                    }
                }
                
                if !endpoints.is_empty() {
                    break;
                }
            }
        }
        
        Ok(endpoints)
    }
    
    async fn extract_endpoint_from_method(&self, method_elem: ElementRef<'_>, method: &str) -> Option<ApiEndpoint> {
        // Navigate through parent/sibling elements to find path and description
        let mut path = String::new();
        let description;
        
        // Check siblings for path information
        for sibling in method_elem.parent()?.children() {
            if let Some(elem) = ElementRef::wrap(sibling) {
                let text = elem.text().collect::<String>();
                
                // Look for API paths
                if text.contains("/api/v2/") || text.contains("/tickets") {
                    path = self.extract_path_from_text(&text);
                    break;
                }
            }
        }
        
        // If no path found in siblings, check parent container
        if path.is_empty() {
            if let Some(parent) = method_elem.parent() {
                if let Some(parent_elem) = ElementRef::wrap(parent) {
                    let parent_text = parent_elem.text().collect::<String>();
                    path = self.extract_path_from_text(&parent_text);
                }
            }
        }
        
        if !path.is_empty() {
            // Try to extract description
            description = self.extract_description_near_element(method_elem)
                .unwrap_or_else(|| format!("Endpoint: {} {}", method, path));
            
            let name = format!("{} {}", method, path);
            
            Some(ApiEndpoint {
                name: name.clone(),
                description,
                method: method.to_string(),
                path: path.clone(),
                parameters: vec![],
                curl_example: None,
            })
        } else {
            None
        }
    }
    
    fn extract_description_near_element(&self, element: ElementRef<'_>) -> Option<String> {
        // Look for description in nearby p tags or div.description
        let desc_selectors = ["p", ".description", ".endpoint-description"];
        
        for selector_str in &desc_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(parent) = element.parent() {
                    if let Some(parent_elem) = ElementRef::wrap(parent) {
                        if let Some(desc_elem) = parent_elem.select(&selector).next() {
                            let desc = desc_elem.text().collect::<String>().trim().to_string();
                            if !desc.is_empty() && desc.len() < 500 {
                                return Some(desc);
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    async fn parse_endpoint_section(&self, element: ElementRef<'_>) -> Option<ApiEndpoint> {
        // Extract the heading (h2) for the endpoint name/description
        let description = if let Ok(h2_selector) = Selector::parse("h2") {
            if let Some(h2) = element.select(&h2_selector).next() {
                h2.text().collect::<String>().trim().to_string()
            } else {
                "API endpoint".to_string()
            }
        } else {
            "API endpoint".to_string()
        };
        
        // Find the code block with curl command
        let curl_example = if let Ok(code_selector) = Selector::parse("pre, .highlight") {
            element.select(&code_selector)
                .next()
                .map(|code| code.text().collect::<String>().trim().to_string())
        } else {
            None
        };
        
        // Extract method and path from curl example
        if let Some(ref curl) = curl_example {
            let method = if curl.contains("-X POST") {
                "POST"
            } else if curl.contains("-X PUT") {
                "PUT"
            } else if curl.contains("-X DELETE") {
                "DELETE"
            } else if curl.contains("-X PATCH") {
                "PATCH"
            } else if curl.contains("-X GET") || curl.contains("curl") {
                "GET"
            } else {
                return None;
            };
            
            let path = self.extract_path_from_text(curl);
            
            if path == "/api/v2/unknown" {
                return None;
            }
            
            // Extract parameters from the section
            let parameters = self.extract_parameters_from_section(element);
            
            println!("  ✓ Extracted: {} {}", method, path);
            
            return Some(ApiEndpoint {
                name: description.clone(),
                description: description.clone(),
                method: method.to_string(),
                path,
                parameters,
                curl_example: Some(curl.clone()),
            });
        }
        
        None
    }
    
    fn extract_description_from_section(&self, element: ElementRef<'_>) -> Option<String> {
        // Try to find h2, h3, h4, or p tags
        let heading_selectors = ["h2", "h3", "h4", "p"];
        
        for selector_str in &heading_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(heading) = element.select(&selector).next() {
                    let desc = heading.text().collect::<String>().trim().to_string();
                    if !desc.is_empty() && !desc.contains("POST") && !desc.contains("GET") {
                        return Some(desc);
                    }
                }
            }
        }
        
        Some("API endpoint".to_string())
    }
    
    fn extract_parameters_from_section(&self, element: ElementRef<'_>) -> Vec<ApiParameter> {
        let mut parameters = Vec::new();
        
        // Look for tables that contain parameter information
        if let Ok(table_selector) = Selector::parse("table") {
            for table in element.select(&table_selector) {
                // Check if this is a parameters table by looking at headers
                let table_text = table.text().collect::<String>().to_lowercase();
                
                if table_text.contains("parameter") || 
                   table_text.contains("attribute") || 
                   table_text.contains("field") {
                    
                    if let Ok(row_selector) = Selector::parse("tr") {
                        let rows: Vec<_> = table.select(&row_selector).collect();
                        
                        // Skip the header row
                        for row in rows.iter().skip(1) {
                            if let Some(param) = self.parse_parameter_row(row) {
                                parameters.push(param);
                            }
                        }
                    }
                }
            }
        }
        
        parameters
    }
    
    fn parse_parameter_row(&self, row: &ElementRef<'_>) -> Option<ApiParameter> {
        if let Ok(cell_selector) = Selector::parse("td") {
            let cells: Vec<_> = row.select(&cell_selector).collect();
            
            if cells.len() >= 2 {
                let name = cells[0].text().collect::<String>().trim().to_string();
                let description = cells.get(1)
                    .map(|c| c.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                
                // Try to determine type from description or third column
                let param_type = cells.get(2)
                    .map(|c| c.text().collect::<String>().trim().to_lowercase())
                    .unwrap_or_else(|| {
                        // Guess type from description
                        let desc_lower = description.to_lowercase();
                        if desc_lower.contains("integer") || desc_lower.contains("number") {
                            "integer".to_string()
                        } else if desc_lower.contains("boolean") {
                            "boolean".to_string()
                        } else if desc_lower.contains("array") {
                            "array".to_string()
                        } else {
                            "string".to_string()
                        }
                    });
                
                // Determine if required
                let required = description.to_lowercase().contains("required") ||
                              description.to_lowercase().contains("mandatory");
                
                // Extract default value if mentioned
                let default = if description.to_lowercase().contains("default") {
                    // Try to extract default value (simplified)
                    Some(extract_default_value(&description))
                } else {
                    None
                };
                
                if !name.is_empty() {
                    return Some(ApiParameter {
                        name,
                        param_type,
                        description,
                        required,
                        default,
                    });
                }
            }
        }
        
        None
    }
    
    fn extract_curl_example(&self, element: ElementRef<'_>) -> Option<String> {
        // Look for code blocks or pre tags
        let code_selectors = ["pre", "code", ".code-block", ".example"];
        
        for selector_str in &code_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                for code_elem in element.select(&selector) {
                    let code = code_elem.text().collect::<String>();
                    if code.contains("curl") {
                        return Some(code.trim().to_string());
                    }
                }
            }
        }
        
        None
    }
    
    fn extract_path_from_text(&self, text: &str) -> String {
        // Use multiple regex patterns to find API paths
        let patterns = vec![
            r"https://[^/]+(/api/v2/[a-zA-Z0-9/_\-{}]+)",  // Full URL
            r"'(/api/v2/[a-zA-Z0-9/_\-{}]+)'",              // Single quoted path
            r#""(/api/v2/[a-zA-Z0-9/_\-{}]+)""#,            // Double quoted path
            r"(/api/v2/[a-zA-Z0-9/_\-{}]+)",                // Plain path
        ];
        
        for pattern in patterns {
            let re = Regex::new(pattern).unwrap();
            if let Some(captures) = re.captures(text) {
                if let Some(path_match) = captures.get(1) {
                    let path = path_match.as_str().to_string();
                    // Clean up any trailing characters
                    let cleaned = path
                        .trim_end_matches('\'')
                        .trim_end_matches('"')
                        .trim_end_matches('\\')
                        .to_string();
                    return cleaned;
                }
            }
        }
        
        // If no match found, return unknown
        "/api/v2/unknown".to_string()
    }
    
    pub async fn debug_html_structure(&self) -> Result<()> {
        let url = "https://api.freshservice.com/v2/#ticket";
        let response = self.client.get(url).send().await?;
        let html_content = response.text().await?;
        let document = Html::parse_document(&html_content);
        
        println!("\n=== COMPREHENSIVE HTML STRUCTURE DEBUG ===\n");
        println!("Total HTML size: {} bytes\n", html_content.len());
        
        // Print all headings with their IDs and classes
        let heading_selectors = ["h1", "h2", "h3", "h4"];
        for selector_str in &heading_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                let headings: Vec<_> = document.select(&selector).collect();
                if !headings.is_empty() {
                    println!("{} {} found:", headings.len(), selector_str);
                    for (i, heading) in headings.iter().enumerate().take(15) {
                        let text = heading.text().collect::<String>();
                        let id = heading.value().id().unwrap_or("");
                        let classes = heading.value().classes().collect::<Vec<_>>().join(" ");
                        println!("  {}. '{}' [id='{}', class='{}']", 
                            i + 1, text.trim(), id, classes);
                    }
                    println!();
                }
            }
        }
        
        // Check for common API doc structures with detailed output
        let api_selectors = [
            "article", ".endpoint", ".method", ".api-content", ".http-method",
            "[data-type]", "section", ".operation", "div[id]", "span.verb",
            ".verb", "[class*='method']", "[class*='endpoint']"
        ];
        
        println!("=== API Documentation Elements ===");
        for selector_str in &api_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                let elements: Vec<_> = document.select(&selector).collect();
                if !elements.is_empty() {
                    println!("\n'{}': {} elements found", selector_str, elements.len());
                    for (i, elem) in elements.iter().enumerate().take(5) {
                        let text = elem.text().collect::<String>();
                        let preview = text.chars().take(100).collect::<String>();
                        let id = elem.value().id().unwrap_or("");
                        let classes = elem.value().classes().collect::<Vec<_>>().join(" ");
                        println!("  [{}] id='{}' class='{}' text='{}'", 
                            i + 1, id, classes, preview.trim());
                    }
                }
            }
        }
        
        // Search for HTTP methods in the HTML with context
        let methods = ["GET", "POST", "PUT", "DELETE", "PATCH"];
        println!("\n=== HTTP Methods in Content ===");
        for method in &methods {
            let re = Regex::new(&format!(r".{{0,50}}{}(.{{0,100}})", method)).unwrap();
            let matches: Vec<_> = re.find_iter(&html_content).take(5).collect();
            if !matches.is_empty() {
                println!("\n'{}': {} occurrences (showing first 5 with context)", 
                    method, html_content.matches(method).count());
                for (i, m) in matches.iter().enumerate() {
                    let context = m.as_str().replace("\n", " ");
                    println!("  [{}] ...{}...", i + 1, context.trim());
                }
            }
        }
        
        // Look for paths with context
        println!("\n=== API Paths Found ===");
        let path_re = Regex::new(r"/api/v2/[a-zA-Z0-9/_\-{}]+").unwrap();
        let paths: std::collections::HashSet<_> = path_re.find_iter(&html_content)
            .map(|m| m.as_str())
            .collect();
        println!("Unique paths found: {}", paths.len());
        for (i, path) in paths.iter().enumerate().take(20) {
            println!("  {}. {}", i + 1, path);
        }
        
        // Look for specific ticket-related IDs
        println!("\n=== Elements with 'ticket' in ID ===");
        if let Ok(selector) = Selector::parse("[id*='ticket']") {
            for (i, elem) in document.select(&selector).enumerate().take(10) {
                let id = elem.value().id().unwrap_or("");
                let tag = elem.value().name();
                let text = elem.text().collect::<String>();
                let preview = text.chars().take(80).collect::<String>();
                println!("  [{}] <{}> id='{}' text='{}'", 
                    i + 1, tag, id, preview.trim());
            }
        }
        
        // Look for code blocks (might contain examples)
        println!("\n=== Code Blocks ===");
        let code_selectors = ["pre", "code", ".highlight", ".code-sample"];
        for selector_str in &code_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                let count = document.select(&selector).count();
                if count > 0 {
                    println!("'{}': {} blocks found", selector_str, count);
                    for (i, code) in document.select(&selector).enumerate().take(3) {
                        let text = code.text().collect::<String>();
                        let preview = text.chars().take(150).collect::<String>();
                        if preview.contains("curl") || preview.contains("/api") {
                            println!("  [{}] {}", i + 1, preview.trim());
                        }
                    }
                }
            }
        }
        
        // Check for JavaScript-rendered content indicators
        println!("\n=== JavaScript Framework Detection ===");
        if html_content.contains("react") || html_content.contains("React") {
            println!("✓ React framework detected");
        }
        if html_content.contains("vue") || html_content.contains("Vue") {
            println!("✓ Vue framework detected");
        }
        if html_content.contains("angular") || html_content.contains("Angular") {
            println!("✓ Angular framework detected");
        }
        if html_content.contains("data-react") || html_content.contains("data-reactroot") {
            println!("✓ React root detected - content may be dynamically rendered");
        }
        
        // Save a sample of the HTML for manual inspection
        println!("\n=== Saving HTML Sample ===");
        if let Some(ticket_section) = self.extract_html_section(&html_content, "ticket") {
            std::fs::write("debug_ticket_section.html", ticket_section)?;
            println!("✓ Saved relevant section to 'debug_ticket_section.html'");
        }
        
        Ok(())
    }
    
    fn extract_html_section(&self, html: &str, keyword: &str) -> Option<String> {
        // Find a section of HTML containing the keyword
        if let Some(pos) = html.to_lowercase().find(keyword) {
            let start = pos.saturating_sub(2000);
            let end = (pos + 10000).min(html.len());
            Some(html[start..end].to_string())
        } else {
            None
        }
    }
    
    async fn fallback_endpoint_extraction(&self) -> Result<Vec<ApiEndpoint>> {
        let mut endpoints = Vec::new();
        
        // Comprehensive fallback data for Freshservice Tickets API
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
                ApiParameter {
                    name: "priority".to_string(),
                    param_type: "integer".to_string(),
                    description: "Priority of the ticket (1-4)".to_string(),
                    required: false,
                    default: Some("1".to_string()),
                },
                ApiParameter {
                    name: "status".to_string(),
                    param_type: "integer".to_string(),
                    description: "Status of the ticket (2-5)".to_string(),
                    required: false,
                    default: Some("2".to_string()),
                },
            ],
            curl_example: Some(r#"curl -v -u yourapikey:X -H "Content-Type: application/json" -d '{"subject":"Ticket Title","description":"<h2>Ticket content</h2>","email":"user@example.com","priority":1,"status":2}' -X POST "https://domain.freshservice.com/api/v2/tickets""#.to_string()),
        });

        endpoints.push(ApiEndpoint {
            name: "Get Ticket".to_string(),
            description: "Retrieve a specific ticket by ID".to_string(),
            method: "GET".to_string(),
            path: "/api/v2/tickets/{id}".to_string(),
            parameters: vec![
                ApiParameter {
                    name: "id".to_string(),
                    param_type: "integer".to_string(),
                    description: "Unique identifier of the ticket".to_string(),
                    required: true,
                    default: None,
                },
                ApiParameter {
                    name: "include".to_string(),
                    param_type: "string".to_string(),
                    description: "Include additional data (conversations, requester, stats)".to_string(),
                    required: false,
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
                    description: "Number of records per page (max 100)".to_string(),
                    required: false,
                    default: Some("30".to_string()),
                },
                ApiParameter {
                    name: "filter".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter tickets by predefined filters".to_string(),
                    required: false,
                    default: None,
                },
            ],
            curl_example: Some(r#"curl -v -u yourapikey:X -X GET "https://domain.freshservice.com/api/v2/tickets?page=1&per_page=30""#.to_string()),
        });

        endpoints.push(ApiEndpoint {
            name: "Update Ticket".to_string(),
            description: "Update an existing ticket".to_string(),
            method: "PUT".to_string(),
            path: "/api/v2/tickets/{id}".to_string(),
            parameters: vec![
                ApiParameter {
                    name: "id".to_string(),
                    param_type: "integer".to_string(),
                    description: "Unique identifier of the ticket".to_string(),
                    required: true,
                    default: None,
                },
                ApiParameter {
                    name: "priority".to_string(),
                    param_type: "integer".to_string(),
                    description: "Priority of the ticket".to_string(),
                    required: false,
                    default: None,
                },
                ApiParameter {
                    name: "status".to_string(),
                    param_type: "integer".to_string(),
                    description: "Status of the ticket".to_string(),
                    required: false,
                    default: None,
                },
            ],
            curl_example: Some(r#"curl -v -u yourapikey:X -H "Content-Type: application/json" -d '{"priority":2,"status":3}' -X PUT "https://domain.freshservice.com/api/v2/tickets/1""#.to_string()),
        });

        endpoints.push(ApiEndpoint {
            name: "Delete Ticket".to_string(),
            description: "Delete a ticket permanently".to_string(),
            method: "DELETE".to_string(),
            path: "/api/v2/tickets/{id}".to_string(),
            parameters: vec![
                ApiParameter {
                    name: "id".to_string(),
                    param_type: "integer".to_string(),
                    description: "Unique identifier of the ticket to delete".to_string(),
                    required: true,
                    default: None,
                },
            ],
            curl_example: Some(r#"curl -v -u yourapikey:X -X DELETE "https://domain.freshservice.com/api/v2/tickets/1""#.to_string()),
        });

        Ok(endpoints)
    }
}

impl Default for FreshserviceScraper {
    fn default() -> Self {
        Self::new()
    }
}