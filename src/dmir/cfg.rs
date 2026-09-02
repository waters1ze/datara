use crate::dmir::{BasicBlockId, Function, Terminator};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NaturalLoop {
    pub header: BasicBlockId,
    pub back_edges: Vec<BasicBlockId>,
    pub blocks: HashSet<BasicBlockId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlFlowGraph {
    pub entry: BasicBlockId,
    pub blocks: Vec<BasicBlockId>,
    pub predecessors: HashMap<BasicBlockId, Vec<BasicBlockId>>,
    pub successors: HashMap<BasicBlockId, Vec<BasicBlockId>>,
    pub idom: HashMap<BasicBlockId, BasicBlockId>,
    pub dom_tree_children: HashMap<BasicBlockId, Vec<BasicBlockId>>,
    pub dominance_frontiers: HashMap<BasicBlockId, HashSet<BasicBlockId>>,
    pub loops: Vec<NaturalLoop>,
}

impl ControlFlowGraph {
    pub fn build(func: &Function) -> Self {
        let entry = func.entry_block;
        let mut blocks = Vec::new();
        let mut predecessors: HashMap<BasicBlockId, Vec<BasicBlockId>> = HashMap::new();
        let mut successors: HashMap<BasicBlockId, Vec<BasicBlockId>> = HashMap::new();

        for b in &func.blocks {
            blocks.push(b.id);
            predecessors.entry(b.id).or_default();
            successors.entry(b.id).or_default();
        }

        for b in &func.blocks {
            let succs = match &b.terminator {
                Terminator::Branch { target, .. } => vec![*target],
                Terminator::CondBranch {
                    then_block,
                    else_block,
                    ..
                } => {
                    let mut s = vec![*then_block];
                    if *else_block != *then_block {
                        s.push(*else_block);
                    }
                    s
                }
                Terminator::Return { .. } | Terminator::Unreachable => Vec::new(),
            };

            for s in &succs {
                successors.entry(b.id).or_default().push(*s);
                predecessors.entry(*s).or_default().push(b.id);
            }
        }

        let mut cfg = Self {
            entry,
            blocks,
            predecessors,
            successors,
            idom: HashMap::new(),
            dom_tree_children: HashMap::new(),
            dominance_frontiers: HashMap::new(),
            loops: Vec::new(),
        };

        cfg.compute_dominance(func);
        cfg.compute_dominance_frontiers();
        cfg.compute_natural_loops();

        cfg
    }

    fn compute_dominance(&mut self, _func: &Function) {
        if self.blocks.is_empty() {
            return;
        }

        // Post-order traversal for fast convergence
        let mut visited = HashSet::new();
        let mut post_order = Vec::new();
        let mut stack = vec![(self.entry, false)];

        while let Some((node, processed)) = stack.pop() {
            if processed {
                post_order.push(node);
            } else if !visited.contains(&node) {
                visited.insert(node);
                stack.push((node, true));
                if let Some(succs) = self.successors.get(&node) {
                    for &s in succs {
                        if !visited.contains(&s) {
                            stack.push((s, false));
                        }
                    }
                }
            }
        }

        let mut rpo = post_order;
        rpo.reverse();

        let mut rpo_index: HashMap<BasicBlockId, usize> = HashMap::new();
        for (i, &b) in rpo.iter().enumerate() {
            rpo_index.insert(b, i);
        }

        let mut idom: HashMap<BasicBlockId, BasicBlockId> = HashMap::new();
        idom.insert(self.entry, self.entry);

        let mut changed = true;
        while changed {
            changed = false;
            for &b in &rpo {
                if b == self.entry {
                    continue;
                }

                let preds = self.predecessors.get(&b).cloned().unwrap_or_default();
                let mut processed_preds = preds.iter().filter(|p| idom.contains_key(p));

                if let Some(&first_p) = processed_preds.next() {
                    let mut new_idom = first_p;
                    for &other_p in processed_preds {
                        new_idom = Self::intersect(other_p, new_idom, &idom, &rpo_index);
                    }

                    if idom.get(&b) != Some(&new_idom) {
                        idom.insert(b, new_idom);
                        changed = true;
                    }
                }
            }
        }

        self.idom = idom;

        for (&b, &parent) in &self.idom {
            if b != self.entry {
                self.dom_tree_children.entry(parent).or_default().push(b);
            }
        }
    }

    fn intersect(
        mut b1: BasicBlockId,
        mut b2: BasicBlockId,
        idom: &HashMap<BasicBlockId, BasicBlockId>,
        rpo_index: &HashMap<BasicBlockId, usize>,
    ) -> BasicBlockId {
        while b1 != b2 {
            while *rpo_index.get(&b1).unwrap_or(&usize::MAX)
                > *rpo_index.get(&b2).unwrap_or(&usize::MAX)
            {
                let parent = *idom.get(&b1).unwrap_or(&b1);
                if parent == b1 {
                    break;
                }
                b1 = parent;
            }
            while *rpo_index.get(&b2).unwrap_or(&usize::MAX)
                > *rpo_index.get(&b1).unwrap_or(&usize::MAX)
            {
                let parent = *idom.get(&b2).unwrap_or(&b2);
                if parent == b2 {
                    break;
                }
                b2 = parent;
            }
            if *idom.get(&b1).unwrap_or(&b1) == b1
                && *idom.get(&b2).unwrap_or(&b2) == b2
                && b1 != b2
            {
                break;
            }
        }
        b1
    }

    pub fn dominates(&self, a: BasicBlockId, mut b: BasicBlockId) -> bool {
        if a == b {
            return true;
        }
        while let Some(&parent) = self.idom.get(&b) {
            if parent == a {
                return true;
            }
            if parent == b {
                break;
            }
            b = parent;
        }
        false
    }

    pub fn strictly_dominates(&self, a: BasicBlockId, b: BasicBlockId) -> bool {
        a != b && self.dominates(a, b)
    }

    fn compute_dominance_frontiers(&mut self) {
        let mut df: HashMap<BasicBlockId, HashSet<BasicBlockId>> = HashMap::new();
        for &b in &self.blocks {
            df.insert(b, HashSet::new());
        }

        for &b in &self.blocks {
            let preds = self.predecessors.get(&b).cloned().unwrap_or_default();
            if preds.len() >= 2 {
                for &p in &preds {
                    let mut runner = p;
                    let target_idom = self.idom.get(&b).copied().unwrap_or(b);
                    while runner != target_idom && runner != self.entry {
                        df.entry(runner).or_default().insert(b);
                        runner = self.idom.get(&runner).copied().unwrap_or(runner);
                    }
                }
            }
        }

        self.dominance_frontiers = df;
    }

    fn compute_natural_loops(&mut self) {
        let mut loops: Vec<NaturalLoop> = Vec::new();

        for &n in &self.blocks {
            if let Some(succs) = self.successors.get(&n) {
                for &header in succs {
                    if self.dominates(header, n) {
                        // Found back-edge n -> header
                        let mut body = HashSet::new();
                        body.insert(header);
                        body.insert(n);

                        let mut worklist = VecDeque::new();
                        worklist.push_back(n);

                        while let Some(m) = worklist.pop_front() {
                            if let Some(preds) = self.predecessors.get(&m) {
                                for &p in preds {
                                    if body.insert(p) {
                                        worklist.push_back(p);
                                    }
                                }
                            }
                        }

                        if let Some(existing) = loops.iter_mut().find(|l| l.header == header) {
                            existing.back_edges.push(n);
                            existing.blocks.extend(body);
                        } else {
                            loops.push(NaturalLoop {
                                header,
                                back_edges: vec![n],
                                blocks: body,
                            });
                        }
                    }
                }
            }
        }

        self.loops = loops;
    }
}
