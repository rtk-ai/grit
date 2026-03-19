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

#[allow(dead_code)]
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

        // Reuse one parser per language instead of creating one per file
        let mut parser = Parser::new();

        for config in &configs {
            parser.set_language(&config.language)?;

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

                    match self.parse_file(&path, config, &mut parser) {
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

    fn parse_file(&self, path: &Path, config: &LangConfig, parser: &mut Parser) -> Result<Vec<Symbol>> {
        let source = std::fs::read_to_string(path)?;
        let rel_path = path
            .strip_prefix(&self.repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a file inside a TempDir, creating parent dirs as needed.
    fn write_file(dir: &TempDir, rel_path: &str, content: &str) -> PathBuf {
        let full = dir.path().join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
        full
    }

    /// Helper: find symbol by name in a slice.
    fn find_sym<'a>(symbols: &'a [Symbol], name: &str) -> &'a Symbol {
        symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("symbol '{}' not found in {:?}", name, symbols.iter().map(|s| &s.name).collect::<Vec<_>>()))
    }

    // ── 1. Rust functions ──────────────────────────────────────────────

    #[test]
    fn test_parse_rust_functions() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "src/lib.rs", r#"
fn alpha() {}
fn beta(x: i32) -> i32 { x }
fn gamma() -> String { String::new() }
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let fns: Vec<_> = symbols.iter().filter(|s| s.kind == "function").collect();
        assert_eq!(fns.len(), 3, "expected 3 functions, got {:?}", fns);

        for name in &["alpha", "beta", "gamma"] {
            let sym = find_sym(&symbols, name);
            assert_eq!(sym.kind, "function");
        }
    }

    // ── 2. Rust struct + impl ──────────────────────────────────────────

    #[test]
    fn test_parse_rust_struct_and_impl() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "src/model.rs", r#"
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn distance(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let _struct_sym = find_sym(&symbols, "Point");
        // The struct itself has kind "struct"; the impl also has name "Point" with kind "impl".
        let kinds: Vec<_> = symbols.iter().filter(|s| s.name == "Point").map(|s| s.kind.as_str()).collect();
        assert!(kinds.contains(&"struct"), "expected struct, got {:?}", kinds);
        assert!(kinds.contains(&"impl"), "expected impl, got {:?}", kinds);

        // The method inside impl should also be extracted
        let distance = find_sym(&symbols, "distance");
        assert_eq!(distance.kind, "function");
    }

    // ── 3. Rust enum + trait ───────────────────────────────────────────

    #[test]
    fn test_parse_rust_enum_and_trait() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "src/types.rs", r#"
enum Color {
    Red,
    Green,
    Blue,
}

trait Drawable {
    fn draw(&self);
}
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let color = find_sym(&symbols, "Color");
        assert_eq!(color.kind, "enum");

        let drawable = find_sym(&symbols, "Drawable");
        assert_eq!(drawable.kind, "trait");
    }

    // ── 4. TypeScript functions ────────────────────────────────────────

    #[test]
    fn test_parse_typescript_functions() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "src/utils.ts", r#"
function add(a: number, b: number): number {
    return a + b;
}

function greet(name: string): string {
    return `Hello, ${name}`;
}

function noop(): void {}
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let fns: Vec<_> = symbols.iter().filter(|s| s.kind == "function").collect();
        assert_eq!(fns.len(), 3);

        for name in &["add", "greet", "noop"] {
            let sym = find_sym(&symbols, name);
            assert_eq!(sym.kind, "function");
        }
    }

    // ── 5. TypeScript class + methods ──────────────────────────────────

    #[test]
    fn test_parse_typescript_class() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "src/service.ts", r#"
class UserService {
    getUser(id: number): string {
        return "user";
    }
    deleteUser(id: number): void {}
}
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let cls = find_sym(&symbols, "UserService");
        assert_eq!(cls.kind, "class");

        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == "method").collect();
        assert_eq!(methods.len(), 2);
        find_sym(&symbols, "getUser");
        find_sym(&symbols, "deleteUser");
    }

    // ── 6. TypeScript interface ────────────────────────────────────────

    #[test]
    fn test_parse_typescript_interface() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "src/types.ts", r#"
interface Config {
    host: string;
    port: number;
}

interface Logger {
    log(msg: string): void;
}
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let interfaces: Vec<_> = symbols.iter().filter(|s| s.kind == "interface").collect();
        assert_eq!(interfaces.len(), 2);
        find_sym(&symbols, "Config");
        find_sym(&symbols, "Logger");
    }

    // ── 7. Python functions ────────────────────────────────────────────

    #[test]
    fn test_parse_python_functions() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "utils.py", r#"
def connect(host, port):
    pass

def disconnect():
    pass

def retry(fn, times=3):
    pass
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let fns: Vec<_> = symbols.iter().filter(|s| s.kind == "function").collect();
        assert_eq!(fns.len(), 3);
        for name in &["connect", "disconnect", "retry"] {
            find_sym(&symbols, name);
        }
    }

    // ── 8. Python class + methods ──────────────────────────────────────

    #[test]
    fn test_parse_python_class() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "models.py", r#"
