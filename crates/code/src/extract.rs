use std::path::Path;

use anyhow::{Context, Result};
use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator, Tree};

use super::grammar::{LanguageEntry, Registry};
use super::index::{Call, Definition, DefinitionKind, Import, Span};

pub struct QuerySet {
    pub definitions: Query,
    pub calls: Query,
    pub imports: Option<Query>,
    def_name_idx: u32,
    def_kind_indices: Vec<(u32, DefinitionKind)>,
    call_name_idx: u32,
    import_path_indices: Vec<u32>,
    import_alias_idx: Option<u32>,
}

impl QuerySet {
    pub fn load(language: Language, entry: &LanguageEntry) -> Result<Self> {
        let definitions = Query::new(&language, &entry.definition_query)
            .with_context(|| format!("failed to compile definition query for {}", entry.name))?;

        let calls = Query::new(&language, &entry.call_query)
            .with_context(|| format!("failed to compile call query for {}", entry.name))?;

        let imports = if entry.import_query.trim().is_empty() {
            None
        } else {
            Some(
                Query::new(&language, &entry.import_query)
                    .with_context(|| format!("failed to compile import query for {}", entry.name))?,
            )
        };

        let def_name_idx = definitions
            .capture_index_for_name("name")
            .unwrap_or(u32::MAX);

        let def_kind_indices = Self::build_definition_kind_indices(&definitions);

        let call_name_idx = calls.capture_index_for_name("name").unwrap_or(u32::MAX);

        let (import_path_indices, import_alias_idx) = if let Some(ref q) = imports {
            let path_indices = ["path", "source", "system_path", "local_path", "module"]
                .iter()
                .filter_map(|name| q.capture_index_for_name(name))
                .collect();
            let alias = q.capture_index_for_name("alias");
            (path_indices, alias)
        } else {
            (vec![], None)
        };

        Ok(Self {
            definitions,
            calls,
            imports,
            def_name_idx,
            def_kind_indices,
            call_name_idx,
            import_path_indices,
            import_alias_idx,
        })
    }

    fn build_definition_kind_indices(query: &Query) -> Vec<(u32, DefinitionKind)> {
        let mut indices = Vec::new();

        let kind_mappings = [
            ("function.definition", DefinitionKind::Function),
            ("method.definition", DefinitionKind::Method),
            ("class.definition", DefinitionKind::Class),
            ("struct.definition", DefinitionKind::Struct),
            ("enum.definition", DefinitionKind::Enum),
            ("trait.definition", DefinitionKind::Trait),
            ("interface.definition", DefinitionKind::Interface),
            ("module.definition", DefinitionKind::Module),
            ("object.definition", DefinitionKind::Other("object".into())),
        ];

        for (name, kind) in kind_mappings {
            if let Some(idx) = query.capture_index_for_name(name) {
                indices.push((idx, kind));
            }
        }

        indices
    }
}

pub struct Extractor {
    language: Language,
    queries: QuerySet,
}

impl Extractor {
    pub fn new(lang_name: &str) -> Result<Self> {
        let registry = Registry::global();
        let entry = registry
            .get(lang_name)
            .with_context(|| format!("unknown language: {}", lang_name))?;

        let language = super::grammar::load_language(lang_name)?;
        let queries = QuerySet::load(language.clone(), entry)?;

        Ok(Self { language, queries })
    }

    pub fn from_extension(ext: &str) -> Result<Self> {
        let registry = Registry::global();
        let entry = registry
            .get_by_extension(ext)
            .with_context(|| format!("no language for extension: {}", ext))?;

        let language = super::grammar::load_language(&entry.name)?;
        let queries = QuerySet::load(language.clone(), entry)?;

        Ok(Self { language, queries })
    }

    pub fn language(&self) -> &Language {
        &self.language
    }

