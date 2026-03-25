//! Go-to-symbol outline: tree-sitter extraction of top-level definitions.

use tree_sitter::{Node, Parser, Point};

/// Kind of outline entry (affects list prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Module,
    Class,
    Interface,
    Impl,
    TypeAlias,
    Variable,
}

impl SymbolKind {
    /// Short label for the outline list (e.g. `fn`, `struct`).
    pub fn prefix(self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Module => "mod",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Impl => "impl",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Variable => "var",
        }
    }
}

/// One navigable symbol in the current buffer.
#[derive(Debug, Clone)]
pub struct OutlineSymbol {
    pub kind: SymbolKind,
    pub name: String,
    /// Zero-based line index (start of the definition).
    pub start_line: usize,
    /// Zero-based character column on `start_line` (name start).
    pub name_start_col: usize,
}

/// Extract top-level symbols for supported languages. Returns empty when unsupported or on parse failure.
pub fn extract_symbols(language_id: &str, source: &str) -> Vec<OutlineSymbol> {
    let Some(lang) = language_for(language_id) else {
        return Vec::new();
    };
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let mut out = match language_id {
        "rust" => collect_rust_top_level(&root, source),
        "python" => collect_python_top_level(&root, source),
        "javascript" => collect_js_top_level(&root, source),
        "typescript" => collect_ts_top_level(&root, source),
        "go" => collect_go_top_level(&root, source),
        _ => Vec::new(),
    };
    out.sort_by(|a, b| {
        (a.start_line, a.name_start_col)
            .cmp(&(b.start_line, b.name_start_col))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

fn language_for(id: &str) -> Option<tree_sitter::Language> {
    Some(match id {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        // TSX grammar accepts most TypeScript including `.ts` files.
        "typescript" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        _ => return None,
    })
}

fn node_name_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    let t = name.utf8_text(source).ok()?;
    Some(t.to_string())
}

fn node_type_text(node: Node<'_>, source: &[u8]) -> Option<String> {
    let t = node.child_by_field_name("type")?;
    let txt = t.utf8_text(source).ok()?;
    Some(txt.to_string())
}

fn point_to_line_col(source: &str, point: Point) -> (usize, usize) {
    let row = point.row as usize;
    let byte_col = point.column as usize;
    let line_start_byte = nth_line_start_byte(source, row);
    let name_byte = line_start_byte.saturating_add(byte_col).min(source.len());
    let line_bytes = line_slice_bytes(source, row);
    let col_in_line_bytes = name_byte.saturating_sub(line_start_byte).min(line_bytes.len());
    let char_col = std::str::from_utf8(&line_bytes[..col_in_line_bytes])
        .map(|s| s.chars().count())
        .unwrap_or(0);
    (row, char_col)
}

fn nth_line_start_byte(s: &str, line_idx: usize) -> usize {
    let mut line = 0usize;
    for (i, b) in s.as_bytes().iter().enumerate() {
        if line == line_idx {
            return i;
        }
        if *b == b'\n' {
            line += 1;
        }
    }
    if line == line_idx {
        s.len()
    } else {
        s.len()
    }
}

fn line_slice_bytes(s: &str, line_idx: usize) -> &[u8] {
    let start = nth_line_start_byte(s, line_idx);
    let rest = s[start..].as_bytes();
    let end_rel = rest
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(rest.len());
    &rest[..end_rel]
}

fn push_symbol(
    out: &mut Vec<OutlineSymbol>,
    kind: SymbolKind,
    name: String,
    name_node: Node<'_>,
    source: &str,
) {
    if name.is_empty() {
        return;
    }
    let (start_line, name_start_col) = point_to_line_col(source, name_node.start_position());
    out.push(OutlineSymbol {
        kind,
        name,
        start_line,
        name_start_col,
    });
}

fn collect_rust_top_level(root: &Node<'_>, source: &str) -> Vec<OutlineSymbol> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let n = root.named_child_count();
    while i < n {
        let Some(node) = root.named_child(i) else {
            i += 1;
            continue;
        };
        match node.kind() {
            "function_item" | "struct_item" | "enum_item" | "trait_item" | "mod_item"
            | "type_item" => {
                if let Some(name) = node_name_text(node, bytes) {
                    let kind = match node.kind() {
                        "function_item" => SymbolKind::Function,
                        "struct_item" => SymbolKind::Struct,
                        "enum_item" => SymbolKind::Enum,
                        "trait_item" => SymbolKind::Trait,
                        "mod_item" => SymbolKind::Module,
                        "type_item" => SymbolKind::TypeAlias,
                        _ => SymbolKind::Function,
                    };
                    if let Some(name_n) = node.child_by_field_name("name") {
                        push_symbol(&mut out, kind, name, name_n, source);
                    }
                }
            }
            "impl_item" => {
                if let Some(type_txt) = node_type_text(node, bytes) {
                    let label = format!("impl {type_txt}");
                    let start = node.start_position();
                    let (start_line, name_start_col) = point_to_line_col(source, start);
                    out.push(OutlineSymbol {
                        kind: SymbolKind::Impl,
                        name: label,
                        start_line,
                        name_start_col,
                    });
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn collect_python_top_level(root: &Node<'_>, source: &str) -> Vec<OutlineSymbol> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for i in 0..root.named_child_count() {
        let Some(node) = root.named_child(i) else {
            continue;
        };
        match node.kind() {
            "function_definition" | "class_definition" => {
                if let (Some(name), Some(name_n)) = (node_name_text(node, bytes), node.child_by_field_name("name"))
                {
                    let kind = if node.kind() == "class_definition" {
                        SymbolKind::Class
                    } else {
                        SymbolKind::Function
                    };
                    push_symbol(&mut out, kind, name, name_n, source);
                }
            }
            "decorated_definition" => {
                for j in 0..node.named_child_count() {
                    let Some(child) = node.named_child(j) else { continue };
                    match child.kind() {
                        "function_definition" | "class_definition" => {
                            if let (Some(name), Some(name_n)) =
                                (node_name_text(child, bytes), child.child_by_field_name("name"))
                            {
                                let kind = if child.kind() == "class_definition" {
                                    SymbolKind::Class
                                } else {
                                    SymbolKind::Function
                                };
                                push_symbol(&mut out, kind, name, name_n, source);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn collect_js_top_level(root: &Node<'_>, source: &str) -> Vec<OutlineSymbol> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for i in 0..root.named_child_count() {
        let Some(node) = root.named_child(i) else {
            continue;
        };
        match node.kind() {
            "function_declaration" => {
                if let (Some(name), Some(name_n)) = (node_name_text(node, bytes), node.child_by_field_name("name"))
                {
                    push_symbol(&mut out, SymbolKind::Function, name, name_n, source);
                }
            }
            "class_declaration" => {
                if let (Some(name), Some(name_n)) = (node_name_text(node, bytes), node.child_by_field_name("name"))
                {
                    push_symbol(&mut out, SymbolKind::Class, name, name_n, source);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                for j in 0..node.named_child_count() {
                    let Some(decl) = node.named_child(j) else { continue };
                    if decl.kind() != "variable_declarator" {
                        continue;
                    }
                    let Some(name_n) = decl.child_by_field_name("name") else {
                        continue;
                    };
                    let Some(name) = name_n.utf8_text(bytes).ok().map(str::to_string) else {
                        continue;
                    };
                    let val = decl.child_by_field_name("value");
                    let kind = if let Some(v) = val {
                        if matches!(v.kind(), "function" | "arrow_function") {
                            SymbolKind::Function
                        } else {
                            SymbolKind::Variable
                        }
                    } else {
                        SymbolKind::Variable
                    };
                    push_symbol(&mut out, kind, name, name_n, source);
                }
            }
            _ => {}
        }
    }
    out
}

fn collect_ts_top_level(root: &Node<'_>, source: &str) -> Vec<OutlineSymbol> {
    let mut out = collect_js_top_level(root, source);
    let bytes = source.as_bytes();
    for i in 0..root.named_child_count() {
        let Some(node) = root.named_child(i) else {
            continue;
        };
        match node.kind() {
            "interface_declaration" => {
                if let (Some(name), Some(name_n)) = (node_name_text(node, bytes), node.child_by_field_name("name"))
                {
                    push_symbol(&mut out, SymbolKind::Interface, name, name_n, source);
                }
            }
            "enum_declaration" => {
                if let (Some(name), Some(name_n)) = (node_name_text(node, bytes), node.child_by_field_name("name"))
                {
                    push_symbol(&mut out, SymbolKind::Enum, name, name_n, source);
                }
            }
            "type_alias_declaration" => {
                if let (Some(name), Some(name_n)) = (node_name_text(node, bytes), node.child_by_field_name("name"))
                {
                    push_symbol(&mut out, SymbolKind::TypeAlias, name, name_n, source);
                }
            }
            _ => {}
        }
    }
    out
}

fn collect_go_top_level(root: &Node<'_>, source: &str) -> Vec<OutlineSymbol> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for i in 0..root.named_child_count() {
        let Some(node) = root.named_child(i) else {
            continue;
        };
        match node.kind() {
            "function_declaration" => {
                if let (Some(name), Some(name_n)) = (node_name_text(node, bytes), node.child_by_field_name("name"))
                {
                    push_symbol(&mut out, SymbolKind::Function, name, name_n, source);
                }
            }
            "type_declaration" => {
                for j in 0..node.named_child_count() {
                    let Some(spec) = node.named_child(j) else { continue };
                    if spec.kind() == "type_spec" {
                        if let (Some(name), Some(name_n)) =
                            (node_name_text(spec, bytes), spec.child_by_field_name("name"))
                        {
                            push_symbol(&mut out, SymbolKind::TypeAlias, name, name_n, source);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_language_empty() {
        assert!(extract_symbols("text", "hello").is_empty());
    }

    #[test]
    fn rust_outline() {
        let src = r#"
fn alpha() {}
struct Beta { x: i32 }
mod gamma {}
"#;
        let sym = extract_symbols("rust", src);
        assert!(sym.iter().any(|s| s.name == "alpha" && s.kind == SymbolKind::Function));
        assert!(sym.iter().any(|s| s.name == "Beta" && s.kind == SymbolKind::Struct));
        assert!(sym.iter().any(|s| s.name == "gamma" && s.kind == SymbolKind::Module));
    }

    #[test]
    fn python_outline() {
        let src = r#"
def foo():
    pass
class Bar:
    pass
"#;
        let sym = extract_symbols("python", src);
        assert!(sym.iter().any(|s| s.name == "foo"));
        assert!(sym.iter().any(|s| s.name == "Bar" && s.kind == SymbolKind::Class));
    }

    #[test]
    fn go_outline() {
        let src = r#"
package main
func main() {}
type Thing struct { n int }
"#;
        let sym = extract_symbols("go", src);
        assert!(sym.iter().any(|s| s.name == "main"));
        assert!(sym.iter().any(|s| s.name == "Thing"));
    }

    #[test]
    fn typescript_interface() {
        let src = r"interface User { id: number }";
        let sym = extract_symbols("typescript", src);
        assert!(sym.iter().any(|s| s.name == "User" && s.kind == SymbolKind::Interface));
    }
}
