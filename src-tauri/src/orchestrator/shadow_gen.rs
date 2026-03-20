use crate::orchestrator::shadow_scanner::WorkspaceScan;
use crate::orchestrator::shadow_temporal::HotSpotsReport;
use crate::orchestrator::SamLogic;
use std::collections::HashMap;
use std::env;

// ──────────────────────────────────────────────────
// Shadow Doc Generation via LLM
// ──────────────────────────────────────────────────

/// Generate all shadow docs for a workspace using LLM analysis.
/// Returns a map of filename → content for each generated doc.
pub async fn generate_shadow_docs(
    scan: &WorkspaceScan,
    hotspots: &HotSpotsReport,
    sam_logic: &SamLogic,
) -> Result<HashMap<String, String>, String> {
    let mut docs = HashMap::new();

    // Build a compact context string from the scan
    let context = build_scan_context(scan, hotspots);

    println!("📝 Generating shadow docs ({} chars of context)", context.len());

    // Generate each doc
    let architecture = generate_doc(
        &context,
        "ARCHITECTURE",
        "Summarize this project's architecture in ≤400 tokens. Include: component breakdown, \
         data flow between components, key entry points, and how the pieces connect. \
         Write as concise markdown with headers. No preamble, no filler.",
        sam_logic,
    ).await?;
    docs.insert("ARCHITECTURE.md".into(), architecture);

    let stack = generate_doc(
        &context,
        "STACK",
        "Summarize this project's tech stack in ≤200 tokens. Include: languages, frameworks, \
         build tools, package managers, runtime requirements, and versions if visible. \
         Write as a concise markdown table. No preamble.",
        sam_logic,
    ).await?;
    docs.insert("STACK.md".into(), stack);

    let state = generate_doc(
        &context,
        "STATE",
        "Summarize this project's state management in ≤400 tokens. Include: where state lives \
         (React state, Redux, context, database, config files), how state flows between \
         frontend and backend, key stateful entities. Write as concise markdown. No preamble.",
        sam_logic,
    ).await?;
    docs.insert("STATE.md".into(), state);

    let patterns = generate_doc(
        &context,
        "PATTERNS",
        "Identify the top 5-8 design patterns and conventions used in this project. \
         Include: naming conventions, architecture patterns (MVC, MVVM, etc.), error handling \
         approach, file organization rules, and any anti-patterns to avoid. \
         Write ≤300 tokens as concise markdown. No preamble.",
        sam_logic,
    ).await?;
    docs.insert("PATTERNS.md".into(), patterns);

    let gotchas = generate_doc(
        &context,
        "GOTCHAS",
        "List potential gotchas, pitfalls, and non-obvious behaviors in this project. \
         Consider: complex build steps, environment setup quirks, common error sources, \
         files that are auto-generated, and any hard-won lessons from the hot spots data. \
         Write ≤200 tokens as concise markdown bullets. No preamble.",
        sam_logic,
    ).await?;
    docs.insert("GOTCHAS.md".into(), gotchas);

    println!("✅ Generated {} shadow docs", docs.len());

    Ok(docs)
}

