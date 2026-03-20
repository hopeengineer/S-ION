use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tree_sitter::{Language, Parser, Node};

// ──────────────────────────────────────────────────
// Atlas Types
// ──────────────────────────────────────────────────

/// A single symbol extracted from a source file via AST parsing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct AtlasSymbol {
    pub name: String,
    /// "function", "class", "struct", "interface", "type", "enum",
    /// "trait", "impl", "component", "const", "method", "module", "macro"
    pub kind: String,
    /// 1-based line number
    pub line: u32,
    /// Declaration signature (first line, ≤200 chars)
    pub signature: String,
    /// Whether exported/public
    pub exported: bool,
    /// Parent scope (e.g., "SamLogic" for methods in impl SamLogic)
    pub parent: Option<String>,
}

/// The complete atlas for a workspace.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RepoAtlas {
    pub files: HashMap<String, Vec<AtlasSymbol>>,
    pub total_symbols: u32,
    pub total_files: u32,
}

// ──────────────────────────────────────────────────
// Directory Walker
// ──────────────────────────────────────────────────

const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "target", "dist", "build", ".next",
    "__pycache__", ".venv", "venv", "coverage", ".turbo",
    ".sion-shadow", "vendor", ".cache", ".svn", "pkg",
];

pub fn build_atlas(root: &Path) -> Result<RepoAtlas, String> {
    if !root.exists() || !root.is_dir() {
        return Err(format!("Invalid workspace: {}", root.display()));
    }

    println!("📊 Building atlas (tree-sitter): {}", root.display());

    let mut files: HashMap<String, Vec<AtlasSymbol>> = HashMap::new();
    let mut total_symbols = 0u32;
    let mut total_files = 0u32;

    index_dir(root, root, &mut files, &mut total_symbols, &mut total_files, 0)?;

    println!("📊 Atlas: {} symbols across {} files", total_symbols, total_files);

    Ok(RepoAtlas { files, total_symbols, total_files })
}

fn index_dir(
    root: &Path, current: &Path,
    files: &mut HashMap<String, Vec<AtlasSymbol>>,
    total_symbols: &mut u32, total_files: &mut u32, depth: u32,
) -> Result<(), String> {
    if depth > 15 { return Ok(()); }

    let entries = match fs::read_dir(current) { Ok(e) => e, Err(_) => return Ok(()) };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) { continue; }
            index_dir(root, &path, files, total_symbols, total_files, depth + 1)?;
        } else {
            let ext = path.extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();

            let lang = match get_language(&ext) { Some(l) => l, None => continue };

            // Skip very large files (>300KB)
            if entry.metadata().map(|m| m.len()).unwrap_or(0) > 300_000 { continue; }

            if let Ok(source) = fs::read_to_string(&path) {
                let symbols = parse_file(&source, lang, &ext);
                if !symbols.is_empty() {
                    let rel = path.strip_prefix(root).unwrap_or(&path);
                    *total_symbols += symbols.len() as u32;
                    *total_files += 1;
                    files.insert(rel.to_string_lossy().to_string(), symbols);
                }
            }
        }
    }
    Ok(())
}

fn get_language(ext: &str) -> Option<Language> {
    match ext {
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "js" | "jsx" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "c" | "h" => Some(tree_sitter_c::LANGUAGE.into()),
        _ => None,
    }
}

fn parse_file(source: &str, language: Language, ext: &str) -> Vec<AtlasSymbol> {
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() { return Vec::new(); }
    let tree = match parser.parse(source, None) { Some(t) => t, None => return Vec::new() };
    let bytes = source.as_bytes();

    match ext {
        "ts" | "tsx" | "js" | "jsx" => extract_ts(tree.root_node(), bytes),
        "rs" => extract_rust(tree.root_node(), bytes),
        "py" => extract_python(tree.root_node(), bytes),
        "go" => extract_go(tree.root_node(), bytes),
        "java" => extract_java(tree.root_node(), bytes),
        "c" | "h" => extract_c(tree.root_node(), bytes),
        _ => Vec::new(),
    }
}

// ──────────────────────────────────────────────────
// TypeScript / JavaScript
// ──────────────────────────────────────────────────

fn extract_ts(root: Node, src: &[u8]) -> Vec<AtlasSymbol> {
    let mut out = Vec::new();
    walk_ts(root, src, &mut out, None, false);
    out
}

