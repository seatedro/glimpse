use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};

use super::index::{Definition, Index};
use super::lsp::LspResolver;
use super::resolve::Resolver;

pub type NodeId = usize;

#[derive(Debug, Clone)]
pub struct CallGraphNode {
    pub definition: Definition,
    pub callees: HashSet<NodeId>,
    pub callers: HashSet<NodeId>,
}

#[derive(Debug, Default)]
pub struct CallGraph {
    pub nodes: HashMap<NodeId, CallGraphNode>,
    name_to_id: HashMap<String, NodeId>,
    file_name_to_id: HashMap<(String, String), NodeId>,
    next_id: NodeId,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            name_to_id: HashMap::new(),
            file_name_to_id: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn build(index: &Index) -> Self {
        Self::build_with_options(index, false)
    }

    pub fn build_with_options(index: &Index, strict: bool) -> Self {
        let resolver = Resolver::with_strict(index, strict);
        let mut graph = CallGraph::new();

        for def in index.definitions() {
            graph.add_definition(def.clone());
        }

        for call in index.calls() {
            let caller_id = call
                .caller
                .as_ref()
                .and_then(|name| graph.find_node_by_file_and_name(&call.file, name));

            let Some(caller_id) = caller_id else {
                continue;
            };

            let callee_def = if let Some(ref resolved) = call.resolved {
                index
                    .get(&resolved.target_file)
                    .and_then(|r| {
                        r.definitions
                            .iter()
                            .find(|d| d.name == resolved.target_name)
                    })
                    .cloned()
            } else {
                resolver.resolve(&call.callee, call.qualifier.as_deref(), &call.file)
            };

            let callee_id = if let Some(def) = callee_def {
                graph
                    .find_node_by_file_and_name(&def.file, &def.name)
                    .unwrap_or_else(|| graph.add_definition(def))
            } else {
                continue;
            };

            graph.add_edge(caller_id, callee_id);
        }

        graph
    }

    pub fn build_with_lsp(index: &Index, root: &Path) -> Self {
        let heuristic_resolver = Resolver::with_strict(index, false);
        let mut graph = CallGraph::new();

        for def in index.definitions() {
            graph.add_definition(def.clone());
        }

        let calls: Vec<_> = index.calls().collect();
        let total = calls.len();

        if total == 0 {
            return graph;
        }

        let pb = ProgressBar::new(total as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .expect("valid template")
                .progress_chars("#>-"),
        );
        pb.set_message("initializing LSP...");

        let mut lsp_resolver = LspResolver::with_progress(root, pb.clone());

        pb.set_message("resolving calls");

        for call in &calls {
            pb.inc(1);

            let caller_id = call
                .caller
                .as_ref()
                .and_then(|name| graph.find_node_by_file_and_name(&call.file, name));

            let Some(caller_id) = caller_id else {
                continue;
            };

            let callee_def = lsp_resolver.resolve_call(call, index).or_else(|| {
                heuristic_resolver.resolve(&call.callee, call.qualifier.as_deref(), &call.file)
            });

            let callee_id = if let Some(def) = callee_def {
                graph
                    .find_node_by_file_and_name(&def.file, &def.name)
                    .unwrap_or_else(|| graph.add_definition(def))
            } else {
                continue;
            };

            graph.add_edge(caller_id, callee_id);
        }

        pb.finish_and_clear();
        graph
    }

    pub fn build_precise(index: &Index, root: &Path, strict: bool, precise: bool) -> Self {
        if precise {
            Self::build_with_lsp(index, root)
        } else {
            Self::build_with_options(index, strict)
        }
    }

    fn add_definition(&mut self, definition: Definition) -> NodeId {
        let file_key = definition.file.to_string_lossy().to_string();
        let composite_key = (file_key, definition.name.clone());

        if let Some(&existing_id) = self.file_name_to_id.get(&composite_key) {
            return existing_id;
        }

        let id = self.next_id;
        self.next_id += 1;

        let node = CallGraphNode {
            definition: definition.clone(),
            callees: HashSet::new(),
            callers: HashSet::new(),
        };

        self.nodes.insert(id, node);
        self.name_to_id.entry(definition.name.clone()).or_insert(id);
        self.file_name_to_id.insert(composite_key, id);

        id
    }

