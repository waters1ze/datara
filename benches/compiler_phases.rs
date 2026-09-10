use criterion::{Criterion, black_box, criterion_group, criterion_main};
use forgen::codegen::wasm::WasmEmitter;
use forgen::diagnostics::DiagnosticEngine;
use forgen::dmir::Lowering;
use forgen::driver::ForgenCompiler;
use forgen::lexer::Lexer;
use forgen::ownership::DmirOwnershipAnalyzer;
use forgen::parser::Parser;
use forgen::resolver::Resolver;
use forgen::types::TypeChecker;
use std::collections::HashMap;

const BENCH_PROGRAM: &str = r#"
class Point {
    x: Int
    y: Int

    fn distance_squared(other: Point) -> Int {
        let dx = self.x - other.x
        let dy = self.y - other.y
        dx * dx + dy * dy
    }
}

fn compute_factors(n: Int) -> Int {
    mut sum = 0
    mut i = 1
    while i <= n {
        if n % i == 0 {
            sum = sum + i
        }
        i = i + 1
    }
    sum
}

fn classify_number(val: Int) -> Int {
    match val {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => {
            if val > 100 {
                100
            } else {
                val * 2
            }
        }
    }
}

fn main() -> Int {
    let p1 = Point { x: 10, y: 20 }
    let p2 = Point { x: 30, y: 40 }
    let d = p1.distance_squared(p2)
    let f = compute_factors(28)
    let c = classify_number(42)
    d + f + c
}
"#;

fn bench_lexer(c: &mut Criterion) {
    c.bench_function("phase_lexer", |b| {
        b.iter(|| {
            let mut diag = DiagnosticEngine::new("en");
            let mut lexer = Lexer::new(black_box(BENCH_PROGRAM), "bench.dtr");
            let tokens = lexer.tokenize(&mut diag);
            black_box(tokens);
        })
    });
}

fn bench_parser(c: &mut Criterion) {
    let mut diag = DiagnosticEngine::new("en");
    let mut lexer = Lexer::new(BENCH_PROGRAM, "bench.dtr");
    let tokens = lexer.tokenize(&mut diag);

    c.bench_function("phase_parser", |b| {
        b.iter(|| {
            let mut diag = DiagnosticEngine::new("en");
            let mut parser = Parser::new(black_box(tokens.clone()), &mut diag, "bench.dtr");
            let prog = parser.parse_program();
            black_box(prog);
        })
    });
}

fn bench_typecheck(c: &mut Criterion) {
    let mut diag = DiagnosticEngine::new("en");
    let mut lexer = Lexer::new(BENCH_PROGRAM, "bench.dtr");
    let tokens = lexer.tokenize(&mut diag);
    let mut parser = Parser::new(tokens, &mut diag, "bench.dtr");
    let program = parser.parse_program();

    c.bench_function("phase_typecheck", |b| {
        b.iter(|| {
            let mut diag = DiagnosticEngine::new("en");
            let mut resolver = Resolver::new();
            resolver.resolve_program(black_box(&program), &mut diag);
            let mut type_checker = TypeChecker::new(&resolver);
            type_checker.check_program(black_box(&program), &mut diag);
            black_box(&type_checker);
        })
    });
}

fn bench_ownership_dataflow(c: &mut Criterion) {
    let mut diag = DiagnosticEngine::new("en");
    let mut lexer = Lexer::new(BENCH_PROGRAM, "bench.dtr");
    let tokens = lexer.tokenize(&mut diag);
    let mut parser = Parser::new(tokens, &mut diag, "bench.dtr");
    let program = parser.parse_program();
    let mut resolver = Resolver::new();
    resolver.resolve_program(&program, &mut diag);
    let mut type_checker = TypeChecker::new(&resolver);
    type_checker.check_program(&program, &mut diag);
    let mut lowering = Lowering::new(&resolver, &type_checker);
    let dmir = lowering.lower_program(&program, "main");

    c.bench_function("phase_ownership_dataflow", |b| {
        b.iter(|| {
            let mut diag = DiagnosticEngine::new("en");
            let mut analyzer = DmirOwnershipAnalyzer::new(&resolver);
            let mut sigs = HashMap::new();
            for decl in &program.declarations {
                if let forgen::ast::Decl::Function(f) = decl {
                    sigs.insert(f.name.clone(), f.params.clone());
                }
            }
            analyzer.fn_signatures = sigs;
            let mut dmir_clone = dmir.clone();
            let rep = analyzer.analyze_and_lower_module(black_box(&mut dmir_clone), &mut diag);
            black_box(rep);
        })
    });
}

fn bench_cranelift_codegen(c: &mut Criterion) {
    c.bench_function("phase_cranelift_codegen", |b| {
        b.iter(|| {
            let compiler = ForgenCompiler::new("release");
            let res = compiler.compile_source(black_box(BENCH_PROGRAM), "bench.dtr", None);
            black_box(res);
        })
    });
}

fn bench_wasm_codegen(c: &mut Criterion) {
    let mut diag = DiagnosticEngine::new("en");
    let mut lexer = Lexer::new(BENCH_PROGRAM, "bench.dtr");
    let tokens = lexer.tokenize(&mut diag);
    let mut parser = Parser::new(tokens, &mut diag, "bench.dtr");
    let program = parser.parse_program();
    let mut resolver = Resolver::new();
    resolver.resolve_program(&program, &mut diag);
    let mut type_checker = TypeChecker::new(&resolver);
    type_checker.check_program(&program, &mut diag);
    let mut lowering = Lowering::new(&resolver, &type_checker);
    let dmir = lowering.lower_program(&program, "main");

    let temp_dir = std::env::temp_dir().join("datara_wasm_bench");
    let _ = std::fs::create_dir_all(&temp_dir);
    let wasm_path = temp_dir.join("bench.wasm");

    c.bench_function("phase_wasm_codegen", |b| {
        b.iter(|| {
            let res = WasmEmitter::emit_wasm_binary(black_box(&dmir), black_box(&wasm_path));
            black_box(res).unwrap();
        })
    });
}

criterion_group!(
    benches,
    bench_lexer,
    bench_parser,
    bench_typecheck,
    bench_ownership_dataflow,
    bench_cranelift_codegen,
    bench_wasm_codegen
);
criterion_main!(benches);
