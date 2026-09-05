//! Introspection commands: sae, why, context, inspect (semantic graph, DMIR,
//! codegen and CLIF inspection).

use super::{to_json_value, to_pretty_json};
use crate::driver::ForgenCompiler;
use crate::project::ProjectDiscovery;
use std::path::Path;

/// `forgen sae` — Semantic Adaptation Engine decision report.
pub(crate) fn cmd_sae(args: &[String]) -> bool {
    let target_opt = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("-") && *a != "--")
        .map(Path::new);
    let layout = match ProjectDiscovery::discover(target_opt) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("SAE discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let compiler = ForgenCompiler::new("domain");
    let res = if layout.source_files.len() == 1 {
        compiler.compile_file(&layout.source_files[0], None)
    } else {
        compiler.compile_files(&layout.source_files, None)
    };

    if res.success {
        let rep = res.optimization_report.unwrap_or_default();
        if args.iter().any(|a| a == "--json") {
            println!("{}", to_pretty_json(&rep.adaptation_records));
            return false;
        }

        println!(
            "=================================================================================================="
        );
        println!(
            "                   FORGEN SEMANTIC ADAPTATION ENGINE (SAE) DECISION REPORT                        "
        );
        println!(
            "=================================================================================================="
        );
        println!(
            "{:<16} | {:<24} | {:<28} | {:<8} | {:<8}",
            "Category", "Candidate", "Decision", "Benefit", "Cost"
        );
        println!(
            "--------------------------------------------------------------------------------------------------"
        );
        for r in &rep.adaptation_records {
            println!(
                "{:?} | {:<24} | {:<28} | {:>6.1}x | {:>6.1}x",
                r.category, r.candidate, r.decision, r.benefit, r.cost
            );
            println!("  Reason:   {}", r.reason);
            println!("  Evidence: {}", r.evidence);
            println!(
                "- - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - -"
            );
        }
        println!(
            "=================================================================================================="
        );
    } else {
        eprintln!("{}", res.error.unwrap_or_default());
        std::process::exit(1);
    }
    true
}

/// `forgen why` — explain optimization decisions for a symbol.
pub(crate) fn cmd_why(args: &[String]) -> bool {
    if args.len() < 3 {
        eprintln!("Usage: forgen why <symbol> [file.dtr] [--json]");
        std::process::exit(1);
    }
    let target_symbol = &args[2];
    let file_target = args.get(3).filter(|s| !s.starts_with("--")).map(Path::new);

    let compiler = ForgenCompiler::new("domain");
    let layout = match ProjectDiscovery::discover(file_target) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Why discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let res = if layout.source_files.len() == 1 {
        compiler.compile_file(&layout.source_files[0], None)
    } else {
        compiler.compile_files(&layout.source_files, None)
    };

    if let Some(graph) = res.semantic_graph {
        let rep = res.optimization_report.unwrap_or_default();
        let matching_decisions: Vec<_> = rep
            .decision_trace
            .iter()
            .filter(|d| d.candidate.contains(target_symbol))
            .collect();

        if args.iter().any(|a| a == "--json") {
            let mut json_obj = serde_json::Map::new();
            json_obj.insert(
                "symbol".into(),
                serde_json::Value::String(target_symbol.clone()),
            );
            if let Some(node) = graph.inspect_symbol(target_symbol) {
                json_obj.insert("node".into(), to_json_value(node));
            }
            json_obj.insert("decisions".into(), to_json_value(&matching_decisions));
            println!("{}", to_pretty_json(&json_obj));
            return false;
        }

        println!("============================================================");
        println!(
            "             FORGEN EXPLAINABILITY REPORT: {}               ",
            target_symbol
        );
        println!("============================================================");
        if let Some(node) = graph.inspect_symbol(target_symbol) {
            println!(" Symbol Kind:      {:?}", node.kind);
            println!(" Effects Lattice:  {:?}", node.effects);
            println!(" Ownership Model:  {:?}", node.ownership);
            println!(
                " Dependencies:     {:?}",
                graph.inspect_dependencies(target_symbol)
            );
        } else {
            println!(" Symbol:           {}", target_symbol);
        }
        println!("------------------------------------------------------------");
        println!(" Optimization Decisions & Cost Model Breakdown:");
        if matching_decisions.is_empty() {
            println!("   (No specific function-level optimization overrides recorded)");
        } else {
            for d in matching_decisions {
                println!("   [{}] Decision: {}", d.pass, d.decision);
                println!("     Benefit: {}", d.estimated_benefit);
                println!("     Cost:    {}", d.estimated_cost);
                println!("     Reason:  {}", d.reason);
            }
        }
        println!("============================================================");
    } else {
        eprintln!("Compilation failed: {}", res.error.unwrap_or_default());
        std::process::exit(1);
    }
    true
}

