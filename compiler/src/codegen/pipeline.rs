//! Top-level semantic and backend phase orchestration.
//!
//! The phase wrappers are deliberately private. They make the ordering
//! contract explicit without exposing the current HIR or cleanup-plan shapes
//! as a public compiler API.

use std::collections::HashMap;

use crate::ast::Program;
use crate::cleanup::CleanupPlan;

use super::cleanup_plan::build_and_verify_cleanup_plans;
use super::emitter::{evaluate_globals, ConstValue, Emitter};
use super::{Analyzer, Diagnostic, HirProgram};

struct AnalyzedProgram {
    hir: HirProgram,
}

struct PreparedProgram {
    hir: HirProgram,
    cleanup_plans: Vec<CleanupPlan>,
    constants: HashMap<String, ConstValue>,
}

/// Type-check `program` and emit portable textual LLVM IR using opaque
/// pointers. The returned module deliberately omits a target triple so that
/// the caller can compile it for the selected LLVM target. The program must
/// already have passed module resolution; use the crate-level source entry
/// points when compiling parser input.
pub fn compile(program: &Program) -> Result<String, Vec<Diagnostic>> {
    compile_target(program, true)
}

/// Type-check `program` and emit LLVM IR for a library target. Unlike
/// [`compile`], this does not require `main` or generate the platform entry
/// wrapper. The program must already have passed module resolution.
pub fn compile_library(program: &Program) -> Result<String, Vec<Diagnostic>> {
    compile_target(program, false)
}

/// Type-check a library target without requiring or emitting a binary entry
/// point. Global constants are still evaluated so library checks report the
/// same constant-expression diagnostics as binary compilation. The program
/// must already have passed module resolution.
pub fn check_library(program: &Program) -> Result<(), Vec<Diagnostic>> {
    analyze(program, false).and_then(prepare).map(|_| ())
}

fn compile_target(program: &Program, require_entry_point: bool) -> Result<String, Vec<Diagnostic>> {
    let prepared = prepare(analyze(program, require_entry_point)?)?;
    Emitter::new(&prepared.hir, prepared.constants, &prepared.cleanup_plans)
        .emit_module(require_entry_point)
        .map_err(|error| vec![error])
}

fn analyze(
    program: &Program,
    require_entry_point: bool,
) -> Result<AnalyzedProgram, Vec<Diagnostic>> {
    let mut analyzer =
        Analyzer::try_new(program).map_err(|error| vec![Diagnostic::new(error.to_string())])?;
    let hir = analyzer.analyze_target(require_entry_point);
    if !analyzer.diagnostics.is_empty() {
        return Err(analyzer.diagnostics);
    }

    Ok(AnalyzedProgram {
        hir: hir.expect("analysis without diagnostics must produce HIR"),
    })
}

fn prepare(analyzed: AnalyzedProgram) -> Result<PreparedProgram, Vec<Diagnostic>> {
    let cleanup_plans = build_and_verify_cleanup_plans(&analyzed.hir)?;
    let constants = evaluate_globals(&analyzed.hir)?;
    Ok(PreparedProgram {
        hir: analyzed.hir,
        cleanup_plans,
        constants,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    #[test]
    fn check_and_compile_share_semantic_and_preparation_phases() {
        let program = parse("let value = 40 + 2\n").expect("parse library");
        check_library(&program).expect("check library");
        let ir = compile_library(&program).expect("compile library");
        assert!(ir.contains("@sali.global.76616c7565"));
    }
}
