use ::ir::SSAVariable;
use super::{ Phi, Op };

mod builder;
pub use self::builder::FunctionCfgBuilder;

mod graph;

pub type LabelN = self::graph::NodeLabel;
pub type EdgeN = self::graph::EdgeLabel;

#[derive(Debug)]
pub struct BasicBlock {
    pub phi_nodes: Vec<Phi>,
    pub ops: Vec<Op>,
}

#[derive(Debug)]
pub struct FunctionCfg {
    pub entry: LabelN,
    pub args: Vec<SSAVariable>,
    pub graph: graph::Graph<BasicBlock, BasicBlockEdge>,
}

#[derive(Debug)]
pub struct BasicBlockEdge {
    writes: Vec<SSAVariable>,
}

impl FunctionCfg {

    pub fn new() -> Self {
        let mut graph = graph::Graph::new();

        let entry = graph.add_node(BasicBlock {
            phi_nodes: vec![],
            ops: vec![],
        });

        FunctionCfg {
            entry: entry,
            args: vec![],
            graph: graph,
        }
    }

    pub fn entry(&self) -> LabelN {
        self.entry
    }

    pub fn remove_edge(&mut self, lbl: EdgeN) {
        let (edge_from_label, edge_to_label) = {
            let edge_container = &self.graph[lbl];
            (edge_container.from, edge_container.to)
        };

        {
            let dst_container = &self.graph[edge_to_label];
            let mut dst = dst_container.inner.borrow_mut();
            for phi in dst.phi_nodes.iter_mut() {
                let pos = phi.entries.iter()
                    .position(|(from, _src)| *from == edge_from_label)
                    .expect("Phi node was invalid!");
                phi.entries.remove(pos);
            }
        }

        self.graph.remove_edge(lbl);
    }

    pub fn branch_slots(&self, lbl: LabelN) -> Vec<LabelN> {
        self.graph[lbl].outgoing.iter().map(|v| v.1).collect()
    }

    pub fn remove_block(&mut self, lbl: LabelN) {
    //    // Validate that the node is not the entry point
    //    // and is never jumped to
    //    assert!(lbl != self.entry());
    //    for label in self.labels_iter() {
    //        for jump in self.jumps_iter(label) {
    //            assert!(self.edge_target(jump) != lbl);
    //        }
    //    }

    //    // Remove from graph
    //    self.dead_blocks.insert(lbl);

    //    // Remove from phi nodes
    //    for block in self.blocks_iter_mut() {
    //        for phi in block.phi_nodes.iter_mut() {
    //            phi.entries.retain(|entry| entry.0 != lbl);
    //        }
    //    }
    }

}