fn walk_ts(node: Node, src: &[u8], out: &mut Vec<AtlasSymbol>, parent: Option<&str>, in_export: bool) {
    let kind = node.kind();
    let exported = in_export || kind == "export_statement";

    match kind {
        "export_statement" => {
            let mut c = node.walk();
            for child in node.children(&mut c) { walk_ts(child, src, out, parent, true); }
            return;
        }

        "function_declaration" | "generator_function_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "function", node, src, exported, parent));
            }
        }

        "class_declaration" => {
            let name = node.child_by_field_name("name").map(|n| ntxt(n, src));
            if let Some(ref n) = name {
                out.push(sym(n.clone(), "class", node, src, exported, parent));
            }
            if let Some(body) = node.child_by_field_name("body") {
                let mut c = body.walk();
                for child in body.children(&mut c) { walk_ts(child, src, out, name.as_deref(), false); }
            }
            return;
        }

        "interface_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "interface", node, src, exported, parent));
            }
        }

        "type_alias_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "type", node, src, exported, parent));
            }
        }

        "enum_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "enum", node, src, exported, parent));
            }
        }

        "method_definition" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "method", node, src, true, parent));
            }
        }

        "abstract_method_signature" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "method", node, src, true, parent));
            }
        }

        "lexical_declaration" | "variable_declaration" => {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() == "variable_declarator" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = ntxt(name_node, src);
                        let value = child.child_by_field_name("value");
                        let is_upper = name.chars().next().map_or(false, |c| c.is_uppercase());
                        let is_arrow = value.map_or(false, |v| v.kind() == "arrow_function" || v.kind() == "function");
                        let is_call = value.map_or(false, |v| v.kind() == "call_expression");

                        let k = if is_upper && (is_arrow || is_call) { "component" }
                            else if is_arrow { "function" }
                            else { "const" };

                        out.push(sym(name, k, node, src, exported, parent));
                    }
                }
            }
        }

        _ => {}
    }

    if kind != "class_declaration" {
        let mut c = node.walk();
        for child in node.children(&mut c) { walk_ts(child, src, out, parent, false); }
    }
}

// ──────────────────────────────────────────────────
// Rust
// ──────────────────────────────────────────────────

fn extract_rust(root: Node, src: &[u8]) -> Vec<AtlasSymbol> {
    let mut out = Vec::new();
    walk_rust(root, src, &mut out, None);
    out
}

fn walk_rust(node: Node, src: &[u8], out: &mut Vec<AtlasSymbol>, parent: Option<&str>) {
    let kind = node.kind();
    let vis = has_vis(node);

    match kind {
        "function_item" => {
            if let Some(n) = node.child_by_field_name("name") {
                let k = if parent.is_some() { "method" } else { "function" };
                out.push(sym(ntxt(n, src), k, node, src, vis, parent));
            }
        }
        "struct_item" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "struct", node, src, vis, parent));
            }
        }
        "enum_item" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "enum", node, src, vis, parent));
            }
        }
        "trait_item" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "trait", node, src, vis, parent));
            }
        }
        "type_item" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "type", node, src, vis, parent));
            }
        }
        "const_item" | "static_item" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "const", node, src, vis, parent));
            }
        }
        "mod_item" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "module", node, src, vis, parent));
            }
        }
        "macro_definition" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "macro", node, src, vis, parent));
            }
        }
        "impl_item" => {
            let type_name = node.child_by_field_name("type").map(|n| ntxt(n, src));
            let trait_name = node.child_by_field_name("trait").map(|n| ntxt(n, src));
            let display = match (&type_name, &trait_name) {
                (Some(ty), Some(tr)) => format!("{} for {}", tr, ty),
                (Some(ty), None) => ty.clone(),
                _ => "unknown".into(),
            };
            out.push(sym(display.clone(), "impl", node, src, vis, parent));

            if let Some(body) = node.child_by_field_name("body") {
                let mut c = body.walk();
                for child in body.children(&mut c) { walk_rust(child, src, out, Some(&display)); }
            }
            return;
        }
        _ => {}
    }

    if kind != "impl_item" {
        let mut c = node.walk();
        for child in node.children(&mut c) { walk_rust(child, src, out, parent); }
    }
}

fn has_vis(node: Node) -> bool {
    let mut c = node.walk();
    for child in node.children(&mut c) {
        if child.kind() == "visibility_modifier" { return true; }
        if child.kind() == "identifier" || child.kind() == "field_declaration_list" { break; }
    }
    false
}

// ──────────────────────────────────────────────────
// Python
// ──────────────────────────────────────────────────