    pub fn extract_definitions(
        &self,
        tree: &Tree,
        source: &[u8],
        path: &Path,
    ) -> Vec<Definition> {
        let mut cursor = QueryCursor::new();
        let mut definitions = Vec::new();
        let mut matches = cursor.matches(&self.queries.definitions, tree.root_node(), source);

        while let Some(m) = matches.next() {
            let mut name: Option<&str> = None;
            let mut kind: Option<DefinitionKind> = None;
            let mut span_node: Option<Node> = None;

            for capture in m.captures {
                if capture.index == self.queries.def_name_idx {
                    name = capture.node.utf8_text(source).ok();
                }

                for (kind_idx, kind_type) in &self.queries.def_kind_indices {
                    if capture.index == *kind_idx {
                        kind = Some(kind_type.clone());
                        span_node = Some(capture.node);
                        break;
                    }
                }
            }

            if let (Some(name), Some(kind), Some(node)) = (name, kind, span_node) {
                definitions.push(Definition {
                    name: name.to_string(),
                    kind,
                    span: node_to_span(&node),
                    file: path.to_path_buf(),
                });
            }
        }

        definitions
    }

    pub fn extract_calls(&self, tree: &Tree, source: &[u8], path: &Path) -> Vec<Call> {
        let definitions = self.extract_definitions(tree, source, path);
        let mut cursor = QueryCursor::new();
        let mut calls = Vec::new();
        let mut matches = cursor.matches(&self.queries.calls, tree.root_node(), source);

        while let Some(m) = matches.next() {
            let mut callee: Option<&str> = None;
            let mut call_node: Option<Node> = None;

            for capture in m.captures {
                if capture.index == self.queries.call_name_idx {
                    callee = capture.node.utf8_text(source).ok();
                    call_node = Some(capture.node);
                }
            }

            if let (Some(callee), Some(node)) = (callee, call_node) {
                let caller = find_enclosing_definition(&definitions, node.start_byte());

                calls.push(Call {
                    callee: callee.to_string(),
                    span: node_to_span(&node),
                    file: path.to_path_buf(),
                    caller,
                });
            }
        }

        calls
    }

    pub fn extract_imports(&self, tree: &Tree, source: &[u8], path: &Path) -> Vec<Import> {
        let Some(ref import_query) = self.queries.imports else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let mut imports = Vec::new();
        let mut seen_ranges = std::collections::HashSet::new();
        let mut matches = cursor.matches(import_query, tree.root_node(), source);

        while let Some(m) = matches.next() {
            let mut module_path: Option<&str> = None;
            let mut alias: Option<&str> = None;
            let mut import_node: Option<Node> = None;

            for capture in m.captures {
                if self.queries.import_path_indices.contains(&capture.index)
                    && module_path.is_none()
                {
                    module_path = capture.node.utf8_text(source).ok();
                    import_node = Some(capture.node);
                }

                if self.queries.import_alias_idx == Some(capture.index) {
                    alias = capture.node.utf8_text(source).ok();
                }
            }

            if let (Some(module_path), Some(node)) = (module_path, import_node) {
                let range = (node.start_byte(), node.end_byte());
                if seen_ranges.contains(&range) {
                    continue;
                }
                seen_ranges.insert(range);

                let cleaned_path = clean_import_path(module_path);

                imports.push(Import {
                    module_path: cleaned_path,
                    alias: alias.map(|s| s.to_string()),
                    span: node_to_span(&node),
                    file: path.to_path_buf(),
                });
            }
        }

        imports
    }
}

fn node_to_span(node: &Node) -> Span {
    Span {
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
    }
}

fn find_enclosing_definition(definitions: &[Definition], byte_offset: usize) -> Option<String> {
    definitions
        .iter()
        .filter(|d| d.span.start_byte <= byte_offset && byte_offset < d.span.end_byte)
        .min_by_key(|d| d.span.end_byte - d.span.start_byte)
        .map(|d| d.name.clone())
}

fn clean_import_path(path: &str) -> String {
    path.trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_import_path() {
        assert_eq!(clean_import_path("\"std::fs\""), "std::fs");
        assert_eq!(clean_import_path("'./module'"), "./module");
        assert_eq!(clean_import_path("std::path"), "std::path");
    }

    #[test]
    fn test_span_fields() {
        let span = Span {
            start_byte: 10,
            end_byte: 50,
            start_line: 2,
            end_line: 5,
        };

        assert_eq!(span.start_byte, 10);
        assert_eq!(span.end_byte, 50);
        assert_eq!(span.start_line, 2);
        assert_eq!(span.end_line, 5);
    }
}
