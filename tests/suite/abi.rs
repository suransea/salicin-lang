use crate::support::*;

#[test]
fn tuple_types_literals_projection_patterns_and_cleanup_run_natively() {
    let fixtures = ["tuple_basics.sc", "tuple_resource_drop.sc"];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn tuple_diagnostics_are_source_level_and_specific() {
    let cases = [
        (
            "tuple_pattern_length_mismatch.sc",
            "tuple pattern length mismatch: expected 2, found 1",
        ),
        (
            "tuple_projection_out_of_bounds.sc",
            "tuple index 2 is out of bounds for tuple of length 2",
        ),
        (
            "tuple_projection_named.sc",
            "tuple projection requires a decimal index, found `left`",
        ),
    ];
    for (name, expected) in cases {
        let source = fs::read_to_string(fixture("fail", name)).expect("read tuple failure fixture");
        let diagnostics =
            check_source(&source).expect_err("tuple failure fixture unexpectedly passed");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "{name} did not contain `{expected}`: {diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.contains('$')),
            "{name} leaked an internal name: {diagnostics:?}"
        );
    }
}

#[test]
fn primitive_scalar_widths_and_boundaries_run_natively() {
    for (name, output) in
        batched_native_fixture_outputs(&["primitive_scalar_widths.sc", "numeric_utilities.sc"])
    {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn primitive_scalar_overflows_and_conversions_are_diagnosed() {
    let cases = [
        (
            "primitive_i8_literal_overflow.sc",
            "integer literal `128` does not fit in `i8`",
        ),
        (
            "primitive_u8_literal_overflow.sc",
            "integer literal `256` does not fit in `u8`",
        ),
        (
            "primitive_i128_positive_overflow.sc",
            "does not fit in `i128`",
        ),
        (
            "primitive_i128_negative_overflow.sc",
            "does not fit in `i128`",
        ),
        (
            "primitive_u128_literal_overflow.sc",
            "integer literal is too large",
        ),
        (
            "primitive_no_implicit_widening.sc",
            "type mismatch for argument for parameter `value`: expected `i16`, found `i8`",
        ),
        (
            "numeric_checked_into_non_integer.sc",
            "`checked_into` requires an integer `output` type",
        ),
    ];
    for (name, expected) in cases {
        let source =
            fs::read_to_string(fixture("fail", name)).expect("read scalar failure fixture");
        let diagnostics = check_source(&source).expect_err("scalar failure fixture passed");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "{name} did not contain `{expected}`: {diagnostics:?}"
        );
    }
}

#[test]
fn c_struct_representation_preserves_layout_and_rejects_invalid_fields() {
    for (name, output) in batched_native_fixture_outputs(&["c_struct_layout.sc"]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }

    for (name, expected) in [
        ("c_struct_bool_field.sc", "not valid in `struct(c)`"),
        ("c_struct_borrow_field.sc", "not valid in `struct(c)`"),
        ("c_struct_empty.sc", "cannot be empty"),
        ("c_struct_generic_invalid.sc", "not valid in `struct(c)`"),
        ("c_struct_salicin_field.sc", "not valid in `struct(c)`"),
        ("c_struct_zero_length_array.sc", "not valid in `struct(c)`"),
    ] {
        let source =
            fs::read_to_string(fixture("fail", name)).expect("read C struct failure fixture");
        let diagnostics = check_source(&source).expect_err("C struct failure fixture passed");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "{name} did not contain `{expected}`: {diagnostics:?}"
        );
    }
}

