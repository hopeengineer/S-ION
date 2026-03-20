use serde::{Deserialize, Serialize};

// ──────────────────────────────────────────────────
// Memory Router (Reflective Hook)
// ──────────────────────────────────────────────────

/// A memory fact extracted by the Reflective Hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMemory {
    pub content: String,
    pub category: String,  // preference, fact, decision, observation
    pub is_global: bool,
}

/// System prompt for the Reflective Hook micro-LLM call.
const REFLECTION_PROMPT: &str = r#"You are S-ION's memory extractor. Given a conversation exchange between a user and an AI assistant, extract 0-3 memorable facts.

Rules:
- Only extract GENUINELY important facts, preferences, or decisions. Not every exchange is memorable.
- Tag each as: "preference" (user likes/dislikes), "fact" (personal info, constants), "decision" (project-specific architectural choices), or "observation" (temporal state that may change).
- Set is_global=true for preferences and facts (apply across all projects). Set is_global=false for decisions and observations (project-specific).
- If nothing is worth remembering, return an empty array.
- Be extremely selective. Quality over quantity.

Return ONLY a JSON array (no markdown, no explanation):
[{"content": "...", "category": "preference", "is_global": true}]
Or: []"#;

/// Extract memorable facts from a conversation exchange.
/// Uses a lightweight LLM call (Gemini Flash Lite or similar cheap model).
pub async fn extract_memories(
    user_message: &str,
    assistant_response: &str,
    api_key: &str,
    triage_model_url: &str,
) -> Result<Vec<ExtractedMemory>, String> {
    let exchange = format!(
        "USER: {}\n\nASSISTANT: {}",
        &user_message[..user_message.len().min(2000)],
        &assistant_response[..assistant_response.len().min(2000)]
    );

    let body = serde_json::json!({
        "contents": [{
            "parts": [{"text": exchange}]
        }],
        "systemInstruction": {
            "parts": [{"text": REFLECTION_PROMPT}]
        },
        "generationConfig": {
            "temperature": 0.1,
            "maxOutputTokens": 500,
            "responseMimeType": "application/json"
        }
    });

    let client = reqwest::Client::new();
    let resp = client.post(triage_model_url)
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send().await
        .map_err(|e| format!("Reflection LLM call failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Reflection LLM error {}: {}", status, text));
    }

    let result: serde_json::Value = resp.json().await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    // Extract text from Gemini response
    let text = result["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .unwrap_or("[]");

    // Parse the JSON array
    let memories: Vec<ExtractedMemory> = serde_json::from_str(text)
        .unwrap_or_else(|_| {
            // Try to extract JSON from markdown code blocks
            let cleaned = text.trim()
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();
            serde_json::from_str(cleaned).unwrap_or_default()
        });

    if !memories.is_empty() {
        println!("🧠 Reflective Hook: extracted {} memories", memories.len());
        for mem in &memories {
            println!("   [{}] {} ({})", mem.category, &mem.content[..mem.content.len().min(60)],
                if mem.is_global { "global" } else { "project" });
        }
    }

    Ok(memories)
}
