# Freshservice RAG (Retrieval-Augmented Generation)

A Rust-based system for scraping, parsing, and querying the Freshservice API documentation using Retrieval-Augmented Generation (RAG) techniques. This project enables you to:

- Scrape and structure Freshservice API documentation
- Query the documentation using natural language
- Get relevant API endpoints, parameters, and cURL examples
- Integrate with LLMs (e.g., Groq) for answer generation
- Run a web server with REST endpoints for querying

## Features
- **Scraper:** Extracts endpoints, parameters, and examples from the Freshservice API docs
- **RAG Pipeline:** Finds and ranks relevant endpoints for a user query
- **Web API:** Query endpoint for natural language questions, health and debug endpoints
- **LLM Integration:** Uses Groq API for answer generation from context

## Getting Started

### Prerequisites
- Rust (edition 2021 or later)
- [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html)

### Installation
1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/freshservice-rag.git
   cd freshservice-rag
   ```
2. Install dependencies:
   ```bash
   cargo build
   ```

### Usage

#### 1. Scrape Documentation
Scrape the Freshservice API docs and save as JSON:
```bash
cargo run -- scrape --output data/scraped/documentation.json
```

#### 2. Start the Web Server
Set your Groq API key (optional, for LLM answers):
```bash
export GROQ_API_KEY=your_groq_api_key
```
Start the server:
```bash
cargo run -- serve --port 8080
```

#### 3. Query the API
Send a POST request to `http://localhost:8080/query` with JSON body:
```json
{
  "query": "How do I create a ticket?"
}
```

#### 4. Health and Debug Endpoints
- `GET /health` — Health check
- `GET /debug` — List available endpoints

## Project Structure
- `src/scraper/` — Scraper logic for Freshservice docs
- `src/rag/` — RAG pipeline for matching and ranking endpoints
- `src/web/` — Web server and API routes
- `src/llm/` — LLM (Groq) integration
- `data/scraped/` — Scraped documentation output

## Environment Variables
- `GROQ_API_KEY` — (Optional) API key for Groq LLM integration. If not set, a placeholder is used.