fn extract_python(root: Node, src: &[u8]) -> Vec<AtlasSymbol> {
    let mut out = Vec::new();
    walk_py(root, src, &mut out, None, 0);
    out
}

fn walk_py(node: Node, src: &[u8], out: &mut Vec<AtlasSymbol>, parent: Option<&str>, depth: u32) {
    if depth > 10 { return; }
    let kind = node.kind();

    match kind {
        "function_definition" => {
            if let Some(n) = node.child_by_field_name("name") {
                let name = ntxt(n, src);
                let is_private = name.starts_with('_') && !name.starts_with("__");
                let k = if parent.is_some() { "method" } else { "function" };
                out.push(sym(name, k, node, src, !is_private, parent));
            }
            return; // don't recurse into function bodies
        }
        "class_definition" => {
            let name = node.child_by_field_name("name").map(|n| ntxt(n, src));
            if let Some(ref n) = name {
                out.push(sym(n.clone(), "class", node, src, !n.starts_with('_'), parent));
            }
            if let Some(body) = node.child_by_field_name("body") {
                let mut c = body.walk();
                for child in body.children(&mut c) { walk_py(child, src, out, name.as_deref(), depth + 1); }
            }
            return;
        }
        "decorated_definition" => {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() == "function_definition" || child.kind() == "class_definition" {
                    walk_py(child, src, out, parent, depth + 1);
                }
            }
            return;
        }
        _ => {}
    }

    let mut c = node.walk();
    for child in node.children(&mut c) { walk_py(child, src, out, parent, depth + 1); }
}

// ──────────────────────────────────────────────────
// Go
// ──────────────────────────────────────────────────

fn extract_go(root: Node, src: &[u8]) -> Vec<AtlasSymbol> {
    let mut out = Vec::new();
    walk_go(root, src, &mut out);
    out
}

fn walk_go(node: Node, src: &[u8], out: &mut Vec<AtlasSymbol>) {
    let kind = node.kind();

    match kind {
        "function_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                let name = ntxt(n, src);
                let exported = name.chars().next().map_or(false, |c| c.is_uppercase());
                out.push(sym(name, "function", node, src, exported, None));
            }
        }
        "method_declaration" => {
            let name = node.child_by_field_name("name").map(|n| ntxt(n, src));
            let receiver = node.child_by_field_name("receiver")
                .and_then(|r| {
                    // Receiver is a parameter_list like (r *Router)
                    // Extract the type name
                    let txt = ntxt(r, src);
                    txt.split_whitespace().last().map(|s| s.trim_start_matches('*').to_string())
                });

            if let Some(n) = name {
                let exported = n.chars().next().map_or(false, |c| c.is_uppercase());
                out.push(sym(n, "method", node, src, exported, receiver.as_deref()));
            }
        }
        "type_declaration" => {
            // type X struct/interface/...
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() == "type_spec" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = ntxt(name_node, src);
                        let exported = name.chars().next().map_or(false, |c| c.is_uppercase());
                        let type_node = child.child_by_field_name("type");
                        let k = match type_node.map(|t| t.kind()) {
                            Some("struct_type") => "struct",
                            Some("interface_type") => "interface",
                            _ => "type",
                        };
                        out.push(sym(name, k, child, src, exported, None));
                    }
                }
            }
        }
        _ => {}
    }

    let mut c = node.walk();
    for child in node.children(&mut c) { walk_go(child, src, out); }
}

// ──────────────────────────────────────────────────
// Java
// ──────────────────────────────────────────────────

fn extract_java(root: Node, src: &[u8]) -> Vec<AtlasSymbol> {
    let mut out = Vec::new();
    walk_java(root, src, &mut out, None);
    out
}

