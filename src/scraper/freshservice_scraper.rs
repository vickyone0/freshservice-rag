use crate::models::{ApiEndpoint, ApiParameter, ScrapedDocumentation};
use anyhow::{Result, Context};
use scraper::{Html, Selector, ElementRef};
use regex::Regex;

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
        println!("Scraping tickets from: {}", url);
        
        let response = self.client.get(url).send().await
            .context("Failed to fetch documentation page")?;
        let html_content = response.text().await?;
        
        println!("Fetched HTML: {} bytes", html_content.len());
        println!("════════════════════════════════════════════════════════════════");
        
        let document = Html::parse_document(&html_content);
        let endpoints = self.extract_ticket_endpoints(&document).await?;
        
        println!("════════════════════════════════════════════════════════════════");
        println!("Total ticket endpoints: {}", endpoints.len());
        
        if !endpoints.is_empty() {
            println!("\nEndpoints:");
            for (i, ep) in endpoints.iter().enumerate() {
                println!("  {}. {} {}", i + 1, ep.method, ep.path);
            }
        }
        
        Ok(ScrapedDocumentation {
            base_url: self.base_url.clone(),
            endpoints,
            scraped_at: chrono::Utc::now(),
        })
    }
    
    async fn extract_ticket_endpoints(&self, document: &Html) -> Result<Vec<ApiEndpoint>> {
        let mut endpoints = Vec::new();
        let mut seen = std::collections::HashSet::new();
        
        // Strategy 1: Extract from ticket div sections
        println!("Extracting from ticket divs...");
        if let Ok(selector) = Selector::parse("div[id*='ticket']") {
            for div in document.select(&selector) {
                if let Some(id) = div.value().id() {
                    if id == "tickets" || id == "tickets-panel" || id == "ticket_attributes" {
                        continue;
                    }
                    
                    if let Some(ep) = self.parse_section(div).await {
                        let key = format!("{} {}", ep.method, ep.path);
                        if seen.insert(key) {
                            endpoints.push(ep);
                        }
                    }
                }
            }
        }
        
        // Strategy 2: Extract from code blocks in ticket section
        println!("Extracting from code blocks...");
        if let Ok(selector) = Selector::parse("div#tickets") {
            if let Some(section) = document.select(&selector).next() {
                let code_eps = self.extract_from_code_blocks(section).await?;
                for ep in code_eps {
                    let key = format!("{} {}", ep.method, ep.path);
                    if seen.insert(key) {
                        endpoints.push(ep);
                    }
                }
            }
        }
        
        if endpoints.is_empty() {
            println!("No endpoints found, using fallback");
        }
        
        Ok(endpoints)
    }
    
    async fn parse_section(&self, element: ElementRef<'_>) -> Option<ApiEndpoint> {
        // Get description from h2
        let description = Selector::parse("h2").ok()
            .and_then(|sel| element.select(&sel).next())
            .map(|h2| h2.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "API endpoint".to_string());
        
        // Get curl example
        let curl = Selector::parse("pre, .highlight").ok()
            .and_then(|sel| element.select(&sel).next())
            .map(|code| code.text().collect::<String>().trim().to_string())?;
        
        if !curl.contains("curl") {
            return None;
        }
        
        // Extract method
        let method = if curl.contains("-X POST") { "POST" }
        else if curl.contains("-X PUT") { "PUT" }
        else if curl.contains("-X DELETE") { "DELETE" }
        else if curl.contains("-X PATCH") { "PATCH" }
        else { "GET" };
        
        let path = self.extract_path(&curl)?;
        
        println!("  {} {}", method, path);
        
        Some(ApiEndpoint {
            name: description.clone(),
            description,
            method: method.to_string(),
            path,
            parameters: self.extract_parameters(element),
            curl_example: Some(curl),
        })
    }
    
    async fn extract_from_code_blocks(&self, section: ElementRef<'_>) -> Result<Vec<ApiEndpoint>> {
        let mut endpoints = Vec::new();
        let mut seen = std::collections::HashSet::new();
        
        if let Ok(selector) = Selector::parse("pre, .highlight") {
            for code_elem in section.select(&selector) {
                let curl = code_elem.text().collect::<String>();
                
                if !curl.contains("curl") || !curl.contains("/tickets") {
                    continue;
                }
                
                let method = if curl.contains("-X POST") { "POST" }
                else if curl.contains("-X PUT") { "PUT" }
                else if curl.contains("-X DELETE") { "DELETE" }
                else if curl.contains("-X PATCH") { "PATCH" }
                else { "GET" };
                
                if let Some(path) = self.extract_path(&curl) {
                    if !path.contains("/tickets") {
                        continue;
                    }
                    
                    let key = format!("{} {}", method, path);
                    if !seen.insert(key.clone()) {
                        continue;
                    }
                    
                    let description = self.find_description(code_elem)
                        .unwrap_or_else(|| self.infer_description(&path, method));
                    
                    println!("     {} {}", method, path);
                    
                    endpoints.push(ApiEndpoint {
                        name: key,
                        description,
                        method: method.to_string(),
                        path,
                        parameters: vec![],
                        curl_example: Some(curl.trim().to_string()),
                    });
                }
            }
        }
        
        Ok(endpoints)
    }
    
    fn extract_path(&self, text: &str) -> Option<String> {
        let patterns = vec![
            r"https://[^/]+(/api/v2/[a-zA-Z0-9/_\-{}]+)",
            r"'(/api/v2/[a-zA-Z0-9/_\-{}]+)'",
            r#""(/api/v2/[a-zA-Z0-9/_\-{}]+)""#,
            r"(/api/v2/[a-zA-Z0-9/_\-{}]+)",
        ];
        
        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(cap) = re.captures(text) {
                    if let Some(m) = cap.get(1) {
                        return Some(m.as_str().trim_end_matches('\'').trim_end_matches('"').to_string());
                    }
                }
            }
        }
        None
    }
    
    fn find_description(&self, code_elem: ElementRef<'_>) -> Option<String> {
        let mut current = code_elem;
        
        for _ in 0..5 {
            if let Some(parent) = current.parent().and_then(ElementRef::wrap) {
                // Check for div ID
                if let Some(id) = parent.value().id() {
                    if !id.is_empty() {
                        return Some(id.replace('_', " ")
                            .split_whitespace()
                            .map(|w| {
                                let mut c = w.chars();
                                match c.next() {
                                    None => String::new(),
                                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" "));
                    }
                }
                
                // Check for h2
                if let Ok(sel) = Selector::parse("h2") {
                    if let Some(h2) = parent.select(&sel).next() {
                        let text = h2.text().collect::<String>().trim().to_string();
                        if !text.is_empty() && text.len() < 100 {
                            return Some(text);
                        }
                    }
                }
                
                current = parent;
            } else {
                break;
            }
        }
        None
    }
    
    fn infer_description(&self, path: &str, method: &str) -> String {
        match (method, path) {
            ("POST", p) if p.ends_with("/tickets") => "Create a Ticket",
            ("GET", p) if p.contains("/tickets/{id}") => "View a Ticket",
            ("GET", p) if p.ends_with("/tickets") => "List All Tickets",
            ("PUT", p) if p.contains("/tickets/{id}") => "Update a Ticket",
            ("DELETE", p) if p.contains("/tickets/{id}") => "Delete a Ticket",
            ("PUT", p) if p.contains("/restore") => "Restore a Ticket",
            ("POST", p) if p.contains("/notes") => "Create Ticket Note",
            ("GET", p) if p.contains("/notes") => "List Ticket Notes",
            ("PUT", p) if p.contains("/notes") => "Update Ticket Note",
            ("DELETE", p) if p.contains("/notes") => "Delete Ticket Note",
            ("POST", p) if p.contains("/tasks") => "Create Ticket Task",
            ("GET", p) if p.contains("/tasks") => "View Ticket Tasks",
            ("PUT", p) if p.contains("/tasks") => "Update Ticket Task",
            ("DELETE", p) if p.contains("/tasks") => "Delete Ticket Task",
            ("POST", p) if p.contains("/time_entries") => "Create Time Entry",
            ("GET", p) if p.contains("/time_entries") => "View Time Entries",
            ("PUT", p) if p.contains("/time_entries") => "Update Time Entry",
            ("DELETE", p) if p.contains("/time_entries") => "Delete Time Entry",
            _ => "Ticket Operation",
        }.to_string()
    }
    
    fn extract_parameters(&self, element: ElementRef<'_>) -> Vec<ApiParameter> {
        let mut params = Vec::new();
        
        if let Ok(selector) = Selector::parse("table") {
            for table in element.select(&selector) {
                let text = table.text().collect::<String>().to_lowercase();
                
                if text.contains("parameter") || text.contains("attribute") || text.contains("field") {
                    if let Ok(row_sel) = Selector::parse("tr") {
                        for row in table.select(&row_sel).skip(1) {
                            if let Some(param) = self.parse_param_row(row) {
                                params.push(param);
                            }
                        }
                    }
                }
            }
        }
        
        params
    }
    
    fn parse_param_row(&self, row: ElementRef<'_>) -> Option<ApiParameter> {
        let selector = Selector::parse("td").ok()?;
        let cells: Vec<_> = row.select(&selector)
            .map(|c| c.text().collect::<String>().trim().to_string())
            .collect();
        
        if cells.len() < 2 {
            return None;
        }
        
        let name = cells[0].clone();
        let desc = cells[1].clone();
        
        if name.is_empty() {
            return None;
        }
        
        let param_type = cells.get(2)
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| {
                if desc.to_lowercase().contains("integer") { "integer" }
                else if desc.to_lowercase().contains("boolean") { "boolean" }
                else if desc.to_lowercase().contains("array") { "array" }
                else { "string" }
            }.to_string());
        
        let required = desc.to_lowercase().contains("required");
        
        Some(ApiParameter {
            name,
            param_type,
            description: desc,
            required,
            default: None,
        })
    }
    
           
}

impl Default for FreshserviceScraper {
    fn default() -> Self {
        Self::new()
    }
}