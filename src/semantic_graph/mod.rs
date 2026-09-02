use crate::ast::*;
use crate::effects::EffectAnalyzer;
use crate::optimizer::OptimizationReport;
use crate::resolver::Resolver;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    Module,
    Symbol,
    Type,
    Function,
    Class,
    Behavior,
    Role,
    Component,
    Call,
    DataFlow,
    Effect,
    Ownership,
    GenericInstance,
    EntryPoint,
    FFIBoundary,
    RuntimeCapability,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Calls,
    Uses,
    Returns,
    Owns,
    Borrows,
    Implements,
    Composes,
    Extends,
    Replaces,
    DependsOn,
    HasEffect,
    Reads,
    Writes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationFacts {
    pub inlining: InliningFact,
    pub allocation: AllocationFact,
    pub vectorization: VectorizationFact,
    pub parallelization: ParallelizationFact,
    pub generic_specializations: Vec<String>,
    pub runtime_modules_linked: Vec<String>,
    pub runtime_modules_stripped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InliningFact {
    pub applied: bool,
    pub inlined_calls: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationFact {
    pub eliminated_allocations: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorizationFact {
    pub enabled: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelizationFact {
    pub applied: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub is_reachable: bool,
    pub effects: String,
    pub ownership: String,
    pub metadata: serde_json::Value,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
    pub compositions: Vec<String>,
    pub optimization: OptimizationFacts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticGraph {
    pub nodes: HashMap<String, GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub root_entry_points: Vec<String>,
}

impl SemanticGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            root_entry_points: vec!["fn:main".to_string()],
        }
    }

    pub fn add_edge(&mut self, source: &str, target: &str, kind: EdgeKind) {
        self.edges.push(GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            kind,
        });
    }

    pub fn build(_program: &Program, resolver: &Resolver, effects: &EffectAnalyzer) -> Self {
        let mut graph = Self::new();

        // 1. Add classes
        for (name, sym) in &resolver.classes {
            let mut fields_map = serde_json::Map::new();
            for f in sym.fields.keys() {
                fields_map.insert(f.clone(), serde_json::Value::String("String".into()));
            }
            let mut methods_map = serde_json::Map::new();
            for m in sym.methods.keys() {
                methods_map.insert(m.clone(), serde_json::Value::String("() -> String".into()));
            }

            let mut meta = serde_json::Map::new();
            meta.insert(
                "baseType".into(),
                serde_json::to_value(&sym.base_type).unwrap_or(serde_json::Value::Null),
            );
            meta.insert(
                "compositions".into(),
                serde_json::to_value(&sym.compositions).unwrap(),
            );
            meta.insert("fields".into(), serde_json::Value::Object(fields_map));
            meta.insert("methods".into(), serde_json::Value::Object(methods_map));
            meta.insert(
                "span".into(),
                serde_json::Value::String(sym.span.to_string()),
            );

            let class_id = format!("class:{}", name);
            graph.nodes.insert(
                class_id.clone(),
                GraphNode {
                    id: class_id.clone(),
                    kind: NodeKind::Class,
                    label: name.clone(),
                    is_reachable: true,
                    effects: "Pure".to_string(),
                    ownership: "Aggregate (Value/Object)".to_string(),
                    metadata: serde_json::Value::Object(meta),
                    callers: Vec::new(),
                    callees: Vec::new(),
                    compositions: sym.compositions.clone(),
                    optimization: OptimizationFacts {
                        inlining: InliningFact {
                            applied: false,
                            inlined_calls: Vec::new(),
                            reason: "Type abstraction".into(),
                        },
                        allocation: AllocationFact {
                            eliminated_allocations: 0,
                            reason: "Zero-cost scalar candidate".into(),
                        },
                        vectorization: VectorizationFact {
                            enabled: false,
                            reason: "Non-vectorizable aggregate type".into(),
                        },
                        parallelization: ParallelizationFact {
                            applied: false,
                            reason: "Sequential aggregate".into(),
                        },
                        generic_specializations: Vec::new(),
                        runtime_modules_linked: vec!["core".into()],
                        runtime_modules_stripped: vec!["network".into(), "database".into()],
                    },
                },
            );

            for (m_name, _m_sym) in &sym.methods {
                let m_id = format!("method:{}.{}", name, m_name);
                let eff_str = effects
                    .function_effects
                    .get(&format!("{}.{}", name, m_name))
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "Pure".into());

                let mut m_meta = serde_json::Map::new();
                m_meta.insert(
                    "returnType".into(),
                    serde_json::Value::String("String".into()),
                );
                m_meta.insert("effects".into(), serde_json::Value::String(eff_str.clone()));

                let is_pure = eff_str == "Pure";

                graph.nodes.insert(
                    m_id.clone(),
                    GraphNode {
                        id: m_id.clone(),
                        kind: NodeKind::Function,
                        label: format!("{}.{}", name, m_name),
                        is_reachable: true,
                        effects: eff_str.clone(),
                        ownership: "Borrowed (this)".to_string(),
                        metadata: serde_json::Value::Object(m_meta),
                        callers: Vec::new(),
                        callees: Vec::new(),
                        compositions: Vec::new(),
                        optimization: OptimizationFacts {
                            inlining: InliningFact {
                                applied: false,
                                inlined_calls: Vec::new(),
                                reason: if is_pure {
                                    "Pure method candidate for leaf inlining".into()
                                } else {
                                    "Has side effects; outline dispatch required".into()
                                },
                            },
                            allocation: AllocationFact {
                                eliminated_allocations: 0,
                                reason: "SROA evaluation pending whole-program reachability".into(),
                            },
                            vectorization: VectorizationFact {
                                enabled: false,
                                reason: "No loop induction found in method body".into(),
                            },
                            parallelization: ParallelizationFact {
                                applied: false,
                                reason: "Sequential method execution".into(),
                            },
                            generic_specializations: Vec::new(),
                            runtime_modules_linked: vec!["core".into()],
                            runtime_modules_stripped: vec!["network".into(), "database".into()],
                        },
                    },
                );

                graph.add_edge(&class_id, &m_id, EdgeKind::Owns);
            }
        }

        // 2. Add functions
        for (name, sym) in &resolver.functions {
            let eff_str = effects
                .function_effects
                .get(name)
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Pure".into());

            let mut f_meta = serde_json::Map::new();
            f_meta.insert(
                "returnType".into(),
                serde_json::Value::String("Unit".into()),
            );
            f_meta.insert("effects".into(), serde_json::Value::String(eff_str.clone()));
            f_meta.insert(
                "span".into(),
                serde_json::Value::String(sym.span.to_string()),
            );

            let fn_id = format!("fn:{}", name);
            let is_main = name == "main";

            graph.nodes.insert(
                fn_id.clone(),
                GraphNode {
                    id: fn_id.clone(),
                    kind: if is_main {
                        NodeKind::EntryPoint
                    } else {
                        NodeKind::Function
                    },
                    label: name.clone(),
                    is_reachable: is_main,
                    effects: eff_str.clone(),
                    ownership: "Value / Immutable View".to_string(),
                    metadata: serde_json::Value::Object(f_meta),
                    callers: Vec::new(),
                    callees: Vec::new(),
                    compositions: Vec::new(),
                    optimization: OptimizationFacts {
                        inlining: InliningFact {
                            applied: false,
                            inlined_calls: Vec::new(),
                            reason: if is_main {
                                "Entry point cannot be inlined".into()
                            } else {
                                "Subject to optimizer cost model evaluation".into()
                            },
                        },
                        allocation: AllocationFact {
                            eliminated_allocations: 0,
                            reason: "Subject to SROA escape analysis".into(),
                        },
                        vectorization: VectorizationFact {
                            enabled: false,
                            reason: "Non-loop scalar function".into(),
                        },
                        parallelization: ParallelizationFact {
                            applied: false,
                            reason: "Sequential execution".into(),
                        },
                        generic_specializations: Vec::new(),
                        runtime_modules_linked: vec!["core".into(), "io".into()],
                        runtime_modules_stripped: vec![
                            "network".into(),
                            "database".into(),
                            "gui".into(),
                        ],
                    },
                },
            );
        }

        graph
    }

    pub fn attach_optimization_report(
        &mut self,
        report: &OptimizationReport,
        dmir_module: &crate::dmir::Module,
    ) {
        for (node_id, node) in self.nodes.iter_mut() {
            let fn_key = if node_id.starts_with("fn:") {
                node_id.trim_start_matches("fn:").to_string()
            } else if node_id.starts_with("method:") {
                node_id.trim_start_matches("method:").replace('.', "_")
            } else {
                "".to_string()
            };

            if !fn_key.is_empty() {
                node.is_reachable = dmir_module.functions.contains_key(&fn_key);
                if !node.is_reachable {
                    node.optimization.inlining.reason =
                        "Stripped: unreachable from main entry point".into();
                    node.optimization.allocation.reason =
                        "Stripped by dead symbol elimination".into();
                } else if report.functions_inlined > 0 && fn_key == "main" {
                    node.optimization.inlining.applied = true;
                    node.optimization.inlining.reason = format!(
                        "Successfully inlined {} pure call(s) directly into main body",
                        report.functions_inlined
                    );
                }

                if report.allocations_eliminated > 0 && node.is_reachable {
                    node.optimization.allocation.eliminated_allocations =
                        report.allocations_eliminated;
                    node.optimization.allocation.reason = format!(
                        "Scalarized {} non-escaping local struct allocation(s) onto stack (SROA)",
                        report.allocations_eliminated
                    );
                }

                node.optimization.generic_specializations = report.generic_specializations.clone();
                node.optimization.runtime_modules_linked = report.runtime_modules_linked.clone();
                node.optimization.runtime_modules_stripped =
                    report.runtime_modules_stripped.clone();
            }
        }
    }

    // --- Semantic Graph 2.0 Query API ---

    pub fn find_symbol(&self, name: &str) -> Option<&GraphNode> {
        for node in self.nodes.values() {
            if node.label == name
                || node.label.ends_with(&format!(".{}", name))
                || node.id.ends_with(&format!(":{}", name))
            {
                return Some(node);
            }
        }
        None
    }

    pub fn find_dependencies(&self, id: &str) -> Vec<String> {
        let mut deps = Vec::new();
        for edge in &self.edges {
            if edge.source == id
                && matches!(
                    edge.kind,
                    EdgeKind::DependsOn | EdgeKind::Calls | EdgeKind::Uses
                )
            {
                deps.push(edge.target.clone());
            }
        }
        deps
    }

    pub fn find_callers(&self, symbol_or_id: &str) -> Vec<String> {
        let id = if self.nodes.contains_key(symbol_or_id) {
            symbol_or_id.to_string()
        } else if let Some(n) = self.find_symbol(symbol_or_id) {
            n.id.clone()
        } else {
            symbol_or_id.to_string()
        };
        let mut callers = Vec::new();
        for edge in &self.edges {
            if edge.target == id && matches!(edge.kind, EdgeKind::Calls) {
                callers.push(edge.source.clone());
            }
        }
        callers
    }

    pub fn find_callees(&self, symbol_or_id: &str) -> Vec<String> {
        let id = if self.nodes.contains_key(symbol_or_id) {
            symbol_or_id.to_string()
        } else if let Some(n) = self.find_symbol(symbol_or_id) {
            n.id.clone()
        } else {
            symbol_or_id.to_string()
        };
        let mut callees = Vec::new();
        for edge in &self.edges {
            if edge.source == id && matches!(edge.kind, EdgeKind::Calls) {
                callees.push(edge.target.clone());
            }
        }
        callees
    }

    pub fn find_effects(&self, name: &str) -> Option<serde_json::Value> {
        let node = self.find_symbol(name)?;
        let mut obj = serde_json::Map::new();
        obj.insert(
            "symbol".into(),
            serde_json::Value::String(node.label.clone()),
        );
        if let Some(eff) = node.metadata.get("effects") {
            obj.insert("effects".into(), eff.clone());
        } else {
            obj.insert("effects".into(), serde_json::Value::String("Pure".into()));
        }
        Some(serde_json::Value::Object(obj))
    }

    pub fn find_ownership(&self, name: &str) -> Option<serde_json::Value> {
        let node = self.find_symbol(name)?;
        let mut obj = serde_json::Map::new();
        obj.insert(
            "symbol".into(),
            serde_json::Value::String(node.label.clone()),
        );
        obj.insert(
            "is_reachable".into(),
            serde_json::Value::Bool(node.is_reachable),
        );
        obj.insert(
            "kind".into(),
            serde_json::to_value(&node.kind).unwrap_or(serde_json::Value::Null),
        );
        Some(serde_json::Value::Object(obj))
    }

    pub fn find_reachable(&self) -> Vec<String> {
        self.nodes
            .values()
            .filter(|n| n.is_reachable)
            .map(|n| n.id.clone())
            .collect()
    }

    pub fn find_specializations(&self, class_name: &str) -> Vec<String> {
        if let Some(node) = self.find_symbol(class_name) {
            node.optimization.generic_specializations.clone()
        } else {
            Vec::new()
        }
    }

    pub fn find_runtime_dependencies(&self) -> Vec<String> {
        let mut set = HashSet::new();
        for node in self.nodes.values() {
            if node.is_reachable {
                for r in &node.optimization.runtime_modules_linked {
                    set.insert(r.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    pub fn inspect_symbol(&self, name: &str) -> Option<&GraphNode> {
        self.find_symbol(name)
    }

    pub fn inspect_effects(&self, name: &str) -> Option<serde_json::Value> {
        self.find_effects(name)
    }

    pub fn inspect_optimization(&self, name: &str) -> Option<serde_json::Value> {
        let node = self.find_symbol(name)?;
        let mut root = serde_json::Map::new();
        root.insert(
            "symbol".into(),
            serde_json::Value::String(node.label.clone()),
        );
        root.insert(
            "kind".into(),
            serde_json::to_value(&node.kind).unwrap_or(serde_json::Value::String("Unknown".into())),
        );
        root.insert(
            "isReachable".into(),
            serde_json::Value::Bool(node.is_reachable),
        );
        root.insert(
            "optimizationFacts".into(),
            serde_json::to_value(&node.optimization).unwrap(),
        );
        Some(serde_json::Value::Object(root))
    }

    pub fn inspect_dependencies(&self, name: &str) -> Vec<String> {
        if let Some(node) = self.find_symbol(name) {
            self.find_dependencies(&node.id)
        } else {
            Vec::new()
        }
    }
}
