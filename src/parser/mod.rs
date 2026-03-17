use anyhow::{Context, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tree_sitter::{Language, Parser};

#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: String,         // "src/auth.ts::login"
    pub file: String,       // "src/auth.ts"
    pub name: String,       // "login"
    pub kind: String,       // "function" | "class" | "method" | "struct" | "impl"
    pub start_line: u32,
    pub end_line: u32,
    pub hash: String,       // hash of the symbol's source text
}

pub struct SymbolIndex {
    repo_root: PathBuf,
}

struct LangConfig {
    language: Language,
    extensions: &'static [&'static str],
    /// (node_kind, name_field) pairs to extract
    symbol_queries: Vec<(&'static str, NameExtractor)>,
}

enum NameExtractor {
    Field(&'static str),
    ChildKind(&'static str),
}

impl SymbolIndex {
    pub fn new(repo_root: &str) -> Result<Self> {
        let repo_root = std::fs::canonicalize(repo_root)
            .with_context(|| format!("Cannot access repo: {}", repo_root))?;
        Ok(Self { repo_root })
    }

    pub fn scan_all(&self) -> Result<Vec<Symbol>> {
        let configs = Self::lang_configs();
        let mut all_symbols = Vec::new();

        for config in &configs {
            for ext in config.extensions {
                let pattern = format!("{}/**/*.{}", self.repo_root.display(), ext);
                for entry in glob::glob(&pattern)? {
                    let path = entry?;
                    // Skip hidden dirs, node_modules, target, .git
                    let rel = path.strip_prefix(&self.repo_root).unwrap_or(&path);
                    let rel_str = rel.to_string_lossy();
                    if rel_str.contains("/node_modules/")
                        || rel_str.contains("/target/")
                        || rel_str.contains("/.")
                        || rel_str.contains("/.git/")
                    {
                        continue;
                    }

                    match self.parse_file(&path, config) {
                        Ok(symbols) => all_symbols.extend(symbols),
                        Err(e) => {
                            eprintln!("  warn: skipping {}: {}", rel_str, e);
                        }
                    }
                }
            }
        }

        Ok(all_symbols)
    }

    fn parse_file(&self, path: &Path, config: &LangConfig) -> Result<Vec<Symbol>> {
        let source = std::fs::read_to_string(path)?;
        let rel_path = path
            .strip_prefix(&self.repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let mut parser = Parser::new();
        parser.set_language(&config.language)?;

        let tree = parser
            .parse(&source, None)
            .context("Failed to parse file")?;

        let mut symbols = Vec::new();
        self.walk_tree(&tree.root_node(), &source, &rel_path, config, &mut symbols);
        Ok(symbols)
    }

    fn walk_tree(
        &self,
        node: &tree_sitter::Node,
        source: &str,
        file: &str,
        config: &LangConfig,
        out: &mut Vec<Symbol>,
    ) {
        let kind = node.kind();

        for (target_kind, extractor) in &config.symbol_queries {
            if kind == *target_kind {
                if let Some(name) = self.extract_name(node, extractor, source) {
                    let start = node.start_position().row as u32 + 1;
                    let end = node.end_position().row as u32 + 1;
                    let text = &source[node.byte_range()];
                    let hash = Self::hash_str(text);
                    let symbol_kind = Self::normalize_kind(kind);

                    out.push(Symbol {
                        id: format!("{}::{}", file, name),
                        file: file.to_string(),
                        name,
                        kind: symbol_kind.to_string(),
                        start_line: start,
                        end_line: end,
                        hash,
                    });
                }
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_tree(&child, source, file, config, out);
        }
    }

    fn extract_name(&self, node: &tree_sitter::Node, extractor: &NameExtractor, source: &str) -> Option<String> {
        match extractor {
            NameExtractor::Field(field) => {
                node.child_by_field_name(field)
                    .map(|n| source[n.byte_range()].to_string())
            }
            NameExtractor::ChildKind(kind) => {
                let mut cursor = node.walk();
                let result = node.children(&mut cursor)
                    .find(|c| c.kind() == *kind)
                    .map(|n| source[n.byte_range()].to_string());
                result
            }
        }
    }

    fn normalize_kind(tree_sitter_kind: &str) -> &str {
        match tree_sitter_kind {
            "function_declaration" | "function_definition" | "function_item" => "function",
            "method_definition" | "method_declaration" => "method",
            "class_declaration" | "class_definition" => "class",
            "struct_item" => "struct",
            "impl_item" => "impl",
            "enum_item" | "enum_declaration" => "enum",
            "interface_declaration" => "interface",
            "trait_item" => "trait",
            "type_alias_declaration" | "type_item" => "type",
            "arrow_function" => "arrow_fn",
            "export_statement" => "export",
            other => other,
        }
    }

    fn hash_str(s: &str) -> String {
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    fn lang_configs() -> Vec<LangConfig> {
        vec![
            // TypeScript / TSX
            LangConfig {
                language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                extensions: &["ts", "tsx"],
                symbol_queries: vec![
                    ("function_declaration", NameExtractor::Field("name")),
                    ("class_declaration", NameExtractor::Field("name")),
                    ("method_definition", NameExtractor::Field("name")),
                    ("interface_declaration", NameExtractor::Field("name")),
                    ("type_alias_declaration", NameExtractor::Field("name")),
                    ("enum_declaration", NameExtractor::Field("name")),
                ],
            },
            // JavaScript
            LangConfig {
                language: tree_sitter_javascript::LANGUAGE.into(),
                extensions: &["js", "jsx"],
                symbol_queries: vec![
                    ("function_declaration", NameExtractor::Field("name")),
                    ("class_declaration", NameExtractor::Field("name")),
                    ("method_definition", NameExtractor::Field("name")),
                ],
            },
            // Rust
            LangConfig {
                language: tree_sitter_rust::LANGUAGE.into(),
                extensions: &["rs"],
                symbol_queries: vec![
                    ("function_item", NameExtractor::Field("name")),
                    ("struct_item", NameExtractor::Field("name")),
                    ("enum_item", NameExtractor::Field("name")),
                    ("trait_item", NameExtractor::Field("name")),
                    ("impl_item", NameExtractor::Field("type")),
                    ("type_item", NameExtractor::Field("name")),
                ],
            },
            // Python
            LangConfig {
                language: tree_sitter_python::LANGUAGE.into(),
                extensions: &["py"],
                symbol_queries: vec![
                    ("function_definition", NameExtractor::Field("name")),
                    ("class_definition", NameExtractor::Field("name")),
                ],
            },
        ]
    }
}
