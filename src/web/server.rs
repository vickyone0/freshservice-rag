use crate::rag::RagPipeline;
use crate::scraper::FreshserviceScraper;
use crate::llm::GroqClient;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use warp::Filter;

#[derive(Debug, Deserialize)]
struct QueryRequest {
    query: String,
}

#[derive(Debug, Serialize)]
struct QueryResponse {
    answer: String,
    sources: Vec<String>,
    confidence: f32,
}

pub async fn run_server(port: u16) -> Result<()> {
    // Initialize components
    let scraper = FreshserviceScraper::new();
    let documentation = scraper.scrape_ticket_attributes().await?;
    let rag_pipeline = Arc::new(RagPipeline::new(documentation));
    
    // Initialize Groq client
    let groq_client = Arc::new(GroqClient::new(
        std::env::var("GROQ_API_KEY").unwrap_or_else(|_| {
            eprintln!("Warning: GROQ_API_KEY not set. Using placeholder key.");
            "gsk_placeholder_key".to_string()
        }),
    ));
    
    let rag_pipeline_filter = rag_pipeline.clone();
    let groq_client_filter = groq_client.clone();
    
    // Define routes
    let query_route = warp::path("query")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(move |request: QueryRequest| {
            let rag_pipeline = rag_pipeline_filter.clone();
            let groq_client = groq_client_filter.clone();
            
            async move {
                // Process query using RAG pipeline
                let relevant_endpoints = rag_pipeline.find_relevant_endpoints(&request.query);
                let context = rag_pipeline.format_context(relevant_endpoints);
                
                println!("Context length: {} characters", context.len());
                println!("Query: {}", request.query);
                
                // Use Groq to generate answer from context
                let answer = if context.trim().is_empty() {
                    "I couldn't find any relevant information in the Freshservice documentation for your query. Please try asking about specific API endpoints like creating tickets, updating tickets, or ticket attributes.".to_string()
                } else {
                    match groq_client.generate_answer(&request.query, &context).await {
                        Ok(answer) => answer,
                        Err(e) => {
                            eprintln!("Groq API error: {}", e);
                            format!("I found some relevant information but encountered an error processing it. Here's what I found:\n\n{}", context)
                        }
                    }
                };
                
                let sources: Vec<String> = vec!["Freshservice API Documentation".to_string()];
                
                Ok::<_, warp::Rejection>(warp::reply::json(&QueryResponse {
                    answer,
                    sources,
                    confidence: 0.8,
                }))
            }
        });
    
    let health_route = warp::path("health")
        .map(|| warp::reply::json(&serde_json::json!({"status": "healthy"})));

    // Debug route to see available endpoints
    let debug_route = warp::path("debug")
        .and(warp::get())
        .map(move || {
            let endpoints_count = rag_pipeline.documentation.endpoints.len();
            let endpoint_names: Vec<String> = rag_pipeline.documentation.endpoints
                .iter()
                .map(|e| e.name.clone())
                .collect();
            
            warp::reply::json(&serde_json::json!({
                "total_endpoints": endpoints_count,
                "endpoints": endpoint_names,
                "sample_endpoint": &rag_pipeline.documentation.endpoints.get(0)
            }))
        });
    
    let routes = query_route
        .or(health_route)
        .or(debug_route)
        .with(warp::cors().allow_any_origin());
    
    println!("Server running on http://localhost:{}", port);
    println!("Make sure to set GROQ_API_KEY environment variable");
    warp::serve(routes)
        .run(([127, 0, 0, 1], port))
        .await;
    
    Ok(())
}