    fn add_edge(&mut self, caller: NodeId, callee: NodeId) {
        if caller == callee {
            return;
        }

        if let Some(caller_node) = self.nodes.get_mut(&caller) {
            caller_node.callees.insert(callee);
        }

        if let Some(callee_node) = self.nodes.get_mut(&callee) {
            callee_node.callers.insert(caller);
        }
    }

    pub fn find_node(&self, name: &str) -> Option<NodeId> {
        self.name_to_id.get(name).copied()
    }

    pub fn find_node_by_file_and_name(&self, file: &Path, name: &str) -> Option<NodeId> {
        let file_key = file.to_string_lossy().to_string();
        self.file_name_to_id
            .get(&(file_key, name.to_string()))
            .copied()
    }

    pub fn get_node(&self, id: NodeId) -> Option<&CallGraphNode> {
        self.nodes.get(&id)
    }

    pub fn get_callees(&self, node_id: NodeId) -> Vec<&CallGraphNode> {
        self.nodes
            .get(&node_id)
            .map(|node| {
                node.callees
                    .iter()
                    .filter_map(|id| self.nodes.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_callers(&self, node_id: NodeId) -> Vec<&CallGraphNode> {
        self.nodes
            .get(&node_id)
            .map(|node| {
                node.callers
                    .iter()
                    .filter_map(|id| self.nodes.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn get_transitive_callees(&self, node_id: NodeId) -> Vec<&CallGraphNode> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        if let Some(node) = self.nodes.get(&node_id) {
            for &callee_id in &node.callees {
                queue.push_back(callee_id);
            }
        }

        while let Some(current_id) = queue.pop_front() {
            if !visited.insert(current_id) {
                continue;
            }

            if let Some(node) = self.nodes.get(&current_id) {
                result.push(node);

                for &callee_id in &node.callees {
                    if !visited.contains(&callee_id) {
                        queue.push_back(callee_id);
                    }
                }
            }
        }

        result
    }

    pub fn get_transitive_callers(&self, node_id: NodeId) -> Vec<&CallGraphNode> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        if let Some(node) = self.nodes.get(&node_id) {
            for &caller_id in &node.callers {
                queue.push_back(caller_id);
            }
        }

        while let Some(current_id) = queue.pop_front() {
            if !visited.insert(current_id) {
                continue;
            }

            if let Some(node) = self.nodes.get(&current_id) {
                result.push(node);

                for &caller_id in &node.callers {
                    if !visited.contains(&caller_id) {
                        queue.push_back(caller_id);
                    }
                }
            }
        }

        result
    }

    pub fn post_order(&self, node_id: NodeId) -> Vec<NodeId> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.post_order_dfs(node_id, &mut visited, &mut result);
        result
    }

    fn post_order_dfs(
        &self,
        node_id: NodeId,
        visited: &mut HashSet<NodeId>,
        result: &mut Vec<NodeId>,
    ) {
        if !visited.insert(node_id) {
            return;
        }

        if let Some(node) = self.nodes.get(&node_id) {
            for &callee_id in &node.callees {
                self.post_order_dfs(callee_id, visited, result);
            }
        }

        result.push(node_id);
    }

    pub fn post_order_definitions(&self, node_id: NodeId) -> Vec<&Definition> {
        self.post_order(node_id)
            .into_iter()
            .filter_map(|id| self.nodes.get(&id).map(|n| &n.definition))
            .collect()
    }

    pub fn get_callees_to_depth(&self, node_id: NodeId, max_depth: usize) -> Vec<NodeId> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back((node_id, 0));
        visited.insert(node_id);

        while let Some((current_id, depth)) = queue.pop_front() {
            result.push(current_id);

            if depth >= max_depth {
                continue;
            }

            if let Some(node) = self.nodes.get(&current_id) {
                for &callee_id in &node.callees {
                    if visited.insert(callee_id) {
                        queue.push_back((callee_id, depth + 1));
                    }
                }
            }
        }

        result
    }

    pub fn get_callers_to_depth(&self, node_id: NodeId, max_depth: usize) -> Vec<NodeId> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back((node_id, 0));
        visited.insert(node_id);

        while let Some((current_id, depth)) = queue.pop_front() {
            result.push(current_id);

            if depth >= max_depth {
                continue;
            }

            if let Some(node) = self.nodes.get(&current_id) {
                for &caller_id in &node.callers {
                    if visited.insert(caller_id) {
                        queue.push_back((caller_id, depth + 1));
                    }
                }
            }
        }

        result
    }

    pub fn definitions_to_depth(&self, node_id: NodeId, max_depth: usize) -> Vec<&Definition> {
        self.get_callees_to_depth(node_id, max_depth)
            .into_iter()
            .filter_map(|id| self.nodes.get(&id).map(|n| &n.definition))
            .collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.nodes.values().map(|n| n.callees.len()).sum()
    }

    pub fn roots(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.callers.is_empty())
            .map(|(&id, _)| id)
            .collect()
    }

    pub fn leaves(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.callees.is_empty())
            .map(|(&id, _)| id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::index::{Call, DefinitionKind, FileRecord, Span};
    use super::*;
    use std::path::PathBuf;

    fn make_span() -> Span {
        Span {
            start_byte: 0,
            end_byte: 100,
            start_line: 1,
            end_line: 10,
        }
    }

    fn make_definition(name: &str, file: &str) -> Definition {
        Definition {
            name: name.to_string(),
            kind: DefinitionKind::Function,
            span: make_span(),
            file: PathBuf::from(file),
            signature: None,
        }
    }

    fn make_call(callee: &str, caller: Option<&str>, file: &str) -> Call {
        Call {
            callee: callee.to_string(),
            qualifier: None,
            span: make_span(),
            file: PathBuf::from(file),
            caller: caller.map(|s| s.to_string()),
            resolved: None,
        }
    }

    #[test]
    fn test_build_empty_index() {
        let index = Index::new();
        let graph = CallGraph::build(&index);
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_build_definitions_only() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/main.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("main", "src/main.rs"),
                make_definition("helper", "src/main.rs"),
            ],
            calls: vec![],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_build_with_calls() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/main.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("main", "src/main.rs"),
                make_definition("helper", "src/main.rs"),
            ],
            calls: vec![make_call("helper", Some("main"), "src/main.rs")],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);

        let main_id = graph.find_node("main").unwrap();
        let callees = graph.get_callees(main_id);
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].definition.name, "helper");
    }

    #[test]
    fn test_get_callees_and_callers() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/lib.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("a", "src/lib.rs"),
                make_definition("b", "src/lib.rs"),
                make_definition("c", "src/lib.rs"),
            ],
            calls: vec![
                make_call("b", Some("a"), "src/lib.rs"),
                make_call("c", Some("a"), "src/lib.rs"),
                make_call("c", Some("b"), "src/lib.rs"),
            ],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);

        let a_id = graph.find_node("a").unwrap();
        let c_id = graph.find_node("c").unwrap();

        let a_callees = graph.get_callees(a_id);
        assert_eq!(a_callees.len(), 2);

        let c_callers = graph.get_callers(c_id);
        assert_eq!(c_callers.len(), 2);

        let a_callers = graph.get_callers(a_id);
        assert!(a_callers.is_empty());

        let c_callees = graph.get_callees(c_id);
        assert!(c_callees.is_empty());

        assert_eq!(graph.roots(), vec![a_id]);
        assert_eq!(graph.leaves(), vec![c_id]);
    }

    #[test]
    fn test_transitive_callees() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/lib.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("a", "src/lib.rs"),
                make_definition("b", "src/lib.rs"),
                make_definition("c", "src/lib.rs"),
                make_definition("d", "src/lib.rs"),
            ],
            calls: vec![
                make_call("b", Some("a"), "src/lib.rs"),
                make_call("c", Some("b"), "src/lib.rs"),
                make_call("d", Some("c"), "src/lib.rs"),
            ],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        let a_id = graph.find_node("a").unwrap();

        let transitive = graph.get_transitive_callees(a_id);
        assert_eq!(transitive.len(), 3);

        let names: HashSet<_> = transitive
            .iter()
            .map(|n| n.definition.name.as_str())
            .collect();
        assert!(names.contains("b"));
        assert!(names.contains("c"));
        assert!(names.contains("d"));
    }

    #[test]
    fn test_transitive_callees_with_cycle() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/lib.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("a", "src/lib.rs"),
                make_definition("b", "src/lib.rs"),
                make_definition("c", "src/lib.rs"),
            ],
            calls: vec![
                make_call("b", Some("a"), "src/lib.rs"),
                make_call("c", Some("b"), "src/lib.rs"),
                make_call("a", Some("c"), "src/lib.rs"),
            ],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        let a_id = graph.find_node("a").unwrap();

        let transitive = graph.get_transitive_callees(a_id);
        assert_eq!(transitive.len(), 3);
    }

    #[test]
    fn test_post_order() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/lib.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("a", "src/lib.rs"),
                make_definition("b", "src/lib.rs"),
                make_definition("c", "src/lib.rs"),
            ],
            calls: vec![
                make_call("b", Some("a"), "src/lib.rs"),
                make_call("c", Some("b"), "src/lib.rs"),
            ],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        let a_id = graph.find_node("a").unwrap();
        let b_id = graph.find_node("b").unwrap();
        let c_id = graph.find_node("c").unwrap();

        let order = graph.post_order(a_id);

        let c_pos = order.iter().position(|&id| id == c_id).unwrap();
        let b_pos = order.iter().position(|&id| id == b_id).unwrap();
        let a_pos = order.iter().position(|&id| id == a_id).unwrap();

        assert!(c_pos < b_pos);
        assert!(b_pos < a_pos);
    }

    #[test]
    fn test_post_order_with_cycle() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/lib.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("a", "src/lib.rs"),
                make_definition("b", "src/lib.rs"),
            ],
            calls: vec![
                make_call("b", Some("a"), "src/lib.rs"),
                make_call("a", Some("b"), "src/lib.rs"),
            ],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        let a_id = graph.find_node("a").unwrap();

        let order = graph.post_order(a_id);
        assert_eq!(order.len(), 2);
    }

    #[test]
    fn test_post_order_definitions() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/lib.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![
                make_definition("main", "src/lib.rs"),
                make_definition("init", "src/lib.rs"),
            ],
            calls: vec![make_call("init", Some("main"), "src/lib.rs")],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        let main_id = graph.find_node("main").unwrap();

        let defs = graph.post_order_definitions(main_id);
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].name, "init");
        assert_eq!(defs[1].name, "main");
    }

    #[test]
    fn test_no_self_loops() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/lib.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![make_definition("recursive", "src/lib.rs")],
            calls: vec![make_call("recursive", Some("recursive"), "src/lib.rs")],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);
        let id = graph.find_node("recursive").unwrap();
        let node = graph.get_node(id).unwrap();

        assert!(node.callees.is_empty());
        assert!(node.callers.is_empty());
    }

    #[test]
    fn test_cross_file_calls() {
        let mut index = Index::new();

        index.update(FileRecord {
            path: PathBuf::from("src/main.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![make_definition("main", "src/main.rs")],
            calls: vec![make_call("helper", Some("main"), "src/main.rs")],
            imports: vec![],
        });

        index.update(FileRecord {
            path: PathBuf::from("src/utils.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![make_definition("helper", "src/utils.rs")],
            calls: vec![],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);

        let main_id = graph.find_node("main").unwrap();
        let callees = graph.get_callees(main_id);

        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].definition.name, "helper");
        assert_eq!(callees[0].definition.file, PathBuf::from("src/utils.rs"));
    }

    #[test]
    fn test_find_node_by_file_and_name() {
        let mut index = Index::new();

        index.update(FileRecord {
            path: PathBuf::from("src/a.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![make_definition("foo", "src/a.rs")],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: PathBuf::from("src/b.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![make_definition("foo", "src/b.rs")],
            calls: vec![],
            imports: vec![],
        });

        let graph = CallGraph::build(&index);

        let a_id = graph.find_node_by_file_and_name(Path::new("src/a.rs"), "foo");
        let b_id = graph.find_node_by_file_and_name(Path::new("src/b.rs"), "foo");

        assert!(a_id.is_some());
        assert!(b_id.is_some());
        assert_ne!(a_id, b_id);

        let a_node = graph.get_node(a_id.unwrap()).unwrap();
        let b_node = graph.get_node(b_id.unwrap()).unwrap();

        assert_eq!(a_node.definition.file, PathBuf::from("src/a.rs"));
        assert_eq!(b_node.definition.file, PathBuf::from("src/b.rs"));
    }
}