/// `forgen context` — structured semantic metadata for a symbol (JSON).
pub(crate) fn cmd_context(args: &[String]) -> bool {
    if args.len() < 3 {
        eprintln!("Usage: forgen context <symbol> [file.dtr]");
        std::process::exit(1);
    }
    let target_symbol = &args[2];
    let file_target = args.get(3).filter(|s| !s.starts_with("--")).map(Path::new);

    let compiler = ForgenCompiler::new("domain");
    let layout = match ProjectDiscovery::discover(file_target) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Context discovery error: {}", e);
            std::process::exit(1);
        }
    };

    let res = if layout.source_files.len() == 1 {
        compiler.compile_file(&layout.source_files[0], None)
    } else {
        compiler.compile_files(&layout.source_files, None)
    };

    if let Some(graph) = res.semantic_graph {
        let rep = res.optimization_report.unwrap_or_default();
        let matching_decisions: Vec<_> = rep
            .decision_trace
            .iter()
            .filter(|d| d.candidate.contains(target_symbol))
            .collect();

        let mut context_map = serde_json::Map::new();
        context_map.insert(
            "symbol".into(),
            serde_json::Value::String(target_symbol.clone()),
        );

        if let Some(node) = graph.inspect_symbol(target_symbol) {
            context_map.insert("kind".into(), to_json_value(&node.kind));
            context_map.insert("effects".into(), to_json_value(&node.effects));
            context_map.insert("ownership".into(), to_json_value(&node.ownership));
        }

        context_map.insert(
            "dependencies".into(),
            to_json_value(&graph.inspect_dependencies(target_symbol)),
        );
        context_map.insert(
            "callers".into(),
            to_json_value(&graph.find_callers(target_symbol)),
        );
        context_map.insert(
            "callees".into(),
            to_json_value(&graph.find_callees(target_symbol)),
        );
        context_map.insert(
            "optimizationDecisions".into(),
            to_json_value(&matching_decisions),
        );

        println!("{}", to_pretty_json(&context_map));
    } else {
        eprintln!("Compilation failed: {}", res.error.unwrap_or_default());
        std::process::exit(1);
    }
    true
}