#[test]
fn c_struct_layout_matches_c_through_raw_pointers() {
    let source = r#"
let Inner = struct(c) {
  small: i16,
  wide: u64,
}

let Record = struct(c) {
  tag: u8,
  inner: Inner,
  huge: i128,
  values: array(u16)(3),
  next: ptr(u8),
}

let c_record_size(): u64 = foreign(c)
let c_record_align(): u64 = foreign(c)
let c_verify_record(record: ptr(Record)): i32 = foreign(c)
let c_fill_record(record: ptr(mut)(Record)): () = foreign(c)

let main(): i32 = {
  let byte: u8 = 31
  let mut record = Record { tag: 7, inner: Inner { small: -3, wide: 1000 }, huge: -4000, values: [11, 13, 17], next: ptr(borrow(byte)) }
  let verified = unsafe {
    c_record_size() == size_of(Record) &&
    c_record_align() == align_of(Record) &&
    c_verify_record(ptr(borrow(record))) == 42
  }
  do {
    unsafe {
      c_fill_record(ptr(mut)(borrow(mut)(record)))
    }
  }
  if verified &&
    record.tag == 9 &&
    record.inner.small == -5 &&
    record.inner.wide == 2000 &&
    record.huge == -8000 &&
    record.values[0] == 19 &&
    record.values[1] == 23 &&
    record.values[2] == 29 {
    42
  } else {
    0
  }
}
"#;
    let c_source = r#"
#include <stddef.h>
#include <stdint.h>

typedef struct {
  int16_t small;
  uint64_t wide;
} Inner;

typedef struct {
  uint8_t tag;
  Inner inner;
  __int128 huge;
  uint16_t values[3];
  const uint8_t *next;
} Record;

size_t c_record_size(void) {
  return sizeof(Record);
}

size_t c_record_align(void) {
  return _Alignof(Record);
}

int32_t c_verify_record(const Record *record) {
  return record->tag == 7 &&
         record->inner.small == -3 &&
         record->inner.wide == 1000 &&
         record->huge == -4000 &&
         record->values[0] == 11 &&
         record->values[1] == 13 &&
         record->values[2] == 17 &&
         *record->next == 31 ? 42 : 0;
}

void c_fill_record(Record *record) {
  record->tag = 9;
  record->inner.small = -5;
  record->inner.wide = 2000;
  record->huge = -8000;
  record->values[0] = 19;
  record->values[1] = 23;
  record->values[2] = 29;
}
"#;
    let ir = compile_source(source).expect("compile C layout interop program");
    let output = link_and_run_ir_with_c(&ir, c_source, "C aggregate pointer layout");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn c_ffi_scalars_and_raw_pointers_link_and_run_natively() {
    let fixtures = ["ffi_c_abs.sc", "ffi_c_memset.sc"];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }

    let source = fs::read_to_string(fixture("pass", "ffi_c_abs.sc")).expect("read FFI fixture");
    let ir = compile_source(&source).expect("compile FFI fixture");
    assert!(ir.contains("declare i32 @abs(i32)"));
    assert!(ir.contains("call i32 @abs(i32"));
    assert!(!ir.contains("define i32 @abs"));
}