/// Build a compact context string from the scan data for LLM consumption.
fn build_scan_context(scan: &WorkspaceScan, hotspots: &HotSpotsReport) -> String {
    let mut ctx = String::new();

    // Tech stack summary
    ctx.push_str("## Tech Stack\n");
    ctx.push_str(&format!("Languages: {}\n", scan.stack.languages.join(", ")));
    ctx.push_str(&format!("Frameworks: {}\n", scan.stack.frameworks.join(", ")));
    ctx.push_str(&format!("Build: {}\n", scan.stack.build_tools.join(", ")));
    ctx.push_str(&format!("Type: {}\n\n", scan.stack.project_type));

    // Stats
    ctx.push_str(&format!(
        "## Stats\n{} files, {} dirs, {}KB, {} source files\n\n",
        scan.stats.total_files, scan.stats.total_dirs,
        scan.stats.total_size_kb, scan.stats.source_files
    ));

    // File tree (compact — dirs and source files only, skip assets)
    ctx.push_str("## File Tree\n");
    let skip_ext = ["png", "jpg", "jpeg", "gif", "svg", "ico", "woff", "woff2", "ttf", "eot", "mp4", "webm"];
    for f in &scan.files {
        if f.is_dir || (SOURCE_EXT.contains(&f.extension.as_str()) && f.size < 500_000) {
            // Skip deeply nested paths to save tokens
            if f.path.matches('/').count() <= 3 {
                if f.is_dir {
                    ctx.push_str(&format!("📁 {}/\n", f.path));
                } else if !skip_ext.contains(&f.extension.as_str()) {
                    ctx.push_str(&format!("  {} ({}KB)\n", f.path, f.size / 1024));
                }
            }
        }
    }
    ctx.push('\n');

    // Key files (truncated contents)
    ctx.push_str("## Key Files\n");
    for (path, content) in &scan.key_files {
        // Limit each key file to ~1000 chars to stay within token budget
        let truncated = if content.len() > 1000 {
            format!("{}...(truncated)", &content[..1000])
        } else {
            content.clone()
        };
        ctx.push_str(&format!("### {}\n```\n{}\n```\n\n", path, truncated));
    }

    // Dependencies (compact)
    if !scan.dependencies.is_empty() {
        ctx.push_str("## Dependencies (imports)\n");
        for (file, deps) in &scan.dependencies {
            if deps.len() <= 10 { // Skip files with too many deps to save tokens
                ctx.push_str(&format!("{} → {}\n", file, deps.join(", ")));
            }
        }
        ctx.push('\n');
    }

    // Hot spots
    if !hotspots.spots.is_empty() {
        ctx.push_str(&format!("## Hot Spots ({} commits in 30d)\n", hotspots.total_commits_30d));
        for spot in hotspots.spots.iter().take(10) {
            ctx.push_str(&format!(
                "🔥 {} — {}x edits ({})\n",
                spot.file, spot.edits_30d, spot.risk
            ));
        }
    }

    ctx
}

const SOURCE_EXT: &[&str] = &[
    "ts", "tsx", "js", "jsx", "rs", "py", "go", "java", "kt",
    "swift", "cs", "cpp", "c", "h", "rb", "php", "vue", "svelte",
];

/// Call the Analyst (DeepSeek) to generate a single shadow doc.
async fn generate_doc(
    context: &str,
    doc_name: &str,
    instruction: &str,
    sam_logic: &SamLogic,
) -> Result<String, String> {
    let api_key = env::var(&sam_logic.swarm.analyst.api_env_key)
        .map_err(|_| format!("Missing env: {}", sam_logic.swarm.analyst.api_env_key))?;

    if api_key.is_empty() {
        return Err(format!("{} is empty", sam_logic.swarm.analyst.api_env_key));
    }

    let system_prompt = format!(
        "You are a senior software architect analyzing a codebase to produce a concise reference doc.\n\
         You are generating the `{}` shadow doc.\n\n\
         Rules:\n\
         - Output ONLY the markdown content. No meta-commentary.\n\
         - Stay within the token budget specified.\n\
         - Be precise and actionable — someone will use this to navigate the codebase.\n\
         - Start with a `# {}` header.\n\n\
         Here is the project context:\n\n{}",
        doc_name, doc_name, context
    );

    let client = reqwest::Client::new();
    let response = client
        .post(format!(
            "{}/chat/completions",
            sam_logic.swarm.analyst.api_base_url
        ))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": sam_logic.swarm.analyst.model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": instruction }
            ],
            "temperature": 0.2,
            "max_tokens": 1024
        }))
        .send()
        .await
        .map_err(|e| format!("{} generation failed: {}", doc_name, e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read {} response: {}", doc_name, e))?;

    if !status.is_success() {
        return Err(format!("{} API error ({}): {}", doc_name, status, body));
    }

    let resp_json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON from {}: {}", doc_name, e))?;

    let content = resp_json["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| format!("No content in {} response", doc_name))?;

    println!("📄 {}: {} chars generated", doc_name, content.len());

    Ok(content.to_string())
}
