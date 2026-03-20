use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

// ──────────────────────────────────────────────────
// Workspace Scan Result
// ──────────────────────────────────────────────────

/// The complete scan output for a user's project.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct WorkspaceScan {
    /// Absolute path to the workspace root
    pub root: String,
    /// Detected tech stack
    pub stack: TechStack,
    /// File tree (flat list of relative paths with metadata)
    pub files: Vec<FileEntry>,
    /// Key files extracted for LLM context (relative path → first N lines)
    pub key_files: HashMap<String, String>,
    /// Shallow dependency map (file → list of files it imports)
    pub dependencies: HashMap<String, Vec<String>>,
    /// Summary stats
    pub stats: ScanStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TechStack {
    /// Primary language(s) detected
    pub languages: Vec<String>,
    /// Framework(s) detected
    pub frameworks: Vec<String>,
    /// Build tool(s)
    pub build_tools: Vec<String>,
    /// Package manager(s)
    pub package_managers: Vec<String>,
    /// Detected project type: "web_app", "cli", "library", "desktop_app", "api", "unknown"
    pub project_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct FileEntry {
    /// Path relative to workspace root
    pub path: String,
    /// File extension (lowercase, no dot)
    pub extension: String,
    /// File size in bytes
    pub size: u32,
    /// true if directory
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ScanStats {
    pub total_files: u32,
    pub total_dirs: u32,
    pub total_size_kb: u32,
    pub source_files: u32,
    pub config_files: u32,
}

// ──────────────────────────────────────────────────
// Skip Patterns
// ──────────────────────────────────────────────────

const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "target", "dist", "build", ".next",
    "__pycache__", ".venv", "venv", ".tox", ".mypy_cache",
    ".gradle", ".idea", ".vscode", ".vs", "vendor",
    "Pods", ".dart_tool", ".pub-cache", "coverage",
    ".turbo", ".vercel", ".output", ".nuxt", ".cache",
    ".sion-shadow", // Don't recursively scan our own output
];

const SKIP_FILES: &[&str] = &[
    ".DS_Store", "Thumbs.db", "desktop.ini",
    "package-lock.json", "yarn.lock", "pnpm-lock.yaml",
    "Cargo.lock", "Gemfile.lock", "poetry.lock",
];

const SOURCE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "rs", "py", "go", "java", "kt",
    "swift", "cs", "cpp", "c", "h", "rb", "php", "vue", "svelte",
    "dart", "ex", "exs", "zig", "lua",
];

const CONFIG_EXTENSIONS: &[&str] = &[
    "json", "yaml", "yml", "toml", "xml", "ini", "env",
    "config", "conf", "properties",
];

// ──────────────────────────────────────────────────
// Scanner Implementation
// ──────────────────────────────────────────────────

/// Scans a workspace directory and produces a WorkspaceScan.
pub fn scan_workspace(root: &Path) -> Result<WorkspaceScan, String> {
    if !root.exists() {
        return Err(format!("Workspace path does not exist: {}", root.display()));
    }
    if !root.is_dir() {
        return Err(format!("Workspace path is not a directory: {}", root.display()));
    }

    println!("🔍 Scanning workspace: {}", root.display());

    let mut files = Vec::new();
    let mut total_dirs = 0u32;
    let mut total_size: u64 = 0;
    let mut source_count = 0u32;
    let mut config_count = 0u32;

    // Walk the file tree
    walk_dir(root, root, &mut files, &mut total_dirs, &mut total_size, &mut source_count, &mut config_count, 0)?;

    // Detect tech stack
    let stack = detect_tech_stack(root, &files);

    // Extract key files
    let key_files = extract_key_files(root, &files);

    // Build shallow dependency map
    let dependencies = build_dependency_map(root, &files);

    let stats = ScanStats {
        total_files: files.len() as u32,
        total_dirs,
        total_size_kb: (total_size / 1024) as u32,
        source_files: source_count,
        config_files: config_count,
    };

    println!(
        "✅ Scan complete: {} files, {} dirs, {}KB, {} source, {} config",
        stats.total_files, stats.total_dirs, stats.total_size_kb,
        stats.source_files, stats.config_files
    );
    println!("📦 Stack: {:?} | {:?} | {}", stack.languages, stack.frameworks, stack.project_type);

    Ok(WorkspaceScan {
        root: root.to_string_lossy().to_string(),
        stack,
        files,
        key_files,
        dependencies,
        stats,
    })
}