#[test]
fn c_ffi_integer_widths_match_c_parameters_and_returns() {
    let source = r#"
let c_i8(): i8 = foreign(c)
let c_i16(): i16 = foreign(c)
let c_i32(): i32 = foreign(c)
let c_i64(): i64 = foreign(c)
let c_i128(): i128 = foreign(c)
let c_isize(): isize = foreign(c)
let c_u8(): u8 = foreign(c)
let c_u16(): u16 = foreign(c)
let c_u32(): u32 = foreign(c)
let c_u64(): u64 = foreign(c)
let c_u128(): u128 = foreign(c)
let c_usize(): usize = foreign(c)
let c_accept(
  a: i8,
  b: i16,
  c: i32,
  d: i64,
  e: i128,
  f: isize,
  g: u8,
  h: u16,
  i: u32,
  j: u64,
  k: u128,
  l: usize,
): i32 = foreign(c)

let main(): i32 = {
  unsafe {
    if c_i8() == -8 &&
      c_i16() == -16 &&
      c_i32() == -32 &&
      c_i64() == -64 &&
      c_i128() == -128 &&
      c_isize() == -42 &&
      c_u8() == 8 &&
      c_u16() == 16 &&
      c_u32() == 32 &&
      c_u64() == 64 &&
      c_u128() == 128 &&
      c_usize() == 42 &&
      c_accept(-8, -16, -32, -64, -128, -42, 8, 16, 32, 64, 128, 42) == 42 {
      42
    } else {
      0
    }
  }
}
"#;
    let c_source = r#"
#include <stdint.h>
#include <stddef.h>

int8_t c_i8(void) { return -8; }
int16_t c_i16(void) { return -16; }
int32_t c_i32(void) { return -32; }
int64_t c_i64(void) { return -64; }
__int128 c_i128(void) { return -128; }
intptr_t c_isize(void) { return -42; }
uint8_t c_u8(void) { return 8; }
uint16_t c_u16(void) { return 16; }
uint32_t c_u32(void) { return 32; }
uint64_t c_u64(void) { return 64; }
unsigned __int128 c_u128(void) { return 128; }
uintptr_t c_usize(void) { return 42; }

int32_t c_accept(
  int8_t a,
  int16_t b,
  int32_t c,
  int64_t d,
  __int128 e,
  intptr_t f,
  uint8_t g,
  uint16_t h,
  uint32_t i,
  uint64_t j,
  unsigned __int128 k,
  uintptr_t l
) {
  return a == -8 && b == -16 && c == -32 && d == -64 &&
         e == -128 && f == -42 && g == 8 && h == 16 &&
         i == 32 && j == 64 && k == 128 && l == 42 ? 42 : 0;
}
"#;
    let ir = compile_source(source).expect("compile C integer ABI program");
    let output = link_and_run_ir_with_c(&ir, c_source, "C integer ABI");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn package_qualified_exports_link_across_independent_llvm_modules() {
    fn package(name: &str, value: i32) -> SourcePackage {
        SourcePackage {
            id: PackageId(0),
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            identity: format!("{name}@1.0.0"),
            is_primary: true,
            dependencies: BTreeMap::new(),
            sources: vec![SourceUnit {
                path: format!("<{name}>"),
                module_path: Vec::new(),
                source: format!("pub let answer(): i32 = {{ {value} }}\n"),
                is_root: true,
            }],
        }
    }

    let alpha =
        compile_library_source_packages(&[package("alpha", 20)]).expect("compile alpha library");
    let beta =
        compile_library_source_packages(&[package("beta", 22)]).expect("compile beta library");
    let alpha_symbol = exported_function_symbols(&alpha).remove(0);
    let beta_symbol = exported_function_symbols(&beta).remove(0);
    assert_ne!(alpha_symbol, beta_symbol);

    let driver = format!(
        "declare i32 @{alpha_symbol}()\n\
         declare i32 @{beta_symbol}()\n\
         define i32 @main() {{\n\
         entry:\n\
           %alpha = call i32 @{alpha_symbol}()\n\
           %beta = call i32 @{beta_symbol}()\n\
           %answer = add i32 %alpha, %beta\n\
           ret i32 %answer\n\
         }}\n"
    );
    let temporary = TestDirectory::new();
    let alpha_path = temporary.write("alpha.ll", &alpha);
    let beta_path = temporary.write("beta.ll", &beta);
    let driver_path = temporary.write("driver.ll", &driver);
    let executable = temporary.join("linked");
    let linked = Command::new("/usr/bin/clang")
        .arg("-Wno-override-module")
        .arg("-x")
        .arg("ir")
        .arg(&alpha_path)
        .arg(&beta_path)
        .arg(&driver_path)
        .arg("-x")
        .arg("none")
        .arg(test_allocator_object())
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("link independent Salicin library modules");
    assert!(linked.status.success(), "{}", output_text(&linked));
    let output = Command::new(executable)
        .output()
        .expect("run independently linked Salicin libraries");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn export_contracts_are_stable_and_reject_incompatible_binding() {
    fn source_package(
        id: usize,
        name: &str,
        version: &str,
        primary: bool,
        dependencies: &[(&str, usize)],
        source: &str,
    ) -> SourcePackage {
        SourcePackage {
            id: PackageId(id),
            name: name.to_owned(),
            version: version.to_owned(),
            identity: format!("{name}@{version}"),
            is_primary: primary,
            dependencies: dependencies
                .iter()
                .map(|(alias, id)| ((*alias).to_owned(), PackageId(*id)))
                .collect(),
            sources: vec![SourceUnit {
                path: format!("<{name}>"),
                module_path: Vec::new(),
                source: source.to_owned(),
                is_root: true,
            }],
        }
    }

    fn graph(dependency_id: usize) -> Vec<SourcePackage> {
        vec![
            source_package(
                0,
                "consumer",
                "1.0.0",
                true,
                &[("dep", dependency_id)],
                "pub let echo(move value: dep.Token): dep.Token = { value }\n",
            ),
            source_package(
                dependency_id,
                "dependency",
                "2.0.0",
                false,
                &[],
                "pub let Token = struct { value: i32 }\n\
                 pub let dependency_only(): i32 = { 42 }\n",
            ),
        ]
    }

    let first = compile_library_source_packages(&graph(1)).expect("compile first dependency graph");
    let reordered =
        compile_library_source_packages(&graph(9)).expect("compile reordered dependency graph");
    let first_exports = exported_function_symbols(&first);
    let reordered_exports = exported_function_symbols(&reordered);
    assert_eq!(first_exports, reordered_exports);
    assert_eq!(first_exports.len(), 1, "dependency export leaked:\n{first}");

    let i32_ir = compile_library_source_packages(&[source_package(
        0,
        "signature",
        "1.0.0",
        true,
        &[],
        "pub let identity(move value: i32): i32 = { value }\n",
    )])
    .expect("compile i32 signature");
    let i64_ir = compile_library_source_packages(&[source_package(
        0,
        "signature",
        "1.0.0",
        true,
        &[],
        "pub let identity(move value: i64): i64 = { value }\n",
    )])
    .expect("compile i64 signature");
    assert_ne!(
        exported_function_symbols(&i32_ir),
        exported_function_symbols(&i64_ir),
        "incompatible declarations must not bind to one export"
    );

    let mut public_provider = source_package(
        0,
        "same",
        "1.0.0",
        true,
        &[],
        "pub let answer(): i32 = { 42 }\n",
    );
    public_provider.identity = "registry:public|same@1.0.0".to_owned();
    let mut private_provider = public_provider.clone();
    private_provider.identity = "registry:private|same@1.0.0".to_owned();
    let public_ir =
        compile_library_source_packages(&[public_provider]).expect("compile public provider");
    let private_ir =
        compile_library_source_packages(&[private_provider]).expect("compile private provider");
    assert_ne!(
        exported_function_symbols(&public_ir),
        exported_function_symbols(&private_ir),
        "provider source must participate in native identity"
    );
}

fn exported_function_symbols(ir: &str) -> Vec<String> {
    let exports = ir
        .lines()
        .filter_map(|line| {
            let (_, suffix) = line.split_once("define ")?;
            let (_, suffix) = suffix.split_once("@sali.export.")?;
            let (suffix, _) = suffix.split_once('(')?;
            Some(format!("sali.export.{suffix}"))
        })
        .collect::<Vec<_>>();
    assert!(!exports.is_empty(), "expected an exported function:\n{ir}");
    exports
}

#[test]
fn c_ffi_rejects_unsafe_calls_and_private_abi_types() {
    let cases = [
        (
            "ffi_unsafe_call.sc",
            "call to unsafe function `c_abs` requires an `unsafe` handler",
        ),
        (
            "ffi_borrow_parameter.sc",
            "has unsupported C ABI type `borrow i32`",
        ),
        (
            "ffi_bool_result.sc",
            "has unsupported C ABI result type `bool`",
        ),
        (
            "ffi_array_parameter.sc",
            "has unsupported C ABI type `array(i32)(2)`",
        ),
        (
            "ffi_c_struct_parameter.sc",
            "has unsupported C ABI type `pair`",
        ),
        (
            "ffi_c_struct_result.sc",
            "has unsupported C ABI result type `pair`",
        ),
        (
            "ffi_function_parameter.sc",
            "has unsupported C ABI type `(i32): i32`",
        ),
        ("ffi_unit_parameter.sc", "has unsupported C ABI type `()`"),
        (
            "ffi_curried.sc",
            "C ABI functions require exactly one runtime parameter group",
        ),
        ("ffi_unsupported_abi.sc", "unsupported foreign ABI `system`"),
        ("ffi_legacy_extern.sc", "grouped `extern` declarations"),
        ("ffi_legacy_attribute.sc", "`@` syntax is not supported"),
        (
            "ffi_duplicate_link_name.sc",
            "use the same link symbol `abs`",
        ),
        (
            "ffi_reserved_link_name.sc",
            "uses reserved link symbol `salicin_alloc`",
        ),
    ];
    for (name, expected) in cases {
        let source = fs::read_to_string(fixture("fail", name)).expect("read FFI failure fixture");
        let diagnostics = check_source(&source).expect_err("FFI failure fixture passed");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains(expected)),
            "{name} did not contain `{expected}`: {diagnostics:?}"
        );
    }
}

