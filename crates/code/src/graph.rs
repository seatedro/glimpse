use std::collections::{HashMap, HashSet};

use anyhow::Result;

use super::index::{Definition, Index};

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
}

impl CallGraph {
    pub fn build(_index: &Index) -> Result<Self> {
        todo!("build call graph from index")
    }

    pub fn get_callees(&self, _node_id: NodeId) -> Vec<&CallGraphNode> {
        todo!("get direct callees")
    }

    pub fn get_transitive_callees(&self, _node_id: NodeId) -> Vec<&CallGraphNode> {
        todo!("get all callees recursively")
    }

    pub fn post_order(&self, _node_id: NodeId) -> Vec<NodeId> {
        todo!("return nodes in post-order traversal")
    }
}
