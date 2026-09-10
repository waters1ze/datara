use forgen::ast::Program;
use forgen::diagnostics::DiagnosticEngine;
use forgen::effects::EffectAnalyzer;
use forgen::lexer::Lexer;
use forgen::parser::Parser;
use forgen::resolver::Resolver;
use forgen::semantic_graph::SemanticGraph;
use std::time::Instant;

fn build_synthetic_program(num_modules: usize, symbols_per_mod: usize) -> Program {
    let mut full_source = String::new();
    full_source.push_str("fn main() { out 42 }\n\n");

    for m in 1..=num_modules {
        full_source.push_str(&format!("class Mod{}_Data {{ id: Int }}\n", m));
        for s in 1..=symbols_per_mod {
            full_source.push_str(&format!(
                "fn mod{}_fn_{}(x: Int) -> Int => x + {}\n",
                m, s, s
            ));
        }
    }

    let mut diag = DiagnosticEngine::new("en");
    diag.set_source("scale_test.dtr", &full_source);
    let mut lexer = Lexer::new(&full_source, "scale_test.dtr");
    let tokens = lexer.tokenize(&mut diag);
    let mut parser = Parser::new(tokens, &mut diag, "scale_test.dtr");
    parser.parse_program()
}

#[test]
fn test_graph_scaling_100_modules() {
    let prog = build_synthetic_program(100, 5); // 500 symbols
    let mut diag = DiagnosticEngine::new("en");
    let mut resolver = Resolver::new();
    resolver.resolve_program(&prog, &mut diag);

    let mut effects = EffectAnalyzer::new();
    effects.analyze_program(&prog);

    let start = Instant::now();
    let graph = SemanticGraph::build(&prog, &resolver, &effects);
    let duration = start.elapsed();

    println!(
        "Graph Scale Test (100 modules, 500 symbols): {:?} (Nodes: {})",
        duration,
        graph.nodes.len()
    );
    assert!(
        duration.as_millis() < 500,
        "100-module graph build must be sub-500ms"
    );
}

#[test]
fn test_graph_scaling_500_modules() {
    let prog = build_synthetic_program(500, 2); // 1,000 symbols
    let mut diag = DiagnosticEngine::new("en");
    let mut resolver = Resolver::new();
    resolver.resolve_program(&prog, &mut diag);

    let mut effects = EffectAnalyzer::new();
    effects.analyze_program(&prog);

    let start = Instant::now();
    let graph = SemanticGraph::build(&prog, &resolver, &effects);
    let duration = start.elapsed();

    println!(
        "Graph Scale Test (500 modules, 1000 symbols): {:?} (Nodes: {})",
        duration,
        graph.nodes.len()
    );
    assert!(
        duration.as_millis() < 1000,
        "500-module graph build must be sub-1s"
    );
}