/// `forgen inspect` — inspect semantic graph, AST, DMIR, codegen, asm, CLIF.
pub(crate) fn cmd_inspect(args: &[String]) -> bool {
    if args.len() < 4 {
        println!(
            "Usage: forgen inspect <symbol|effects|optimize|dependencies|ast|dmir|codegen|asm|clif> <file.dtr> [symbolName]"
        );
        return false;
    }
    let query = &args[2];
    let file_path = Path::new(&args[3]);
    let target_symbol = args.get(4).map(|s| s.as_str());

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_file(file_path, None);

    if let Some(graph) = res.semantic_graph {
        match query.as_str() {
            "symbol" => {
                let sym_name = target_symbol.unwrap_or("main");
                if let Some(node) = graph.inspect_symbol(sym_name) {
                    println!("{}", to_pretty_json(node));
                } else {
                    println!("Symbol '{}' not found in semantic graph", sym_name);
                }
            }
            "effects" => {
                if let Some(sym_name) = target_symbol {
                    if let Some(eff) = graph.inspect_effects(sym_name) {
                        println!("{}", to_pretty_json(&eff));
                    } else {
                        println!("Symbol '{}' not found", sym_name);
                    }
                } else {
                    // `SemanticGraph` stores its nodes in a HashMap
                    // whose iteration order differs between runs.
                    // Round-trip through `serde_json::Value`
                    // (BTreeMap-backed, key-sorted) so the emitted
                    // JSON is byte-identical every run.
                    println!("{}", to_pretty_json(&to_json_value(&graph)));
                }
            }
            "optimize" => {
                let sym_name = target_symbol.unwrap_or("main");
                if let Some(opt) = graph.inspect_optimization(sym_name) {
                    println!("{}", to_pretty_json(&opt));
                } else {
                    println!("Symbol '{}' not found in semantic graph", sym_name);
                }
            }
            "dependencies" => {
                let sym_name = target_symbol.unwrap_or("main");
                let deps = graph.inspect_dependencies(sym_name);
                println!("{}", to_pretty_json(&deps));
            }
            "ast" => {
                if let Some(prog) = res.program {
                    println!("{}", to_pretty_json(&prog));
                }
            }
            "dmir" => {
                if let Some(dmir) = res.dmir_module {
                    // The module stores functions, class fields and
                    // extern signatures in HashMaps, whose iteration
                    // order differs between runs. Round-trip through
                    // `serde_json::Value` (BTreeMap-backed, key-sorted)
                    // so the emitted JSON is byte-identical every run.
                    let value = to_json_value(&dmir);
                    println!("{}", to_pretty_json(&value));
                }
            }
            "codegen" => {
                if let Some(dmir) = &res.dmir_module {
                    let cranelift_backend = crate::codegen::cranelift::CraneliftBackend::for_host();
                    let inspection = cranelift_backend.inspect_module(dmir);
                    if args.iter().any(|a| a == "--json") {
                        println!("{}", to_pretty_json(&inspection));
                    } else {
                        println!("============================================================");
                        println!("             FORGEN CODEGEN & MACHINE CODE INSPECTION       ");
                        println!("============================================================");
                        println!(" Target:               {}", inspection.target);
                        println!(" Calling Convention:   {}", inspection.calling_convention);
                        println!(" Total Functions:      {}", inspection.total_functions);
                        println!(" Total Instructions:   {}", inspection.total_instructions);
                        println!(
                            " Heap Allocations:     {} (Zero-Cost Stack/Scalarized)",
                            inspection.total_heap_allocations
                        );
                        println!("------------------------------------------------------------");
                        println!(
                            " {:<20} | {:<5} | {:<7} | {:<5} | {:<5} | {:<5}",
                            "Function", "Insts", "Stack", "Slots", "Calls", "Branch"
                        );
                        println!("------------------------------------------------------------");
                        for f in &inspection.functions {
                            println!(
                                " {:<20} | {:>5} | {:>5}B | {:>5} | {:>5} | {:>5}",
                                f.name,
                                f.instruction_count,
                                f.stack_frame_bytes,
                                f.explicit_stack_slots,
                                f.direct_calls,
                                f.branches
                            );
                        }
                        println!("============================================================");
                    }
                }
            }
            "asm" | "clif" => {
                if let Some(clif) = res.clif_source {
                    println!("{}", clif);
                } else if let Some(dmir) = &res.dmir_module {
                    let cranelift_backend = crate::codegen::cranelift::CraneliftBackend::for_host();
                    let resolver = crate::resolver::Resolver::new();
                    let types = crate::types::TypeChecker::new(&resolver);
                    let prog = res.program.as_ref().unwrap();
                    let clif = cranelift_backend.emit_clif(dmir, prog, &types);
                    println!("{}", clif);
                }
            }
            _ => {
                // Round-trip through serde_json::Value so the
                // SemanticGraph's internal HashMaps serialize with
                // sorted keys (deterministic output).
                println!("{}", to_pretty_json(&to_json_value(&graph)));
            }
        }
    } else {
        eprintln!("Compilation failed: {}", res.error.unwrap_or_default());
    }
    true
}