/// Recursively walk directory tree, respecting skip patterns.
fn walk_dir(
    root: &Path,
    current: &Path,
    files: &mut Vec<FileEntry>,
    total_dirs: &mut u32,
    total_size: &mut u64,
    source_count: &mut u32,
    config_count: &mut u32,
    depth: u32,
) -> Result<(), String> {
    // Safety: max depth to prevent runaway recursion
    if depth > 20 {
        return Ok(());
    }

    let entries = fs::read_dir(current)
        .map_err(|e| format!("Failed to read dir {}: {}", current.display(), e))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            // Skip ignored directories
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            *total_dirs += 1;

            let rel = path.strip_prefix(root).unwrap_or(&path);
            files.push(FileEntry {
                path: rel.to_string_lossy().to_string(),
                extension: String::new(),
                size: 0,
                is_dir: true,
            });

            walk_dir(root, &path, files, total_dirs, total_size, source_count, config_count, depth + 1)?;
        } else {
            // Skip ignored files
            if SKIP_FILES.contains(&name.as_str()) {
                continue;
            }

            let ext = path.extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            *total_size += size;

            if SOURCE_EXTENSIONS.contains(&ext.as_str()) {
                *source_count += 1;
            }
            if CONFIG_EXTENSIONS.contains(&ext.as_str()) {
                *config_count += 1;
            }

            let rel = path.strip_prefix(root).unwrap_or(&path);
            files.push(FileEntry {
                path: rel.to_string_lossy().to_string(),
                extension: ext,
                size: size as u32,
                is_dir: false,
            });
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────────
// Tech Stack Detection
// ──────────────────────────────────────────────────

fn detect_tech_stack(root: &Path, files: &[FileEntry]) -> TechStack {
    let mut languages = Vec::new();
    let mut frameworks = Vec::new();
    let mut build_tools = Vec::new();
    let mut package_managers = Vec::new();
    let mut project_type = "unknown".to_string();

    let file_names: HashSet<String> = files.iter()
        .filter(|f| !f.is_dir)
        .map(|f| {
            Path::new(&f.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .collect();

    let extensions: HashSet<String> = files.iter()
        .filter(|f| !f.is_dir)
        .map(|f| f.extension.clone())
        .collect();

    // Languages
    if extensions.contains("ts") || extensions.contains("tsx") { languages.push("TypeScript".into()); }
    if extensions.contains("js") || extensions.contains("jsx") { languages.push("JavaScript".into()); }
    if extensions.contains("rs") { languages.push("Rust".into()); }
    if extensions.contains("py") { languages.push("Python".into()); }
    if extensions.contains("go") { languages.push("Go".into()); }
    if extensions.contains("java") || extensions.contains("kt") { languages.push("Java/Kotlin".into()); }
    if extensions.contains("swift") { languages.push("Swift".into()); }
    if extensions.contains("dart") { languages.push("Dart".into()); }
    if extensions.contains("cs") { languages.push("C#".into()); }
    if extensions.contains("cpp") || extensions.contains("c") { languages.push("C/C++".into()); }

    // Frameworks
    if file_names.contains("tauri.conf.json") || file_names.contains("tauri.conf.json5") {
        frameworks.push("Tauri".into());
        project_type = "desktop_app".into();
    }
    if file_names.contains("next.config.js") || file_names.contains("next.config.ts") || file_names.contains("next.config.mjs") {
        frameworks.push("Next.js".into());
        project_type = "web_app".into();
    }
    if file_names.contains("vite.config.ts") || file_names.contains("vite.config.js") {
        build_tools.push("Vite".into());
    }
    if file_names.contains("nuxt.config.ts") || file_names.contains("nuxt.config.js") {
        frameworks.push("Nuxt".into());
        project_type = "web_app".into();
    }
    if file_names.contains("angular.json") {
        frameworks.push("Angular".into());
        project_type = "web_app".into();
    }
    if file_names.contains("svelte.config.js") {
        frameworks.push("SvelteKit".into());
        project_type = "web_app".into();
    }

    // Check package.json for React/Vue/etc.
    if let Ok(pj) = fs::read_to_string(root.join("package.json")) {
        if pj.contains("\"react\"") { frameworks.push("React".into()); }
        if pj.contains("\"vue\"") { frameworks.push("Vue".into()); }
        if pj.contains("\"express\"") { frameworks.push("Express".into()); project_type = "api".into(); }
        if pj.contains("\"fastify\"") { frameworks.push("Fastify".into()); project_type = "api".into(); }
    }

    // Check Cargo.toml for Rust frameworks
    if let Ok(ct) = fs::read_to_string(root.join("Cargo.toml"))
        .or_else(|_| fs::read_to_string(root.join("src-tauri/Cargo.toml")))
    {
        if ct.contains("actix") { frameworks.push("Actix".into()); project_type = "api".into(); }
        if ct.contains("axum") { frameworks.push("Axum".into()); project_type = "api".into(); }
        if ct.contains("tauri") { frameworks.push("Tauri".into()); project_type = "desktop_app".into(); }
    }

    // Check for Python frameworks
    if let Ok(req) = fs::read_to_string(root.join("requirements.txt"))
        .or_else(|_| fs::read_to_string(root.join("pyproject.toml")))
    {
        if req.contains("django") { frameworks.push("Django".into()); project_type = "web_app".into(); }
        if req.contains("flask") { frameworks.push("Flask".into()); project_type = "api".into(); }
        if req.contains("fastapi") { frameworks.push("FastAPI".into()); project_type = "api".into(); }
    }

    // Package managers
    if file_names.contains("package.json") { package_managers.push("npm".into()); }
    if file_names.contains("pnpm-lock.yaml") || file_names.contains("pnpm-workspace.yaml") {
        package_managers.push("pnpm".into());
    }
    if file_names.contains("yarn.lock") { package_managers.push("yarn".into()); }
    if file_names.contains("Cargo.toml") { package_managers.push("cargo".into()); }
    if file_names.contains("go.mod") { package_managers.push("go mod".into()); }
    if file_names.contains("requirements.txt") || file_names.contains("pyproject.toml") {
        package_managers.push("pip".into());
    }

    // Build tools
    if file_names.contains("webpack.config.js") { build_tools.push("Webpack".into()); }
    if file_names.contains("tsconfig.json") { build_tools.push("TypeScript Compiler".into()); }
    if file_names.contains("Makefile") { build_tools.push("Make".into()); }
    if file_names.contains("Dockerfile") { build_tools.push("Docker".into()); }

    // Fallback project type from languages
    if project_type == "unknown" {
        if languages.contains(&"Rust".to_string()) && !frameworks.iter().any(|f| f == "Tauri") {
            project_type = "cli".to_string();
        } else if languages.contains(&"Python".to_string()) {
            project_type = "cli".to_string();
        }
    }

    // Deduplicate
    languages.dedup();
    frameworks.dedup();

    TechStack {
        languages,
        frameworks,
        build_tools,
        package_managers,
        project_type,
    }
}

// ──────────────────────────────────────────────────
// Key File Extraction
// ──────────────────────────────────────────────────

/// Priority files to extract for LLM context.
const KEY_FILE_PATTERNS: &[&str] = &[
    "README.md", "README", "readme.md",
    "package.json", "Cargo.toml", "pyproject.toml", "go.mod",
    "tsconfig.json", "vite.config.ts", "next.config.ts", "next.config.js",
    "tauri.conf.json",
    ".env.example", ".env.sample",
];

/// Entry point patterns to detect.
const ENTRY_POINT_PATTERNS: &[&str] = &[
    "src/main.rs", "src/lib.rs", "src/main.ts", "src/main.tsx",
    "src/index.ts", "src/index.tsx", "src/index.js",
    "src/App.tsx", "src/App.vue", "src/App.svelte",
    "app/page.tsx", "app/layout.tsx",
    "pages/index.tsx", "pages/_app.tsx",
    "main.py", "app.py", "manage.py",
    "main.go", "cmd/main.go",
];

/// Max lines to extract per key file.
const KEY_FILE_MAX_LINES: usize = 100;

fn extract_key_files(root: &Path, files: &[FileEntry]) -> HashMap<String, String> {
    let mut key_files = HashMap::new();
    let file_paths: HashSet<String> = files.iter()
        .filter(|f| !f.is_dir)
        .map(|f| f.path.clone())
        .collect();

    // Extract known key files
    for pattern in KEY_FILE_PATTERNS.iter().chain(ENTRY_POINT_PATTERNS.iter()) {
        if file_paths.contains(*pattern) {
            if let Ok(content) = fs::read_to_string(root.join(pattern)) {
                let truncated: String = content.lines()
                    .take(KEY_FILE_MAX_LINES)
                    .collect::<Vec<&str>>()
                    .join("\n");
                key_files.insert(pattern.to_string(), truncated);
            }
        }
    }

    // Also check nested Cargo.toml (e.g., src-tauri/Cargo.toml)
    for f in files.iter().filter(|f| !f.is_dir) {
        let name = Path::new(&f.path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if (name == "Cargo.toml" || name == "tauri.conf.json") && !key_files.contains_key(&f.path) {
            if let Ok(content) = fs::read_to_string(root.join(&f.path)) {
                let truncated: String = content.lines()
                    .take(KEY_FILE_MAX_LINES)
                    .collect::<Vec<&str>>()
                    .join("\n");
                key_files.insert(f.path.clone(), truncated);
            }
        }
    }

    key_files
}

// ──────────────────────────────────────────────────
// Shallow Dependency Map (DAG, depth 3)
// ──────────────────────────────────────────────────

/// Build a shallow dependency map from import/require statements.
/// DAG-limited to depth 3 to prevent graph explosion.
fn build_dependency_map(root: &Path, files: &[FileEntry]) -> HashMap<String, Vec<String>> {
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();

    for f in files.iter().filter(|f| !f.is_dir && SOURCE_EXTENSIONS.contains(&f.extension.as_str())) {
        // Only process files smaller than 100KB to avoid huge files
        if f.size > 100_000 {
            continue;
        }

        let full_path = root.join(&f.path);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let mut file_deps = Vec::new();

            // Only scan first 50 lines for imports (performance)
            for line in content.lines().take(50) {
                let trimmed = line.trim();

                // TypeScript/JavaScript: import ... from "..."
                if (trimmed.starts_with("import ") || trimmed.starts_with("export ")) && trimmed.contains("from ") {
                    if let Some(path) = extract_import_path(trimmed) {
                        if path.starts_with('.') {
                            file_deps.push(path);
                        }
                    }
                }
                // Rust: use crate::..., mod ...
                else if trimmed.starts_with("use crate::") || trimmed.starts_with("mod ") {
                    let module = trimmed
                        .trim_start_matches("use ")
                        .trim_start_matches("mod ")
                        .trim_end_matches(';')
                        .trim();
                    file_deps.push(module.to_string());
                }
                // Python: from ... import ..., import ...
                else if trimmed.starts_with("from ") || (trimmed.starts_with("import ") && !trimmed.contains("from")) {
                    let module = trimmed
                        .trim_start_matches("from ")
                        .trim_start_matches("import ")
                        .split_whitespace()
                        .next()
                        .unwrap_or("");
                    if !module.is_empty() {
                        file_deps.push(module.to_string());
                    }
                }
                // Go: import "..."
                else if trimmed.starts_with("import ") && trimmed.contains('"') {
                    if let Some(path) = extract_import_path(trimmed) {
                        file_deps.push(path);
                    }
                }
            }

            if !file_deps.is_empty() {
                deps.insert(f.path.clone(), file_deps);
            }
        }
    }

    deps
}

/// Extract the path string from an import statement.
fn extract_import_path(line: &str) -> Option<String> {
    // Find quoted string: "..." or '...'
    let rest = line;
    for quote in ['"', '\''] {
        if let Some(start) = rest.rfind(quote) {
            let before = &rest[..start];
            if let Some(open) = before.rfind(quote) {
                let path = &rest[open + 1..start];
                if !path.is_empty() {
                    return Some(path.to_string());
                }
            }
        }
    }
    None
}