fn walk_java(node: Node, src: &[u8], out: &mut Vec<AtlasSymbol>, parent: Option<&str>) {
    let kind = node.kind();

    match kind {
        "class_declaration" | "record_declaration" => {
            let name = node.child_by_field_name("name").map(|n| ntxt(n, src));
            let vis = java_visibility(node, src);
            if let Some(ref n) = name {
                let k = if kind == "record_declaration" { "struct" } else { "class" };
                out.push(sym(n.clone(), k, node, src, vis, parent));
            }
            if let Some(body) = node.child_by_field_name("body") {
                let mut c = body.walk();
                for child in body.children(&mut c) { walk_java(child, src, out, name.as_deref()); }
            }
            return;
        }
        "interface_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                let vis = java_visibility(node, src);
                out.push(sym(ntxt(n, src), "interface", node, src, vis, parent));
            }
            if let Some(body) = node.child_by_field_name("body") {
                let mut c = body.walk();
                for child in body.children(&mut c) { walk_java(child, src, out, Some(&ntxt(node.child_by_field_name("name").unwrap(), src))); }
            }
            return;
        }
        "enum_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                let vis = java_visibility(node, src);
                out.push(sym(ntxt(n, src), "enum", node, src, vis, parent));
            }
        }
        "method_declaration" | "constructor_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                let vis = java_visibility(node, src);
                let k = if parent.is_some() { "method" } else { "function" };
                out.push(sym(ntxt(n, src), k, node, src, vis, parent));
            }
        }
        "annotation_type_declaration" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "interface", node, src, true, parent));
            }
        }
        _ => {}
    }

    if kind != "class_declaration" && kind != "interface_declaration" && kind != "record_declaration" {
        let mut c = node.walk();
        for child in node.children(&mut c) { walk_java(child, src, out, parent); }
    }
}

fn java_visibility(node: Node, src: &[u8]) -> bool {
    let mut c = node.walk();
    for child in node.children(&mut c) {
        if child.kind() == "modifiers" {
            let txt = ntxt(child, src);
            return txt.contains("public") || txt.contains("protected");
        }
    }
    true // default package-private is somewhat visible
}

// ──────────────────────────────────────────────────
// C / C++
// ──────────────────────────────────────────────────

fn extract_c(root: Node, src: &[u8]) -> Vec<AtlasSymbol> {
    let mut out = Vec::new();
    walk_c(root, src, &mut out);
    out
}

fn walk_c(node: Node, src: &[u8], out: &mut Vec<AtlasSymbol>) {
    let kind = node.kind();

    match kind {
        "function_definition" => {
            if let Some(declarator) = node.child_by_field_name("declarator") {
                if let Some(name) = find_identifier(declarator, src) {
                    out.push(sym(name, "function", node, src, true, None));
                }
            }
        }
        "declaration" => {
            // struct/enum/union declarations or function prototypes
            let mut c = node.walk();
            for child in node.children(&mut c) {
                match child.kind() {
                    "struct_specifier" | "union_specifier" => {
                        if let Some(n) = child.child_by_field_name("name") {
                            out.push(sym(ntxt(n, src), "struct", child, src, true, None));
                        }
                    }
                    "enum_specifier" => {
                        if let Some(n) = child.child_by_field_name("name") {
                            out.push(sym(ntxt(n, src), "enum", child, src, true, None));
                        }
                    }
                    _ => {}
                }
            }
        }
        "type_definition" => {
            // typedef struct { ... } Name;
            if let Some(declarator) = node.child_by_field_name("declarator") {
                if let Some(name) = find_identifier(declarator, src) {
                    out.push(sym(name, "type", node, src, true, None));
                }
            }
        }
        "preproc_function_def" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "macro", node, src, true, None));
            }
        }
        "preproc_def" => {
            if let Some(n) = node.child_by_field_name("name") {
                out.push(sym(ntxt(n, src), "const", node, src, true, None));
            }
        }
        _ => {}
    }

    let mut c = node.walk();
    for child in node.children(&mut c) { walk_c(child, src, out); }
}

/// Recursively find an identifier node inside a declarator (handles pointer_declarator, etc.)
fn find_identifier(node: Node, src: &[u8]) -> Option<String> {
    if node.kind() == "identifier" {
        return Some(ntxt(node, src));
    }
    // function_declarator has a declarator child with the name
    if let Some(d) = node.child_by_field_name("declarator") {
        return find_identifier(d, src);
    }
    let mut c = node.walk();
    for child in node.children(&mut c) {
        if let Some(name) = find_identifier(child, src) {
            return Some(name);
        }
    }
    None
}

// ──────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────

fn ntxt(node: Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or("").to_string()
}

fn sig_line(node: Node, src: &[u8]) -> String {
    let text = node.utf8_text(src).unwrap_or("");
    let first = text.lines().next().unwrap_or("").trim();
    if first.len() <= 200 { first.to_string() } else { format!("{}...", &first[..197]) }
}

fn sym(name: String, kind: &str, node: Node, src: &[u8], exported: bool, parent: Option<&str>) -> AtlasSymbol {
    AtlasSymbol {
        name,
        kind: kind.into(),
        line: node.start_position().row as u32 + 1,
        signature: sig_line(node, src),
        exported,
        parent: parent.map(String::from),
    }
}