#[test]
fn raise_and_unwrap_operators_run_through_standard_and_custom_protocols() {
    let fixtures = [
        "raise_result.sc",
        "raise_custom.sc",
        "unwrap_option_result.sc",
        "unwrap_custom.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_loops_and_arrays_run_with_expected_result() {
    let fixtures = [
        "while_mutation.sc",
        "while_let.sc",
        "continue.sc",
        "do_while_continue.sc",
        "continue_cleanup.sc",
        "loop_break_value.sc",
        "fixed_array_index.sc",
        "array_index_assignment.sc",
        "array_constant_index_place.sc",
        "array_index_move_reinitialize.sc",
        "array_nested_constant_index_place.sc",
        "array_index_raw_pointer.sc",
        "array_non_copy_element.sc",
        "array_resource_drop.sc",
        "array_resource_nested_drop.sc",
        "array_resource_overwrite_drop.sc",
        "array_resource_temporary_index.sc",
        "dynamic_array_index.sc",
        "empty_array_typed.sc",
        "nested_loop_break.sc",
        "loop_move_then_break.sc",
        "for_iterator.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn defer_runs_lexical_actions_lifo_on_all_control_exits() {
    for (name, output) in batched_native_fixture_outputs(&["defer_control.sc", "defer_throw.sc"]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "defer_expression_position.sc"))
        .output()
        .expect("check rejected defer expression");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("only valid as a standalone statement"),
        "{}",
        output_text(&output)
    );
}