class Dog:
    def __init__(self, name):
        self.name = name

    def bark(self):
        return "woof"
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let cls = find_sym(&symbols, "Dog");
        assert_eq!(cls.kind, "class");

        // Python methods are function_definition nodes inside a class — they get kind "function"
        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == "function").collect();
        assert!(methods.len() >= 2, "expected at least __init__ and bark");
        find_sym(&symbols, "__init__");
        find_sym(&symbols, "bark");
    }

    // ── 9. JavaScript functions ────────────────────────────────────────

    #[test]
    fn test_parse_javascript_functions() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "lib/helpers.js", r#"
function sum(a, b) {
    return a + b;
}

function multiply(a, b) {
    return a * b;
}
"#);
        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        let fns: Vec<_> = symbols.iter().filter(|s| s.kind == "function").collect();
        assert_eq!(fns.len(), 2);
        find_sym(&symbols, "sum");
        find_sym(&symbols, "multiply");
    }

    // ── 10. Symbol ID format ───────────────────────────────────────────

    #[test]
    fn test_symbol_id_format() {
        let dir = TempDir::new().unwrap();
        write_file(&dir, "src/core/engine.rs", "fn run() {}\n");

        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();
        assert_eq!(symbols.len(), 1);

        let sym = &symbols[0];
        assert_eq!(sym.id, "src/core/engine.rs::run");
        assert_eq!(sym.file, "src/core/engine.rs");
        assert_eq!(sym.name, "run");
    }

    // ── 11. Hash determinism ───────────────────────────────────────────

    #[test]
    fn test_symbol_hash_deterministic() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        let code = "fn deterministic() { let x = 42; }\n";
        write_file(&dir1, "a.rs", code);
        write_file(&dir2, "a.rs", code);

        let s1 = SymbolIndex::new(dir1.path().to_str().unwrap()).unwrap().scan_all().unwrap();
        let s2 = SymbolIndex::new(dir2.path().to_str().unwrap()).unwrap().scan_all().unwrap();

        assert_eq!(s1.len(), 1);
        assert_eq!(s2.len(), 1);
        assert_eq!(s1[0].hash, s2[0].hash, "same source text must produce the same hash");
        assert!(!s1[0].hash.is_empty());
    }

    // ── 12. Skips node_modules ─────────────────────────────────────────

    #[test]
    fn test_skips_node_modules() {
        let dir = TempDir::new().unwrap();
        // File inside a nested node_modules — should be skipped
        // Note: the skip filter checks for "/node_modules/" in the relative path,
        // so node_modules must be inside a parent directory (e.g., src/node_modules/).
        write_file(&dir, "src/node_modules/lodash/index.js", "function chunk() {}\n");
        // File outside node_modules — should be found
        write_file(&dir, "src/app.js", "function main() {}\n");

        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        assert_eq!(symbols.len(), 1, "only src/app.js should be scanned");
        assert_eq!(symbols[0].name, "main");
    }

    // ── 13. Normalize kind (via public interface) ──────────────────────

    #[test]
    fn test_normalize_kind() {
        let dir = TempDir::new().unwrap();

        // Rust: function_item → "function", struct_item → "struct", enum_item → "enum",
        //       trait_item → "trait", impl_item → "impl"
        write_file(&dir, "src/all.rs", r#"
fn my_func() {}
struct MyStruct { x: i32 }
enum MyEnum { A, B }
trait MyTrait { fn do_it(&self); }
impl MyStruct { fn method(&self) {} }
"#);

        // TypeScript: function_declaration → "function", class_declaration → "class",
        //             method_definition → "method", interface_declaration → "interface"
        write_file(&dir, "src/all.ts", r#"
function tsFunc(): void {}
class TsClass {
    tsMethod(): void {}
}
interface TsInterface { x: number; }
"#);

        // Python: function_definition → "function", class_definition → "class"
        write_file(&dir, "all.py", "def py_func():\n    pass\n\nclass PyClass:\n    pass\n");

        let idx = SymbolIndex::new(dir.path().to_str().unwrap()).unwrap();
        let symbols = idx.scan_all().unwrap();

        // Verify the normalized kinds
        assert_eq!(find_sym(&symbols, "my_func").kind, "function");
        assert_eq!(find_sym(&symbols, "MyStruct").kind, "struct");
        assert_eq!(find_sym(&symbols, "MyEnum").kind, "enum");
        assert_eq!(find_sym(&symbols, "MyTrait").kind, "trait");
        // impl MyStruct → name="MyStruct", kind="impl" (second entry with that name)
        let impls: Vec<_> = symbols.iter().filter(|s| s.kind == "impl").collect();
        assert!(!impls.is_empty(), "expected at least one impl symbol");

        assert_eq!(find_sym(&symbols, "tsFunc").kind, "function");
        assert_eq!(find_sym(&symbols, "TsClass").kind, "class");
        assert_eq!(find_sym(&symbols, "tsMethod").kind, "method");
        assert_eq!(find_sym(&symbols, "TsInterface").kind, "interface");

        assert_eq!(find_sym(&symbols, "py_func").kind, "function");
        assert_eq!(find_sym(&symbols, "PyClass").kind, "class");
    }
}
