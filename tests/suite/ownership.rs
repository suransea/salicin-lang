use crate::support::*;

#[test]
fn borrowed_utf8_str_preserves_validation_and_source_loans() {
    let valid = salic()
        .arg("run")
        .arg(fixture("pass", "string_utf8.sc"))
        .output()
        .expect("run borrowed UTF-8 str fixture");
    assert_eq!(valid.status.code(), Some(42), "{}", output_text(&valid));

    for (name, expected) in [
        (
            "str_view_local_escape.sc",
            "source is local or cannot be proven",
        ),
        ("str_view_write_conflict.sc", "already borrowed"),
        (
            "str_subview_local_escape.sc",
            "source is local or cannot be proven",
        ),
        ("str_subview_write_conflict.sc", "already borrowed"),
        ("raw_str_safe.sc", "requires an `unsafe` block"),
        ("raw_subview_safe.sc", "requires an `unsafe` block"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid borrowed UTF-8 str fixture");
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn owned_utf8_conversion_preserves_the_success_or_error_owner() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "string_owned_utf8.sc"))
        .output()
        .expect("run owned UTF-8 conversion fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let unsafe_parts = salic()
        .arg("check")
        .arg(fixture("fail", "string_raw_parts_safe.sc"))
        .output()
        .expect("check safe string raw-parts access");
    assert!(!unsafe_parts.status.success());
    assert!(
        String::from_utf8_lossy(&unsafe_parts.stderr).contains("requires an `unsafe` handler"),
        "{}",
        output_text(&unsafe_parts)
    );
}

#[test]
fn string_construction_and_mutation_preserve_utf8() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "string_mutation.sc"))
        .output()
        .expect("run string mutation fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn string_ranges_ordering_and_search_share_utf8_boundaries() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "string_search.sc"))
        .output()
        .expect("run string range and search fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn option_and_result_helpers_preserve_borrows_and_lazy_fallbacks() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "value_helpers.sc"))
        .output()
        .expect("run option and result helper fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn raw_pointer_read_and_write_run_with_expected_result() {
    let fixtures = [
        "raw_pointer_read.sc",
        "raw_pointer_write.sc",
        "raw_pointer_access_family.sc",
        "raw_pointer_projected_place.sc",
        "do_forwards_unsafe_color.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn raw_allocator_abi_allocates_aligned_storage_and_deallocates_it() {
    let fixtures = [
        "raw_allocator_i32.sc",
        "raw_allocator_inferred.sc",
        "raw_allocator_layout.sc",
        "raw_pointer_offset.sc",
        "raw_pointer_offset_shared.sc",
        "raw_pointer_offset_unit.sc",
        "raw_pointer_borrow.sc",
        "raw_pointer_methods.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn raw_pointer_intrinsic_errors_report_their_cause() {
    for (name, expected) in [
        ("raw_offset_safe.sc", "requires an `unsafe` block"),
        (
            "raw_offset_non_pointer.sc",
            "requires `ptr(t)` or `ptr(mut)(t)`",
        ),
        ("raw_trap_safe.sc", "requires an `unsafe` block"),
        (
            "raw_trap_arguments.sc",
            "expects one empty runtime argument group",
        ),
        ("raw_borrow_safe.sc", "requires an `unsafe` block"),
        (
            "raw_pointer_projected_place_safe.sc",
            "requires an `unsafe` block",
        ),
        (
            "raw_pointer_projected_place_shared_write.sc",
            "cannot assign through a shared borrow",
        ),
        (
            "raw_borrow_mut_immutable_pointer.sc",
            "requires a `ptr(mut)(t)`",
        ),
        ("raw_borrow_anchor_conflict.sc", "borrowed"),
        (
            "raw_borrow_mut_shared_anchor.sc",
            "requires a mutable borrow anchor",
        ),
        (
            "raw_pointer_mut_method_shared.sc",
            "unknown method `take` on `ptr(i32)`",
        ),
        (
            "raw_pointer_foreign_extension.sc",
            "inherent extension for `ptr` must be declared in the package that defines the type",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid raw pointer offset fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn target_layout_intrinsics_cover_globals_aggregates_and_generic_instances() {
    let fixtures = ["layout_intrinsics.sc", "layout_intrinsics_generic.sc"];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn composite_ctfe_globals_run_natively() {
    for (name, output) in batched_native_fixture_outputs(&["ctfe_composite_globals.sc"]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn alloc_box_owns_copy_and_resource_payloads() {
    let fixtures = [
        "box_i32.sc",
        "box_resource.sc",
        "box_drop_once.sc",
        "box_nested_and_unit.sc",
        "box_recursive_layout.sc",
        "box_read.sc",
        "box_into_inner_drop_once.sc",
        "box_raw_roundtrip_drop_once.sc",
        "box_replace_drop.sc",
        "box_borrow.sc",
        "forget_resource.sc",
        "forget_temporary_resource.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "box_resource_drop_trap.sc"))
        .output()
        .expect("run Box recursive drop fixture");
    assert!(
        !trapped.status.success(),
        "boxed resource destructor did not run: {}",
        output_text(&trapped)
    );

    for name in [
        "box_borrow_then_replace.sc",
        "box_mut_borrow_conflict.sc",
        "box_borrow_then_into_inner.sc",
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check Box pointee borrow conflict");
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("borrowed"),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn alloc_vec_owns_copy_and_resource_elements() {
    let successful = [
        "vec_copy.sc",
        "vec_unit.sc",
        "vec_resource.sc",
        "vec_borrow.sc",
        "vec_ordered_copy.sc",
        "vec_ordered_resource.sc",
        "vec_reorder_resource.sc",
        "index_protocol_containers.sc",
        "vec_index_resource_overwrite.sc",
        "vec_into_iterator.sc",
        "vec_into_iterator_break_cleanup.sc",
        "array_into_iterator.sc",
        "slice_iterator.sc",
        "slice_iterator_mut.sc",
        "slice_iterator_resource.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&successful) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let trapping = [
        "vec_read_out_of_bounds.sc",
        "vec_write_out_of_bounds.sc",
        "vec_replace_out_of_bounds.sc",
        "vec_swap_remove_out_of_bounds.sc",
        "vec_insert_out_of_bounds.sc",
        "vec_remove_out_of_bounds.sc",
        "vec_at_out_of_bounds.sc",
        "vec_at_access_mut_out_of_bounds.sc",
        "vec_index_out_of_bounds.sc",
        "vec_swap_left_out_of_bounds.sc",
        "vec_swap_right_out_of_bounds.sc",
        "vec_capacity_overflow.sc",
        "vec_reserve_overflow.sc",
        "vec_zst_resource_drop_trap.sc",
    ];
    for (name, output) in trapping_fixture_outputs_in_parallel(&trapping) {
        assert!(
            !output.status.success(),
            "{name} did not trap: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "vec_resource_use_after_push.sc"))
        .output()
        .expect("check use after resource vec push");
    assert!(
        !output.status.success(),
        "resource push unexpectedly copied"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("moved"),
        "{}",
        output_text(&output)
    );

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "vec_use_after_into_iterator.sc"))
        .output()
        .expect("check use after consuming vec iteration");
    assert!(
        !output.status.success(),
        "consumed vec unexpectedly remained usable"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("moved"),
        "{}",
        output_text(&output)
    );

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "vec_append_self_borrow.sc"))
        .output()
        .expect("check self append borrow conflict");
    assert!(
        !output.status.success(),
        "self append unexpectedly compiled"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already borrowed"),
        "{}",
        output_text(&output)
    );

    for name in ["vec_borrow_then_push.sc", "vec_mut_borrow_conflict.sc"] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check vec element borrow conflict");
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("borrowed"),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn core_string_literals_preserve_utf8_and_representation_privacy() {
    let mut outputs = batched_native_fixture_outputs(&["string_utf8.sc"]);
    let (name, output) = outputs.pop().expect("String fixture output");
    assert_eq!(
        output.status.code(),
        Some(42),
        "{name}: {}",
        output_text(&output)
    );

    let name = "string_private_fields.sc";
    let expected = "is private";
    let output = salic()
        .arg("check")
        .arg(fixture("fail", name))
        .output()
        .expect("check invalid String fixture");
    assert!(!output.status.success(), "{name} unexpectedly compiled");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{name} did not report `{expected}`:\n{}",
        output_text(&output)
    );
}

#[test]
fn slices_preserve_array_and_vec_borrow_safety() {
    for (name, output) in batched_native_fixture_outputs(&["slice_array.sc", "slice_vec.sc"]) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    for name in ["slice_out_of_bounds.sc", "slice_index_out_of_bounds.sc"] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", name))
            .output()
            .expect("run out-of-bounds Slice fixture");
        assert!(
            !trapped.status.success(),
            "{name} did not trap: {}",
            output_text(&trapped)
        );
    }

    for name in [
        "slice_array_mut_borrow_conflict.sc",
        "vec_slice_then_push.sc",
        "slice_local_escape.sc",
        "slice_bare_parameter.sc",
        "slice_struct_field.sc",
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid Slice borrow fixture");
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("borrow"),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn generic_inherent_extensions_infer_and_dispatch_concrete_instances() {
    let fixtures = [
        "generic_inherent_extend.sc",
        "generic_inherent_reordered.sc",
        "generic_inherent_resource.sc",
        "generic_inherent_existing_instance.sc",
        "generic_enum_inherent_extend.sc",
        "generic_inherent_internal_dispatch.sc",
        "generic_inherent_from_generic_function.sc",
        "generic_extend_generic_member.sc",
        "box_methods.sc",
        "box_method_context_inference.sc",
        "access_generic.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "generic_inherent_member_shadow.sc"))
        .output()
        .expect("reject a member compile parameter that shadows its extension parameter");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("redeclares outer compile-time parameter"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn parameter_modifier_generics_select_copy_and_move() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "passing_generic.sc"))
        .output()
        .expect("run passing-generic fixture");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "passing_copy_resource.sc"))
        .output()
        .expect("reject copy passing for a resource");
    assert!(!output.status.success(), "{}", output_text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not implement copyable"),
        "{}",
        output_text(&output)
    );

    for (name, expected) in [
        ("passing_move_copy_use_after.sc", "moved"),
        (
            "passing_invalid_argument.sc",
            "invalid parameter modifier argument",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid passing-generic fixture");
        assert!(!output.status.success(), "{name}: {}", output_text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn where_copy_bounds_validate_generic_bodies_and_concrete_calls() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "where_copy_bound.sc"))
        .output()
        .expect("run generic function with a copyable bound");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (name, expected) in [
        ("where_copy_unsatisfied.sc", "not satisfied"),
        ("where_unknown_trait.sc", "unknown trait"),
        ("where_duplicate_predicate.sc", "duplicate where predicate"),
        ("where_trait_arity.sc", "argument count mismatch"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid where predicate");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn where_trait_bounds_enable_abstract_method_dispatch() {
    let fixtures = ["where_method_dispatch.sc", "where_generic_trait_method.sc"];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "where_method_missing_bound.sc"))
        .output()
        .expect("reject unbounded abstract method dispatch");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown method"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn where_associated_equalities_enable_operator_dispatch() {
    let fixtures = [
        "where_operator_output.sc",
        "where_associated_method.sc",
        "where_gat_equality.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "where_associated_type_mismatch.sc"))
        .output()
        .expect("reject an unsatisfied associated type equality");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not satisfied"),
        "{}",
        output_text(&output)
    );

    let output = salic()
        .arg("check")
        .arg(fixture("fail", "where_unknown_associated_type.sc"))
        .output()
        .expect("reject an unknown associated type equality");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown associated type"),
        "{}",
        output_text(&output)
    );

    for (fixture_name, expected) in [
        ("where_gat_equality_mismatch.sc", "not satisfied"),
        (
            "where_gat_equality_group_mismatch.sc",
            "parameter-group shape",
        ),
        (
            "where_gat_equality_kind_mismatch.sc",
            "parameter-group shape",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", fixture_name))
            .output()
            .expect("reject an invalid generic associated type equality");
        assert!(!output.status.success(), "{}", output_text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{}",
            output_text(&output)
        );
    }
}

#[test]
fn constrained_generic_extensions_select_members_per_instance() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "constrained_generic_extend.sc"))
        .output()
        .expect("run constrained generic extension");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (name, expected) in [
        ("constrained_extend_method_unsatisfied.sc", "unknown method"),
        ("box_read_method_resource.sc", "unknown method"),
        ("box_write_method_resource.sc", "unknown method"),
        (
            "constrained_extend_function_unsatisfied.sc",
            "not satisfied",
        ),
        ("constrained_extend_unknown_trait.sc", "unknown trait"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("reject an unsatisfied constrained extension member");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name}: {}",
            output_text(&output)
        );
    }
}

#[test]
fn generic_inherent_extensions_resolve_across_file_modules() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "generic-extend-modules"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write(
        "src/main.sc",
        "let main(): i32 = {\n  let cell = api.cell.new(42)\n  cell.take()\n}\n",
    );
    project.write(
        "src/api.sc",
        "pub(package) let cell(comptime t: type) = struct { value: t }\n\
         extend(cell(t)) {\n\
           let new(move value: t): cell(t) = { cell { value: value } }\n\
           let take(move self)(): t = { self.value }\n\
         }\n",
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run package with a generic extension module");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn generic_trait_methods_dispatch_across_file_modules() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        r#"[package]
name = "generic-trait-method-modules"
version = "0.1.0"
edition = "2026"
"#,
    );
    project.write(
        "src/main.sc",
        "let main(): i32 = {\n  api.cell.new().choose(i32)(42)\n}\n",
    );
    project.write(
        "src/api.sc",
        "pub(package) let choose = trait {\n\
           let choose(comptime value_type: type)(self: borrow(self))(move value: value_type): value_type\n\
         }\n\
         pub(package) let cell = struct {}\n\
         extend(cell, choose) {\n\
           let choose(comptime result: type)(self: borrow(self))(move value: result): result = {\n\
             value\n\
           }\n\
         }\n\
         extend(cell) {\n\
           let new(): cell = { cell {} }\n\
         }\n",
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run package with a generic trait method");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn inherent_extensions_cannot_be_added_outside_the_defining_package() {
    let project = TestDirectory::new();
    project.write(
        "dep/salicin.toml",
        r#"[package]
name = "generic-cell"
version = "0.1.0"
edition = "2026"

[lib]
path = "src/lib.sc"
"#,
    );
    project.write(
        "dep/src/lib.sc",
        "pub let cell(comptime t: type) = struct { pub value: t }\n",
    );
    project.write(
        "app/salicin.toml",
        r#"[package]
name = "foreign-extend"
version = "0.1.0"
edition = "2026"

[dependencies]
dep = { path = "../dep" }
"#,
    );
    project.write(
        "app/src/main.sc",
        "extend(dep.cell(t)) {\n\
           let take(move self)(): t = { self.value }\n\
         }\n\
         let main(): i32 = { 0 }\n",
    );

    let output = salic()
        .arg("check")
        .arg(project.join("app"))
        .output()
        .expect("reject foreign inherent extension");
    assert!(
        !output.status.success(),
        "foreign extension unexpectedly passed"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("package that defines the type"),
        "{}",
        output_text(&output)
    );
}

#[test]
fn raw_allocator_runtime_rejects_an_invalid_layout() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "raw_allocator_invalid_alignment.sc"))
        .output()
        .expect("run invalid allocator layout fixture");
    assert!(
        !output.status.success(),
        "invalid allocator layout unexpectedly succeeded: {}",
        output_text(&output)
    );
}

#[test]
fn raw_allocator_abi_can_be_replaced_by_strong_link_symbols() {
    let directory = TestDirectory::new();
    let source = directory.write(
        "main.sc",
        "let main(): i32 = {\n  let pointer = unsafe { raw_alloc(i32)(4, 4) }\n  unsafe { *pointer = 42 }\n  unsafe { raw_dealloc(pointer, 4, 4) }\n  0\n}\n",
    );
    let ir = directory.join("main.ll");
    let executable = directory.join("main");
    let custom = directory.write(
        "custom.c",
        "#include <stdint.h>\n#include <stdlib.h>\n_Alignas(64) static unsigned char storage[64];\nvoid *salicin_alloc(uint64_t size, uint64_t align) { (void)size; (void)align; return storage; }\nvoid salicin_dealloc(void *pointer, uint64_t size, uint64_t align) { (void)pointer; (void)size; (void)align; _Exit(42); }\n",
    );
    let emitted = salic()
        .args(["emit-ir"])
        .arg(&source)
        .arg("-o")
        .arg(&ir)
        .output()
        .expect("emit allocator ABI IR");
    assert!(emitted.status.success(), "{}", output_text(&emitted));

    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/allocator.c");
    let linked = Command::new("/usr/bin/clang")
        .args(["-Wno-override-module", "-x", "ir"])
        .arg(&ir)
        .args(["-x", "c", "-std=c11"])
        .arg(&custom)
        .arg(&runtime)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("link replacement allocator");
    assert!(linked.status.success(), "{}", output_text(&linked));

    let status = Command::new(&executable)
        .status()
        .expect("run replacement allocator fixture");
    assert_eq!(status.code(), Some(42));
}

#[test]
fn vec_drop_releases_its_allocation_through_the_allocator_abi() {
    let directory = TestDirectory::new();
    let source = directory.write(
        "main.sc",
        "use alloc.vec.vec\n\nlet main(): i32 = {\n  let values: vec(i32) = vec(i32).new()\n  values.len()\n  0\n}\n",
    );
    let ir = directory.join("main.ll");
    let executable = directory.join("main");
    let custom = directory.write(
        "custom.c",
        "#include <stdint.h>\n#include <stdlib.h>\n_Alignas(64) static unsigned char storage[64];\nvoid *salicin_alloc(uint64_t size, uint64_t align) { (void)size; (void)align; return storage; }\nvoid salicin_dealloc(void *pointer, uint64_t size, uint64_t align) { (void)pointer; (void)size; (void)align; _Exit(42); }\n",
    );
    let emitted = salic()
        .args(["emit-ir"])
        .arg(&source)
        .arg("-o")
        .arg(&ir)
        .output()
        .expect("emit vec allocator ABI IR");
    assert!(emitted.status.success(), "{}", output_text(&emitted));

    let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/allocator.c");
    let linked = Command::new("/usr/bin/clang")
        .args(["-Wno-override-module", "-x", "ir"])
        .arg(&ir)
        .args(["-x", "c", "-std=c11"])
        .arg(&custom)
        .arg(&runtime)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("link vec replacement allocator");
    assert!(linked.status.success(), "{}", output_text(&linked));

    let status = Command::new(&executable)
        .status()
        .expect("run vec replacement allocator fixture");
    assert_eq!(status.code(), Some(42));
}

#[test]
fn m1_struct_programs_run_with_expected_result() {
    let fixtures = [
        "struct_fields.sc",
        "struct_match.sc",
        "struct_mutation.sc",
        "positional_constructor.sc",
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
fn type_constructor_aliases_run_and_report_kind_errors() {
    let fixtures = [
        "type_constructor_alias.sc",
        "type_constructor_labeled_arguments.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name}: {}",
            output_text(&output)
        );
    }

    for (name, expected) in [
        ("type_alias_cycle.sc", "cyclic type alias"),
        ("type_alias_arity.sc", "argument count mismatch"),
        (
            "type_constructor_unknown_label.sc",
            "unknown compile-time argument label `element`",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid type alias fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn type_constructor_aliases_cross_module_boundaries() {
    let project = TestDirectory::new();
    project.write(
        "salicin.toml",
        "[package]\nname = \"alias-modules\"\nversion = \"0.1.0\"\nedition = \"2026\"\n",
    );
    project.write(
        "src/types.sc",
        "pub(package) let cell(comptime t: type) = struct { pub(package) value: t }\n\
         pub(package) let family(comptime t: type): type = cell(t)\n\
         pub(package) let constructor: (comptime t: type): type = cell\n\
         pub(package) let scalar = i32\n",
    );
    project.write(
        "src/main.sc",
        "use types.{family, constructor, scalar}\n\n\
         let main(): scalar = {\n\
           let left: family(i32) = family(i32) { value: 40 }\n\
           let right = constructor(i32) { value: 2 }\n\
           left.value + right.value\n\
         }\n",
    );

    let output = salic()
        .arg("run")
        .arg(&project.0)
        .output()
        .expect("run project with imported type-constructor aliases");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn algebraic_effect_operations_check_their_instantiated_row() {
    let valid = salic()
        .arg("check")
        .arg(fixture("pass", "algebraic_effect_operations.sc"))
        .output()
        .expect("check algebraic effect operations");
    assert!(valid.status.success(), "{}", output_text(&valid));

    let invalid = salic()
        .arg("check")
        .arg(fixture("fail", "algebraic_effect_unhandled.sc"))
        .output()
        .expect("reject operation outside its effect row");
    assert!(!invalid.status.success());
    let stderr = String::from_utf8_lossy(&invalid.stderr);
    assert!(
        stderr.contains("call to `state(i32).get` requires custom effect `state(i32)`"),
        "{}",
        output_text(&invalid)
    );
    assert!(
        !stderr.contains("$effect$"),
        "operation diagnostics leaked an internal symbol:\n{}",
        output_text(&invalid)
    );
}

#[test]
fn m1_struct_errors_report_their_cause() {
    for (name, expected) in [
        ("unknown_field.sc", "unknown field"),
        ("constructor_missing_field.sc", "missing field"),
        ("constructor_duplicate_field.sc", "duplicate field"),
        ("constructor_mixed_arguments.sc", "mixed"),
        ("immutable_field_assignment.sc", "immutable"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 struct fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_match_and_partial_programs_run_with_expected_result() {
    let fixtures = [
        "enum_match.sc",
        "nested_match.sc",
        "match_guard.sc",
        "match_literal_payload.sc",
        "match_literal_resource_guard.sc",
        "match_scalar.sc",
        "match_scalar_single_evaluation.sc",
        "if_let.sc",
        "partial_application.sc",
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
fn m1_match_and_partial_errors_report_their_cause() {
    for (name, expected) in [
        ("non_exhaustive_match.sc", "exhaustive"),
        ("pattern_type_mismatch.sc", "pattern"),
        (
            "pattern_literal_payload_mismatch.sc",
            "pattern type mismatch",
        ),
        ("pattern_literal_payload_range.sc", "range"),
        ("match_scalar_constructor.sc", "cannot match scalar"),
        ("match_scalar_non_exhaustive.sc", "not exhaustive"),
        ("match_scalar_literal_range.sc", "range"),
        ("if_let_binding_scope.sc", "unknown"),
        ("temporary_borrow_partial.sc", "partial application"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 match or partial-application fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_ownership_programs_run_with_expected_result() {
    let fixtures = [
        "shared_borrow_call.sc",
        "mut_borrow_field_update.sc",
        "explicit_move_i32_once.sc",
        "borrow_released_after_complete_call.sc",
        "borrowed_unit_is_abi_erased.sc",
        "branch_move_does_not_pollute_sibling.sc",
        "disjoint_mut_field_borrows.sc",
        "inferred_copy_i32.sc",
        "move_then_return_preserves_other_branch.sc",
        "temporary_borrow_argument_order.sc",
        "temporary_mut_borrow_argument.sc",
        "temporary_borrow_argument_drop.sc",
        "temporary_borrow_method_argument.sc",
        "temporary_borrow_partial_call.sc",
        "explicit_borrow_types.sc",
        "region_scoped_borrow.sc",
        "returned_borrow.sc",
        "borrow_value_parameter.sc",
    ];
    for (name, output) in batched_native_fixture_outputs(&fixtures) {
        assert_eq!(
            output.status.code(),
            Some(42),
            "{name} failed:\n{}",
            output_text(&output)
        );
    }

    let unit_borrow = fs::read_to_string(fixture("pass", "borrowed_unit_is_abi_erased.sc"))
        .expect("read borrowed-unit ABI fixture");
    let ir = compile_source(&unit_borrow).expect("compile borrowed-unit ABI fixture");
    assert!(ir.contains("define internal void @sali.fn.6f627365727665()"));
    assert!(ir.contains("call void @sali.fn.6f627365727665()"));
    assert!(!ir.contains("@sali.fn.6f627365727665(ptr"));
}

#[test]
fn m1_ownership_errors_report_their_cause() {
    for (name, expected) in [
        ("use_after_move.sc", &["moved"][..]),
        ("use_after_explicit_move_i32.sc", &["moved"][..]),
        (
            "copy_non_copy.sc",
            &["requires `copyable`", "does not implement copyable"][..],
        ),
        (
            "double_mut_borrow.sc",
            &["mutable borrow", "already borrowed"][..],
        ),
        ("borrow_move_conflict.sc", &["move", "borrowed"][..]),
        (
            "same_field_mut_borrow_conflict.sc",
            &["mutable borrow", "already borrowed"][..],
        ),
        (
            "algebraic_effect_identical_field_borrows.sc",
            &["mutable borrow", "overlapping borrowed arguments"][..],
        ),
        (
            "algebraic_effect_parent_child_borrows.sc",
            &["mutable borrow", "overlapping borrowed arguments"][..],
        ),
        (
            "algebraic_effect_dynamic_index_alias.sc",
            &["mutable borrow", "overlapping borrowed arguments"][..],
        ),
        ("use_after_inferred_move.sc", &["moved"][..]),
        ("possibly_moved_after_branch.sc", &["possibly moved"][..]),
        ("both_branches_move.sc", &["moved"][..]),
        ("short_circuit_possibly_moves.sc", &["possibly moved"][..]),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 ownership fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        for fragment in expected {
            assert!(
                stderr.contains(fragment),
                "{name} did not report `{fragment}`:\n{}",
                output_text(&output)
            );
        }
        assert!(
            !stderr.contains("not supported"),
            "{name} reached a placeholder diagnostic:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn explicit_borrow_type_errors_report_their_cause() {
    for (name, expected) in [
        ("borrow_type_kind_mismatch.sc", "borrow kind mismatch"),
        (
            "borrow_type_non_borrow_initializer.sc",
            "borrow value of local",
        ),
        ("borrow_type_pointee_mismatch.sc", "borrow pointee"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid explicit borrow type fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn region_frontend_errors_report_their_cause() {
    for (name, expected) in [
        (
            "region_undeclared_parameter.sc",
            "undeclared access or region parameter `r`",
        ),
        (
            "region_undeclared_type.sc",
            "undeclared access or region parameter `r`",
        ),
        ("region_duplicate.sc", "duplicate region parameter `r`"),
        ("region_static_redeclared.sc", "predefined"),
        (
            "region_name_with_type_kind.sc",
            "expected a parameter name, found a region name",
        ),
        (
            "region_plain_name.sc",
            "standard-library item `region` is not in the prelude",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid region fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn returned_borrow_errors_report_their_cause() {
    for (name, expected) in [
        ("returned_borrow_local.sc", "borrow of a local value"),
        (
            "returned_borrow_temporary.sc",
            "cannot originate from a temporary",
        ),
        (
            "returned_borrow_shared_as_mut.sc",
            "shared borrow as a mutable borrow",
        ),
        ("returned_borrow_conflicting_write.sc", "already borrowed"),
        (
            "returned_borrow_missing_region.sc",
            "cannot infer the returned borrow region",
        ),
        (
            "returned_borrow_method.sc",
            "cannot originate from a temporary",
        ),
        ("returned_borrow_method_conflict.sc", "already borrowed"),
        ("returned_borrow_method_local.sc", "borrowing a local value"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid returned borrow fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn borrow_value_parameter_errors_report_their_cause() {
    for (name, expected) in [
        ("borrow_value_mut_moved.sc", "moved"),
        ("borrow_value_explicit_move.sc", "moved"),
        ("borrow_value_copy_mut.sc", "requires `copyable`"),
        ("borrow_value_block_escape_conflict.sc", "already borrowed"),
        ("borrow_value_partial.sc", "partial application"),
        (
            "borrow_value_local_escape.sc",
            "source must be a region-bound borrow parameter",
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid borrow value parameter fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn v09_reinitialization_programs_run_with_expected_result() {
    let fixtures = [
        "reinit_after_root_move.sc",
        "reinit_partial_field.sc",
        "reinit_root_move_field_by_field.sc",
        "reinit_after_both_if_branches.sc",
        "reinit_loop_backedge.sc",
        "reinit_after_explicit_copy_move.sc",
        "match_guard_copy_binding.sc",
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
fn v09_reinitialization_errors_preserve_flow_safety() {
    for (name, expected) in [
        (
            "reinit_only_one_if_branch.sc",
            &["possibly", "uninitialized"][..],
        ),
        (
            "move_only_one_if_branch.sc",
            &["possibly", "uninitialized"][..],
        ),
        (
            "reinit_root_move_incomplete_fields.sc",
            &["uninitialized"][..],
        ),
        ("reinit_self_assignment_after_move.sc", &["moved"][..]),
        (
            "match_guard_move_non_copy_binding.sc",
            &["guard", "move"][..],
        ),
        (
            "reinit_widening_many_independent_branches.sc",
            &["possibly", "uninitialized"][..],
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid v0.9 reinitialization fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        for fragment in expected {
            assert!(
                stderr.contains(fragment),
                "{name} did not report `{fragment}`:\n{}",
                output_text(&output)
            );
        }
    }
}

#[test]
fn source_backed_copy_programs_run_with_expected_result() {
    let fixtures = [
        "copy_nominal_repeated_and_parameters.sc",
        "copy_nominal_capture.sc",
        "copy_nominal_enum_array.sc",
        "copy_generic_blanket.sc",
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
fn source_backed_drop_glue_links_and_runs() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_glue.sc"))
        .output()
        .expect("run source-backed droppable program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn drop_runs_on_structured_scope_exits_without_double_drop() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_scope.sc"))
        .output()
        .expect("run structured droppable program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_trap.sc"))
        .output()
        .expect("run observable droppable trap");
    assert!(
        !trapped.status.success(),
        "droppable was not executed:\n{}",
        output_text(&trapped)
    );

    let generic_trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_generic_blanket_trap.sc"))
        .output()
        .expect("run blanket generic droppable trap");
    assert!(
        !generic_trapped.status.success(),
        "blanket generic droppable was not executed:\n{}",
        output_text(&generic_trapped)
    );

    let partial_exit = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_exit.sc"))
        .output()
        .expect("run partial-construction cleanup trap");
    assert!(
        !partial_exit.status.success(),
        "an owned constructor field leaked across return:\n{}",
        output_text(&partial_exit)
    );
}

#[test]
fn projection_drop_flags_preserve_unmoved_fields_and_rebuild_roots() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_field.sc"))
        .output()
        .expect("run projection drop-flag program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_field_trap.sc"))
        .output()
        .expect("run unmoved-field cleanup trap");
    assert!(
        !trapped.status.success(),
        "the unmoved sibling field was not dropped:\n{}",
        output_text(&trapped)
    );
}

#[test]
fn match_payload_moves_transfer_drop_ownership() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_payload.sc"))
        .output()
        .expect("run match payload drop program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_payload_trap.sc"))
        .output()
        .expect("run unmatched payload sibling cleanup trap");
    assert!(
        !trapped.status.success(),
        "the unmatched payload sibling was not dropped:\n{}",
        output_text(&trapped)
    );

    let nested = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_nested.sc"))
        .output()
        .expect("run nested match payload drop program");
    assert_eq!(nested.status.code(), Some(42), "{}", output_text(&nested));

    let nested_trap = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_nested_trap.sc"))
        .output()
        .expect("run nested match sibling cleanup trap");
    assert!(
        !nested_trap.status.success(),
        "the nested unmatched sibling was not dropped:\n{}",
        output_text(&nested_trap)
    );
}

#[test]
fn guarded_match_payload_moves_commit_only_after_success() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_guarded.sc"))
        .output()
        .expect("run guarded match payload program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    let trapped = salic()
        .arg("run")
        .arg(fixture("pass", "drop_match_guarded_trap.sc"))
        .output()
        .expect("run guarded match rollback sibling trap");
    assert!(
        !trapped.status.success(),
        "guard rollback lost the unmatched sibling:\n{}",
        output_text(&trapped)
    );
}

#[test]
fn fn_once_resource_captures_drop_exactly_once() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_closure_once.sc"))
        .output()
        .expect("run resource-owning fn_once closure");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (fixture_name, failure) in [
        (
            "drop_closure_abandon_trap.sc",
            "an abandoned closure environment was not dropped",
        ),
        (
            "drop_closure_early_trap.sc",
            "a capture staged before an early argument return was not dropped",
        ),
    ] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run closure capture cleanup trap");
        assert!(
            !trapped.status.success(),
            "{failure}:\n{}",
            output_text(&trapped)
        );
    }
}

#[test]
fn resource_partial_applications_transfer_and_drop_captures() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_partial_application.sc"))
        .output()
        .expect("run resource-owning partial applications");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (fixture_name, failure) in [
        (
            "drop_partial_application_abandon_trap.sc",
            "an abandoned partial capture was not dropped",
        ),
        (
            "drop_partial_application_early_trap.sc",
            "a partial capture staged before early return was not dropped",
        ),
    ] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run partial capture cleanup trap");
        assert!(
            !trapped.status.success(),
            "{failure}:\n{}",
            output_text(&trapped)
        );
    }
}

#[test]
fn closure_partial_applications_drop_captures_on_abandonment_and_early_exit() {
    for (fixture_name, failure) in [
        (
            "closure_partial_abandon_trap.sc",
            "an abandoned closure partial capture was not dropped",
        ),
        (
            "closure_partial_early_trap.sc",
            "a closure partial capture survived an early argument return",
        ),
    ] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run closure partial cleanup trap");
        assert!(
            !trapped.status.success(),
            "{failure}:\n{}",
            output_text(&trapped)
        );
    }
}

#[test]
fn callable_aliases_move_named_partial_closure_and_resource_environments() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "callable_alias.sc"))
        .output()
        .expect("run callable alias program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));
}

#[test]
fn concrete_partial_environments_return_across_function_boundaries() {
    for fixture_name in [
        "callable_return.sc",
        "callable_resource_return.sc",
        "closure_resource_return.sc",
    ] {
        let output = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run returned callable environment");
        assert_eq!(
            output.status.code(),
            Some(42),
            "{fixture_name} failed:\n{}",
            output_text(&output)
        );
    }

    let abandoned = salic()
        .arg("run")
        .arg(fixture("pass", "callable_resource_return_abandon_trap.sc"))
        .output()
        .expect("run abandoned returned callable environment");
    assert!(
        !abandoned.status.success(),
        "returned resource environment was not dropped:\n{}",
        output_text(&abandoned)
    );
}

#[test]
fn mutable_borrow_overwrite_drops_the_replaced_value() {
    let output = salic()
        .arg("run")
        .arg(fixture("pass", "drop_mut_borrow_overwrite.sc"))
        .output()
        .expect("run mutable-borrow overwrite program");
    assert_eq!(output.status.code(), Some(42), "{}", output_text(&output));

    for (fixture_name, failure) in [
        (
            "drop_mut_borrow_root_trap.sc",
            "root overwrite did not drop the old referent",
        ),
        (
            "drop_mut_borrow_field_trap.sc",
            "field overwrite did not drop the old referent field",
        ),
    ] {
        let trapped = salic()
            .arg("run")
            .arg(fixture("pass", fixture_name))
            .output()
            .expect("run mutable-borrow overwrite trap");
        assert!(
            !trapped.status.success(),
            "{failure}:\n{}",
            output_text(&trapped)
        );
    }
}

#[test]
fn source_backed_copy_errors_report_their_cause() {
    for (name, expected) in [
        (
            "copy_non_copy.sc",
            &["requires `copyable`", "does not implement copyable"][..],
        ),
        (
            "copy_nominal_invalid_struct_impl.sc",
            &["container", "cannot implement `copyable`", "payload"][..],
        ),
        (
            "copy_nominal_invalid_enum_impl.sc",
            &["message", "cannot implement `copyable`", "payload"][..],
        ),
        (
            "copy_nominal_transitive_invalid_impl.sc",
            &["branch", "tree", "cannot implement `copyable`"][..],
        ),
        ("copy_nominal_explicit_move_reuse.sc", &["moved"][..]),
        (
            "copy_nominal_concrete_generic_impl.sc",
            &[
                "function `read`",
                "requires `copyable`",
                "cell(i64)",
                "does not implement copyable",
            ][..],
        ),
        (
            "copy_generic_blanket_unproven.sc",
            &["blanket `copyable`", "not structurally valid"][..],
        ),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid source-backed copyable fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        for fragment in expected {
            assert!(
                stderr.contains(fragment),
                "{name} did not report `{fragment}`:\n{}",
                output_text(&output)
            );
        }
        assert!(
            !stderr.contains("$mono$type$"),
            "{name} leaked an internal monomorphization name:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn m1_local_closure_programs_run_with_expected_result() {
    let fixtures = [
        "capturing_closure.sc",
        "closure_shared_repeat.sc",
        "closure_capture_parameter.sc",
        "closure_curried_capture.sc",
        "closure_partial_application.sc",
        "closure_partial_multistage.sc",
        "closure_partial_fnmut.sc",
        "closure_partial_fnonce.sc",
        "closure_partial_move_argument.sc",
        "closure_partial_effect.sc",
        "closure_partial_resource_multistage.sc",
        "pattern_partial_attempt.sc",
        "pattern_partial_guard_miss.sc",
        "pattern_partial_pass.sc",
        "pattern_partial_fnonce.sc",
        "pattern_partial_effect.sc",
        "closure_mut_capture.sc",
        "closure_move_once.sc",
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
fn m1_local_closure_errors_report_their_cause() {
    for (name, expected) in [
        ("closure_escape_return.sc", "escape"),
        ("closure_fnmut_immutable.sc", "fn_mut"),
        (
            "closure_partial_fnmut_immutable.sc",
            "fn_mut partial application",
        ),
        ("closure_capture_borrow_conflict.sc", "borrowed"),
        ("closure_fnonce_twice.sc", "consumed"),
        ("closure_partial_fnonce_twice.sc", "consumed"),
        (
            "pattern_partial_missing_context.sc",
            "requires a function type annotation",
        ),
        ("pattern_partial_fnonce_twice.sc", "consumed"),
        ("closure_move_capture_source_use.sc", "moved"),
    ] {
        let output = salic()
            .arg("check")
            .arg(fixture("fail", name))
            .output()
            .expect("check invalid M1 closure fixture");
        assert!(!output.status.success(), "{name} unexpectedly passed");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(expected),
            "{name} did not report `{expected}`:\n{}",
            output_text(&output)
        );
    }
}

#[test]
fn shorthand_builds_a_native_executable() {
    let temporary = TestDirectory::new();
    let executable = temporary.join("mutation");
    let built = salic()
        .arg(fixture("pass", "block_mutation.sc"))
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("build source");
    assert!(built.status.success(), "{}", output_text(&built));
    assert!(executable.is_file());

    let status = Command::new(executable)
        .status()
        .expect("run native executable");
    assert_eq!(status.code(), Some(42));
}

#[test]
fn source_errors_fail_check_without_creating_output() {
    for (path, checked) in check_sources_in_parallel(fixture_paths("fail")) {
        let name = path.file_name().unwrap().to_string_lossy();
        let diagnostics = match checked {
            Ok(()) => panic!("{name} unexpectedly passed source checking"),
            Err(diagnostics) => diagnostics,
        };
        assert!(
            !diagnostics.is_empty(),
            "{name} produced no diagnostic output"
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.contains('$')),
            "{name} leaked an internal compiler name: {diagnostics:?}"
        );
    }
}

#[test]
fn every_pass_fixture_checks_successfully() {
    check_passing_fixture_corpus().unwrap_or_else(|diagnostics| {
        panic!("passing fixture corpus failed:\n{}", diagnostics.join("\n"))
    });
}
