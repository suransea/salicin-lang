//! Top-level semantic and backend phase orchestration.
//!
//! The phase wrappers are deliberately private. They make the ordering
//! contract explicit without exposing the current HIR or cleanup-plan shapes
//! as a public compiler API.

use std::collections::HashMap;

use crate::ast::{Program, StructRepresentation};
use crate::cleanup::CleanupPlan;

use super::cleanup_plan::build_and_verify_cleanup_plans;
use super::emitter::{evaluate_globals, ConstValue, Emitter};
use super::hir::Ty;
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

/// Type-check a binary target, including its required `main` entry point,
/// without emitting LLVM IR. Cleanup plans and global constants are still
/// prepared so `check` reports the same frontend diagnostics as compilation.
pub fn check(program: &Program) -> Result<(), Vec<Diagnostic>> {
    analyze(program, true).and_then(prepare).map(|_| ())
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
    let hir = hir.expect("analysis without diagnostics must produce HIR");
    validate_sized_value_positions(&hir)?;

    Ok(AnalyzedProgram { hir })
}

fn validate_sized_value_positions(program: &HirProgram) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();
    let struct_representations = program
        .structs
        .iter()
        .map(|layout| (layout.name.as_str(), layout.representation))
        .collect::<HashMap<_, _>>();
    for layout in &program.structs {
        if layout.representation == StructRepresentation::C && layout.fields.is_empty() {
            diagnostics.push(Diagnostic::new(format!(
                "C representation struct `{}` cannot be empty",
                layout.source_name
            )));
        }
        for field in &layout.fields {
            if !field.ty.is_sized_value() {
                diagnostics.push(Diagnostic::new(format!(
                    "field `{}.{}` has unsized type `{}`; store a borrow or pointer instead",
                    layout.name, field.name, field.ty
                )));
            }
            if layout.representation == StructRepresentation::C
                && !c_field_type_is_valid(&field.ty, &struct_representations)
            {
                diagnostics.push(Diagnostic::new(format!(
                    "field `{}.{}` has type `{}`, which is not valid in `struct(c)`",
                    layout.source_name, field.name, field.ty
                )));
            }
        }
    }
    for layout in &program.enums {
        for variant in &layout.variants {
            for field in &variant.fields {
                if !field.ty.is_sized_value() {
                    diagnostics.push(Diagnostic::new(format!(
                        "field `{}.{}.{}` has unsized type `{}`; store a borrow or pointer instead",
                        layout.name, variant.name, field.name, field.ty
                    )));
                }
            }
        }
    }
    for global in &program.globals {
        if !global.ty.is_sized_value() {
            diagnostics.push(Diagnostic::new(format!(
                "global `{}` has unsized type `{}`; store a borrow or pointer instead",
                global.name, global.ty
            )));
        }
    }
    for function in &program.functions {
        for parameter in &function.params {
            if !parameter.ty.is_sized_value() {
                diagnostics.push(Diagnostic::new(format!(
                    "parameter `{}` of `{}` has unsized type `{}`; pass a borrow or pointer instead",
                    parameter.name, function.name, parameter.ty
                )));
            }
        }
        if !function.result.is_sized_value() {
            diagnostics.push(Diagnostic::new(format!(
                "function `{}` returns unsized type `{}`; return a borrow or pointer instead",
                function.name, function.result
            )));
        }
    }
    for function in &program.foreign_functions {
        for (index, parameter) in function.params.iter().enumerate() {
            if !parameter.is_sized_value() {
                diagnostics.push(Diagnostic::new(format!(
                    "foreign parameter {} of `{}` has unsized type `{parameter}`",
                    index + 1,
                    function.name
                )));
            }
        }
        if !function.result.is_sized_value() {
            diagnostics.push(Diagnostic::new(format!(
                "foreign function `{}` returns unsized type `{}`",
                function.name, function.result
            )));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

fn c_field_type_is_valid(
    ty: &Ty,
    struct_representations: &HashMap<&str, StructRepresentation>,
) -> bool {
    match ty {
        ty if ty.is_integer() => true,
        Ty::Pointer { .. } => true,
        Ty::Array(element, length) => {
            *length != 0 && c_field_type_is_valid(element, struct_representations)
        }
        Ty::Struct(name) => {
            struct_representations.get(name.as_str()) == Some(&StructRepresentation::C)
        }
        _ => false,
    }
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

    #[test]
    fn binary_check_validates_the_entry_point_without_emitting_ir() {
        let valid = parse("let main(): i32 = { 42 }\n").expect("parse binary");
        check(&valid).expect("check binary");

        let library = parse("let answer(): i32 = { 42 }\n").expect("parse library");
        let diagnostics = check(&library).expect_err("binary check requires main");
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("main")));
        check_library(&library).expect("library check does not require main");
    }
}
