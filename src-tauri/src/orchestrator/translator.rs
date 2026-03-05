use crate::orchestrator::SamLogic;
use std::env;

/// Translates a raw technical error into an empathetic, non-technical "Grandma-Speak" message.
/// Uses DeepSeek V3.2 (The Analyst) for cost-effective, high-quality logic.
pub async fn translate_error_to_grandma(raw_error: &str, sam_logic: &SamLogic) -> String {
    let api_key = match env::var(&sam_logic.swarm.analyst.api_env_key) {
        Ok(key) => key,
        Err(_) => return "Oh dear, I couldn't find my glasses (API key missing)! Let's try again in a bit, sweetie.".to_string(),
    };

    if api_key.is_empty() {
        return "Oh honey, my glasses seem to be missing (API key empty). We'll need to find them before I can help you!".to_string();
    }

    let system_prompt = format!(
        r#"You are an empathetic, patient grandmother explaining a computer glitch to your grandchild. 
You do not use ANY technical jargon. You offer reassurance and a simple next step.
Your grandkids are building "S-ION", an AI browser.
You must adhere strictly to the following Constitution rules: {}
Translate the following error into your grandmother persona."#,
        sam_logic.constitution.zero_assumption_directive
    );

    let client = reqwest::Client::new();
    let response = match client
        .post(format!("{}/chat/completions", sam_logic.swarm.analyst.api_base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": sam_logic.swarm.analyst.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": raw_error }
            ],
            "temperature": 0.5,
            "max_tokens": 150
        }))
        .send()
        .await
    {
        Ok(res) => res,
        Err(_) => return "Oh my stars, the internet tubes seem to be clogged right now! Let's pour a cup of tea and try connecting again later.".to_string(),
    };

    if !response.status().is_success() {
        return "Bless your heart, the Analyst model is having a little nap right now. Let's try again when it wakes up!".to_string();
    }

    let body = match response.text().await {
        Ok(b) => b,
        Err(_) => {
            return "I heard something, but I couldn't quite make it out! Let's ask again."
                .to_string()
        }
    };

    let resp_json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(j) => j,
        Err(_) => return "Oh dear, the message came back all jumbled up like a puzzle! Let's try sending it once more.".to_string(),
    };

    if let Some(content) = resp_json["choices"][0]["message"]["content"].as_str() {
        content.to_string()
    } else {
        "Well isn't that silly, they sent us a blank letter! Let's try asking them again, sweetie."
            .to_string()
    }
}
