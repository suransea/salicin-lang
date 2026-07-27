use super::*;

fn messages(plan: &CleanupPlan) -> Vec<String> {
    plan.verify()
        .expect_err("plan should be rejected")
        .into_iter()
        .map(|error| error.message().to_owned())
        .collect()
}

#[test]
fn accepts_nested_scopes_and_complete_return_chain() {
    let mut builder = CleanupPlanBuilder::new();
    let root = builder.root_scope();
    let function = builder.new_scope(root, ScopeKind::FunctionBody).unwrap();
    let nested = builder.new_scope(function, ScopeKind::Lexical).unwrap();
    let owned = builder
        .new_local(nested, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let borrowed = builder
        .new_local(
            nested,
            LocalKind::Temporary,
            LocalOwnership::SharedBorrow,
            false,
        )
        .unwrap();
    let owned_path = builder.new_move_path(Place::local(owned), None).unwrap();
    let body = builder.new_block(nested).unwrap();

    builder
        .set_terminator(
            builder.entry_block(),
            Terminator::Goto(CleanupEdge::new(body, Vec::new())),
        )
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(owned),
        CleanupOp::Init(owned_path),
        CleanupOp::MoveOut(owned_path),
        CleanupOp::StorageDead(owned),
        CleanupOp::StorageLive(borrowed),
    ] {
        builder.push_operation(body, operation).unwrap();
    }
    builder
        .set_terminator(
            body,
            Terminator::Return {
                exited_scopes: vec![nested, function],
            },
        )
        .unwrap();

    let plan = builder.finish().expect("nested cleanup plan should verify");
    assert_eq!(plan.root_scope, root);
    assert_eq!(plan.blocks.len(), 2);
}

#[test]
fn rejects_edge_that_skips_an_intermediate_scope() {
    let mut builder = CleanupPlanBuilder::new();
    let root = builder.root_scope();
    let outer = builder.new_scope(root, ScopeKind::Lexical).unwrap();
    let inner = builder.new_scope(outer, ScopeKind::Lexical).unwrap();
    let inner_block = builder.new_block(inner).unwrap();
    let root_block = builder.new_block(root).unwrap();
    builder
        .set_terminator(builder.entry_block(), Terminator::Unreachable)
        .unwrap();
    builder
        .set_terminator(
            inner_block,
            Terminator::Goto(CleanupEdge::new(root_block, vec![inner])),
        )
        .unwrap();
    builder
        .set_terminator(
            root_block,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder.into_unverified();
    assert!(messages(&plan)
        .iter()
        .any(|message| message.contains("non-contiguous scope exit chain")));
}

#[test]
fn rejects_owned_cleanup_for_borrow_alias() {
    let mut builder = CleanupPlanBuilder::new();
    let borrowed = builder
        .new_local(
            builder.root_scope(),
            LocalKind::Argument,
            LocalOwnership::MutableBorrow,
            true,
        )
        .unwrap();
    let borrowed_path = builder.new_move_path(Place::local(borrowed), None).unwrap();
    let entry = builder.entry_block();
    builder
        .push_operation(entry, CleanupOp::MoveOut(borrowed_path))
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::StorageDead(borrowed))
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder.into_unverified();
    let errors = messages(&plan);
    assert!(
        errors
            .iter()
            .filter(|message| message.contains("borrow alias"))
            .count()
            >= 3
    );
}

#[test]
fn rejects_a_borrow_alias_return_place_without_panicking() {
    let mut builder = CleanupPlanBuilder::new();
    builder
        .new_local(
            builder.root_scope(),
            LocalKind::ReturnPlace,
            LocalOwnership::SharedBorrow,
            false,
        )
        .unwrap();
    builder
        .set_terminator(
            builder.entry_block(),
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| message.contains("return place") && message.contains("owned")));
}

#[test]
fn rejects_bad_edge_target() {
    let mut builder = CleanupPlanBuilder::new();
    builder
        .set_terminator(
            builder.entry_block(),
            Terminator::Goto(CleanupEdge::new(BasicBlockId(99), Vec::new())),
        )
        .unwrap();

    let plan = builder.into_unverified();
    assert!(messages(&plan)
        .iter()
        .any(|message| message.contains("invalid target")));
}

#[test]
fn rejects_a_borrow_alias_as_branch_condition_storage() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let condition = builder
        .new_local(
            scope,
            LocalKind::Argument,
            LocalOwnership::SharedBorrow,
            false,
        )
        .unwrap();
    let then_block = builder.new_block(scope).unwrap();
    let else_block = builder.new_block(scope).unwrap();
    builder
        .set_terminator(
            builder.entry_block(),
            Terminator::Branch {
                condition,
                then_edge: CleanupEdge::new(then_block, vec![]),
                else_edge: CleanupEdge::new(else_block, vec![]),
            },
        )
        .unwrap();
    for block in [then_block, else_block] {
        builder
            .set_terminator(
                block,
                Terminator::Return {
                    exited_scopes: vec![],
                },
            )
            .unwrap();
    }

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| message.contains("owned condition storage")));
}

#[test]
fn rejects_scope_parent_cycle() {
    let mut builder = CleanupPlanBuilder::new();
    let root = builder.root_scope();
    let first = builder.new_scope(root, ScopeKind::Lexical).unwrap();
    let second = builder.new_scope(first, ScopeKind::Temporary).unwrap();
    builder
        .set_terminator(
            builder.entry_block(),
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();
    let mut plan = builder.into_unverified();
    plan.scopes[first.index()].parent = Some(second);

    assert!(messages(&plan)
        .iter()
        .any(|message| message.contains("scope parent cycle")));
}

#[test]
fn rejects_incomplete_return_scope_chain() {
    let mut builder = CleanupPlanBuilder::new();
    let root = builder.root_scope();
    let function = builder.new_scope(root, ScopeKind::FunctionBody).unwrap();
    let nested = builder.new_scope(function, ScopeKind::Lexical).unwrap();
    let body = builder.new_block(nested).unwrap();
    builder
        .set_terminator(builder.entry_block(), Terminator::Unreachable)
        .unwrap();
    builder
        .set_terminator(
            body,
            Terminator::Return {
                exited_scopes: vec![nested],
            },
        )
        .unwrap();

    let plan = builder.into_unverified();
    assert!(messages(&plan)
        .iter()
        .any(|message| message.contains("return") && message.contains("expected")));
}

#[test]
fn rejects_init_whose_place_local_does_not_exist() {
    let mut builder = CleanupPlanBuilder::new();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::Temporary,
            LocalOwnership::Owned,
            false,
        )
        .unwrap();
    let path = builder.new_move_path(Place::local(local), None).unwrap();
    let entry = builder.entry_block();
    builder
        .push_operation(entry, CleanupOp::Init(path))
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();
    let mut plan = builder.into_unverified();
    plan.move_paths[path.index()].place.local = LocalId(99);

    assert!(messages(&plan)
        .iter()
        .any(|message| message.contains("invalid place local")));
}

#[test]
fn rejects_missing_terminator_and_id_index_mismatch() {
    let builder = CleanupPlanBuilder::new();
    let mut plan = builder.into_unverified();
    plan.blocks[0].id = BasicBlockId(7);

    let errors = messages(&plan);
    assert!(errors
        .iter()
        .any(|message| message.contains("id/index mismatch")));
    assert!(errors
        .iter()
        .any(|message| message.contains("has no terminator")));
}

#[test]
fn rejects_overlapping_transfer_paths() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let root = builder.new_move_path(Place::local(local), None).unwrap();
    let field = builder
        .new_move_path(
            Place::local(local).project(Projection::Field(0)),
            Some(root),
        )
        .unwrap();
    builder
        .push_operation(
            entry,
            CleanupOp::Transfer {
                source: field,
                destination: root,
                kind: TransferKind::Initialize,
            },
        )
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| message.contains("overlapping source and destination")));
}

#[test]
fn rejects_a_double_move_on_a_reachable_path() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            false,
        )
        .unwrap();
    let path = builder.new_move_path(Place::local(local), None).unwrap();
    builder
        .push_operation(entry, CleanupOp::Init(path))
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::MoveOut(path))
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::MoveOut(path))
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| message.contains("MoveOut")
            && message.contains("requires an initialized source")));
}

#[test]
fn rejects_consumption_after_storage_dead() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let path = builder.new_move_path(Place::local(local), None).unwrap();
    for operation in [
        CleanupOp::Init(path),
        CleanupOp::StorageDead(local),
        CleanupOp::MoveOut(path),
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| { message.contains("MoveOut") && message.contains("Uninitialized") }));
}

#[test]
fn rejects_a_partially_initialized_return_place() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let return_place = builder
        .new_local(
            builder.root_scope(),
            LocalKind::ReturnPlace,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let root = builder
        .new_move_path(Place::local(return_place), None)
        .unwrap();
    let left = builder
        .new_move_path(
            Place::local(return_place).project(Projection::Field(0)),
            Some(root),
        )
        .unwrap();
    builder
        .new_move_path(
            Place::local(return_place).project(Projection::Field(1)),
            Some(root),
        )
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::Init(left))
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified()).iter().any(|message| {
        message.contains("requires initialized return place") && message.contains("MaybeOrPartial")
    }));
}

#[test]
fn rejects_a_transfer_from_an_uninitialized_source() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let source = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            false,
        )
        .unwrap();
    let destination = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let source_path = builder.new_move_path(Place::local(source), None).unwrap();
    let destination_path = builder
        .new_move_path(Place::local(destination), None)
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::Init(source_path))
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::MoveOut(source_path))
        .unwrap();
    builder
        .push_operation(
            entry,
            CleanupOp::Transfer {
                source: source_path,
                destination: destination_path,
                kind: TransferKind::Initialize,
            },
        )
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let errors = messages(&builder.into_unverified());
    assert!(errors.iter().any(|message| message.contains("Transfer")
        && message.contains("requires an initialized source")));
}

#[test]
fn rejects_a_transfer_between_incompatible_enum_forests() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let source = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let destination = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let source_root = builder.new_move_path(Place::local(source), None).unwrap();
    let source_variant = builder
        .new_move_path(
            Place::local(source).project(Projection::Downcast(0)),
            Some(source_root),
        )
        .unwrap();
    builder
        .new_move_path(
            Place::local(source)
                .project(Projection::Downcast(0))
                .project(Projection::Capture(0)),
            Some(source_variant),
        )
        .unwrap();
    let destination_root = builder
        .new_move_path(Place::local(destination), None)
        .unwrap();
    let destination_variant = builder
        .new_move_path(
            Place::local(destination).project(Projection::Downcast(1)),
            Some(destination_root),
        )
        .unwrap();
    builder
        .new_move_path(
            Place::local(destination)
                .project(Projection::Downcast(1))
                .project(Projection::Capture(0)),
            Some(destination_variant),
        )
        .unwrap();
    for operation in [
        CleanupOp::SetDiscriminant {
            destination: source_root,
            variant: 0,
        },
        CleanupOp::Init(source_variant),
        CleanupOp::Init(source_root),
        CleanupOp::Transfer {
            source: source_root,
            destination: destination_root,
            kind: TransferKind::Initialize,
        },
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| message.contains("incompatible move-path subtrees")));
}

#[test]
fn rejects_a_transfer_between_incompatible_aggregate_forests() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let source = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let destination = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let source = builder.new_move_path(Place::local(source), None).unwrap();
    let destination_root = builder
        .new_move_path(Place::local(destination), None)
        .unwrap();
    builder
        .new_move_path(
            Place::local(destination).project(Projection::Field(0)),
            Some(destination_root),
        )
        .unwrap();
    for operation in [
        CleanupOp::Init(source),
        CleanupOp::Transfer {
            source,
            destination: destination_root,
            kind: TransferKind::Initialize,
        },
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| message.contains("incompatible move-path subtrees")));
}

#[test]
fn rejects_a_transfer_into_an_inactive_enum_downcast() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let value = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let source = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let root = builder.new_move_path(Place::local(value), None).unwrap();
    let first = builder
        .new_move_path(
            Place::local(value).project(Projection::Downcast(0)),
            Some(root),
        )
        .unwrap();
    let second = builder
        .new_move_path(
            Place::local(value).project(Projection::Downcast(1)),
            Some(root),
        )
        .unwrap();
    let source = builder.new_move_path(Place::local(source), None).unwrap();
    for operation in [
        CleanupOp::SetDiscriminant {
            destination: root,
            variant: 0,
        },
        CleanupOp::Init(first),
        CleanupOp::Init(root),
        CleanupOp::Init(source),
        CleanupOp::Transfer {
            source,
            destination: second,
            kind: TransferKind::Initialize,
        },
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified()).iter().any(|message| {
        message.contains("Transfer destination")
            && message.contains("projects variant 1")
            && message.contains("possible variants [0]")
    }));
}

#[test]
fn rejects_invalid_enum_topology_operations_in_unreachable_blocks() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let orphan = builder.new_block(scope).unwrap();
    let scalar = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let scalar = builder.new_move_path(Place::local(scalar), None).unwrap();
    builder
        .set_terminator(
            builder.entry_block(),
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();
    builder
        .push_operation(
            orphan,
            CleanupOp::SetDiscriminant {
                destination: scalar,
                variant: 99,
            },
        )
        .unwrap();
    builder
        .push_operation(
            orphan,
            CleanupOp::AssumeDiscriminant {
                source: scalar,
                variant: 99,
            },
        )
        .unwrap();
    builder
        .set_terminator(orphan, Terminator::Unreachable)
        .unwrap();

    let errors = messages(&builder.into_unverified());
    assert!(errors
        .iter()
        .any(|message| { message.contains("SetDiscriminant") && message.contains("non-enum") }));
    assert!(errors
        .iter()
        .any(|message| { message.contains("AssumeDiscriminant") && message.contains("non-enum") }));
}

#[test]
fn rejects_assuming_a_discriminant_for_an_uninitialized_enum() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let value = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            false,
        )
        .unwrap();
    let root = builder.new_move_path(Place::local(value), None).unwrap();
    builder
        .new_move_path(
            Place::local(value).project(Projection::Downcast(0)),
            Some(root),
        )
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::StorageLive(value))
        .unwrap();
    builder
        .push_operation(
            entry,
            CleanupOp::AssumeDiscriminant {
                source: root,
                variant: 0,
            },
        )
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified()).iter().any(|message| {
        message.contains("AssumeDiscriminant")
            && message.contains("requires an initialized enum source")
    }));
}

#[test]
fn rejects_starting_a_temporary_lifetime_twice() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let temporary = builder
        .new_local(
            builder.root_scope(),
            LocalKind::Temporary,
            LocalOwnership::Owned,
            false,
        )
        .unwrap();
    builder
        .new_move_path(Place::local(temporary), None)
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::StorageLive(temporary))
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::StorageLive(temporary))
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();
    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| message.contains("StorageLive") && message.contains("requires dead")));
}

#[test]
fn rejects_non_root_entry() {
    let mut builder = CleanupPlanBuilder::new();
    let child = builder
        .new_scope(builder.root_scope(), ScopeKind::FunctionBody)
        .unwrap();
    let entry = builder.entry_block();
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();
    let mut plan = builder.into_unverified();
    plan.blocks[entry.index()].scope = child;

    let errors = messages(&plan);
    assert!(errors
        .iter()
        .any(|message| message.contains("entry block") && message.contains("root scope")));
}

#[test]
fn caches_operation_position_states_for_linear_moves() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let path = builder.new_move_path(Place::local(local), None).unwrap();
    for operation in [
        CleanupOp::StorageLive(local),
        CleanupOp::Init(path),
        CleanupOp::MoveOut(path),
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder.finish().expect("linear move state must verify");
    assert_eq!(
        plan.path_initialization_before(entry, 1, path),
        Some(PathInitialization::Uninitialized)
    );
    assert_eq!(
        plan.path_initialization_before(entry, 2, path),
        Some(PathInitialization::Initialized)
    );
    assert_eq!(
        plan.path_initialization_before(entry, 3, path),
        Some(PathInitialization::Uninitialized)
    );
    assert_eq!(
        plan.move_state_before(entry, 3),
        plan.move_state.block_exit(entry).cloned()
    );
    assert!(plan
        .move_state
        .block_entry(entry)
        .expect("cached entry state")
        .discriminants
        .is_empty());
}

#[test]
fn rebuilds_an_aggregate_root_after_every_field_is_initialized() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let root = builder.new_move_path(Place::local(local), None).unwrap();
    let left = builder
        .new_move_path(
            Place::local(local).project(Projection::Field(0)),
            Some(root),
        )
        .unwrap();
    let right = builder
        .new_move_path(
            Place::local(local).project(Projection::Field(1)),
            Some(root),
        )
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(local),
        CleanupOp::Init(root),
        CleanupOp::MoveOut(root),
        CleanupOp::Init(left),
        CleanupOp::Init(right),
        CleanupOp::MoveOut(root),
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder
        .finish()
        .expect("all exhaustive children must rebuild their root");
    assert_eq!(
        plan.path_initialization_before(entry, 4, root),
        Some(PathInitialization::MaybeOrPartial)
    );
    assert_eq!(
        plan.path_initialization_before(entry, 5, root),
        Some(PathInitialization::Initialized)
    );
}

#[test]
fn rebuilds_an_enum_root_after_its_active_variant_is_restored() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let root = builder.new_move_path(Place::local(local), None).unwrap();
    let variant = builder
        .new_move_path(
            Place::local(local).project(Projection::Downcast(0)),
            Some(root),
        )
        .unwrap();
    let field = builder
        .new_move_path(
            Place::local(local)
                .project(Projection::Downcast(0))
                .project(Projection::Field(0)),
            Some(variant),
        )
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(local),
        CleanupOp::SetDiscriminant {
            destination: root,
            variant: 0,
        },
        CleanupOp::Init(field),
        CleanupOp::MoveOut(field),
        CleanupOp::Init(field),
        CleanupOp::MoveOut(root),
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder
        .finish()
        .expect("restoring the active variant must rebuild the enum root");
    assert_eq!(
        plan.path_initialization_before(entry, 3, root),
        Some(PathInitialization::Initialized)
    );
    assert_eq!(
        plan.path_initialization_before(entry, 4, root),
        Some(PathInitialization::MaybeOrPartial)
    );
    assert_eq!(
        plan.path_initialization_before(entry, 5, root),
        Some(PathInitialization::Initialized)
    );
}

#[test]
fn overwrite_forgets_the_previous_enum_discriminant() {
    let mut builder = CleanupPlanBuilder::new();
    let entry = builder.entry_block();
    let local = builder
        .new_local(
            builder.root_scope(),
            LocalKind::User,
            LocalOwnership::Owned,
            true,
        )
        .unwrap();
    let root = builder.new_move_path(Place::local(local), None).unwrap();
    let first = builder
        .new_move_path(
            Place::local(local).project(Projection::Downcast(0)),
            Some(root),
        )
        .unwrap();
    builder
        .new_move_path(
            Place::local(local).project(Projection::Downcast(1)),
            Some(root),
        )
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::StorageLive(local))
        .unwrap();
    for operation in [
        CleanupOp::SetDiscriminant {
            destination: root,
            variant: 0,
        },
        CleanupOp::Init(first),
        CleanupOp::Init(root),
        CleanupOp::Overwrite(root),
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder
        .finish()
        .expect("whole enum overwrite must leave a complete value");
    assert_eq!(
        plan.possible_variants_before(entry, 5, root),
        Some((false, vec![0, 1]))
    );
}

#[test]
fn joins_a_diamond_as_maybe_initialized_for_maybe_overwrite() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let then_block = builder.new_block(scope).unwrap();
    let else_block = builder.new_block(scope).unwrap();
    let join = builder.new_block(scope).unwrap();
    let condition = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let destination = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let source = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let condition_path = builder
        .new_move_path(Place::local(condition), None)
        .unwrap();
    let destination_path = builder
        .new_move_path(Place::local(destination), None)
        .unwrap();
    let source_path = builder.new_move_path(Place::local(source), None).unwrap();
    for local in [condition, destination, source] {
        builder
            .push_operation(entry, CleanupOp::StorageLive(local))
            .unwrap();
    }
    for path in [condition_path, destination_path, source_path] {
        builder
            .push_operation(entry, CleanupOp::Init(path))
            .unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Branch {
                condition,
                then_edge: CleanupEdge::new(then_block, vec![]),
                else_edge: CleanupEdge::new(else_block, vec![]),
            },
        )
        .unwrap();
    builder
        .push_operation(then_block, CleanupOp::MoveOut(destination_path))
        .unwrap();
    builder
        .set_terminator(then_block, Terminator::Goto(CleanupEdge::new(join, vec![])))
        .unwrap();
    builder
        .set_terminator(else_block, Terminator::Goto(CleanupEdge::new(join, vec![])))
        .unwrap();
    builder
        .push_operation(
            join,
            CleanupOp::Transfer {
                source: source_path,
                destination: destination_path,
                kind: TransferKind::MaybeOverwrite,
            },
        )
        .unwrap();
    builder
        .set_terminator(
            join,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder
        .finish()
        .expect("diamond state must accept MaybeOverwrite");
    assert_eq!(
        plan.path_initialization_before(join, 0, destination_path),
        Some(PathInitialization::MaybeOrPartial)
    );
}

#[test]
fn joins_conditional_storage_and_ends_it_idempotently() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let then_block = builder.new_block(scope).unwrap();
    let else_block = builder.new_block(scope).unwrap();
    let join = builder.new_block(scope).unwrap();
    let condition = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let temporary = builder
        .new_local(scope, LocalKind::Temporary, LocalOwnership::Owned, false)
        .unwrap();
    let condition_path = builder
        .new_move_path(Place::local(condition), None)
        .unwrap();
    builder
        .new_move_path(Place::local(temporary), None)
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(condition),
        CleanupOp::Init(condition_path),
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Branch {
                condition,
                then_edge: CleanupEdge::new(then_block, vec![]),
                else_edge: CleanupEdge::new(else_block, vec![]),
            },
        )
        .unwrap();
    builder
        .push_operation(then_block, CleanupOp::StorageLive(temporary))
        .unwrap();
    builder
        .set_terminator(then_block, Terminator::Goto(CleanupEdge::new(join, vec![])))
        .unwrap();
    builder
        .set_terminator(else_block, Terminator::Goto(CleanupEdge::new(join, vec![])))
        .unwrap();
    for operation in [
        CleanupOp::StorageDead(temporary),
        CleanupOp::StorageLive(temporary),
        CleanupOp::StorageDead(temporary),
    ] {
        builder.push_operation(join, operation).unwrap();
    }
    builder
        .set_terminator(
            join,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder
        .finish()
        .expect("conditional storage must have a stable restart point");
    assert_eq!(
        plan.storage_liveness_before(join, 0, temporary),
        Some(StorageLiveness::MaybeLive)
    );
    assert_eq!(
        plan.storage_liveness_before(join, 1, temporary),
        Some(StorageLiveness::Dead)
    );
    assert_eq!(
        plan.storage_liveness_before(join, 2, temporary),
        Some(StorageLiveness::Live)
    );
}

#[test]
fn rejects_transfer_kinds_that_disagree_with_destination_state() {
    fn transfer_errors(kind: TransferKind, initialize_destination: bool) -> Vec<String> {
        let mut builder = CleanupPlanBuilder::new();
        let scope = builder.root_scope();
        let entry = builder.entry_block();
        let source = builder
            .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
            .unwrap();
        let destination = builder
            .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
            .unwrap();
        let source = builder.new_move_path(Place::local(source), None).unwrap();
        let destination = builder
            .new_move_path(Place::local(destination), None)
            .unwrap();
        builder
            .push_operation(entry, CleanupOp::Init(source))
            .unwrap();
        if initialize_destination {
            builder
                .push_operation(entry, CleanupOp::Init(destination))
                .unwrap();
        }
        builder
            .push_operation(
                entry,
                CleanupOp::Transfer {
                    source,
                    destination,
                    kind,
                },
            )
            .unwrap();
        builder
            .set_terminator(
                entry,
                Terminator::Return {
                    exited_scopes: vec![],
                },
            )
            .unwrap();
        messages(&builder.into_unverified())
    }

    for (kind, initialized, expected) in [
        (TransferKind::Initialize, true, "Initialized"),
        (TransferKind::Overwrite, false, "Uninitialized"),
        (TransferKind::MaybeOverwrite, true, "Initialized"),
        (TransferKind::MaybeOverwrite, false, "Uninitialized"),
    ] {
        assert!(transfer_errors(kind, initialized).iter().any(|message| {
            message.contains("incompatible state") && message.contains(expected)
        }));
    }
}

#[test]
fn converges_through_a_loop_and_ignores_an_orphan_block() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let header = builder.new_block(scope).unwrap();
    let body = builder.new_block(scope).unwrap();
    let exit = builder.new_block(scope).unwrap();
    let orphan = builder.new_block(scope).unwrap();
    let condition = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let value = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let orphan_local = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let condition_path = builder
        .new_move_path(Place::local(condition), None)
        .unwrap();
    let value_path = builder.new_move_path(Place::local(value), None).unwrap();
    let orphan_path = builder
        .new_move_path(Place::local(orphan_local), None)
        .unwrap();
    for local in [condition, value] {
        builder
            .push_operation(entry, CleanupOp::StorageLive(local))
            .unwrap();
    }
    for path in [condition_path, value_path] {
        builder
            .push_operation(entry, CleanupOp::Init(path))
            .unwrap();
    }
    builder
        .set_terminator(entry, Terminator::Goto(CleanupEdge::new(header, vec![])))
        .unwrap();
    builder
        .set_terminator(
            header,
            Terminator::Branch {
                condition,
                then_edge: CleanupEdge::new(body, vec![]),
                else_edge: CleanupEdge::new(exit, vec![]),
            },
        )
        .unwrap();
    for operation in [CleanupOp::MoveOut(value_path), CleanupOp::Init(value_path)] {
        builder.push_operation(body, operation).unwrap();
    }
    builder
        .set_terminator(body, Terminator::Goto(CleanupEdge::new(header, vec![])))
        .unwrap();
    builder
        .push_operation(exit, CleanupOp::MoveOut(value_path))
        .unwrap();
    builder
        .set_terminator(
            exit,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();
    builder
        .push_operation(orphan, CleanupOp::MoveOut(orphan_path))
        .unwrap();
    builder
        .set_terminator(orphan, Terminator::Unreachable)
        .unwrap();

    let plan = builder
        .finish()
        .expect("reinitialized loop backedge must reach a fixed point");
    assert_eq!(
        plan.path_initialization_before(header, 0, value_path),
        Some(PathInitialization::Initialized)
    );
    assert!(!plan.move_state.block_entry(orphan).unwrap().reachable);
}

#[test]
fn ignores_an_unreachable_predecessor_that_targets_a_reachable_join() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let join = builder.new_block(scope).unwrap();
    let orphan = builder.new_block(scope).unwrap();
    let value = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let path = builder.new_move_path(Place::local(value), None).unwrap();
    builder
        .push_operation(entry, CleanupOp::StorageLive(value))
        .unwrap();
    builder
        .push_operation(entry, CleanupOp::Init(path))
        .unwrap();
    builder
        .set_terminator(entry, Terminator::Goto(CleanupEdge::new(join, vec![])))
        .unwrap();
    builder
        .push_operation(orphan, CleanupOp::MoveOut(path))
        .unwrap();
    builder
        .set_terminator(orphan, Terminator::Goto(CleanupEdge::new(join, vec![])))
        .unwrap();
    builder
        .push_operation(join, CleanupOp::MoveOut(path))
        .unwrap();
    builder
        .set_terminator(
            join,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder
        .finish()
        .expect("an unreachable predecessor must not weaken the reachable join");
    assert_eq!(
        plan.path_initialization_before(join, 0, path),
        Some(PathInitialization::Initialized)
    );
    assert!(!plan.move_state.block_entry(orphan).unwrap().reachable);
}

#[test]
fn rejects_a_loop_backedge_that_does_not_restore_a_moved_value() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let header = builder.new_block(scope).unwrap();
    let body = builder.new_block(scope).unwrap();
    let exit = builder.new_block(scope).unwrap();
    let condition = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let value = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let condition_path = builder
        .new_move_path(Place::local(condition), None)
        .unwrap();
    let value_path = builder.new_move_path(Place::local(value), None).unwrap();
    for path in [condition_path, value_path] {
        builder
            .push_operation(entry, CleanupOp::Init(path))
            .unwrap();
    }
    builder
        .set_terminator(entry, Terminator::Goto(CleanupEdge::new(header, vec![])))
        .unwrap();
    builder
        .set_terminator(
            header,
            Terminator::Branch {
                condition,
                then_edge: CleanupEdge::new(body, vec![]),
                else_edge: CleanupEdge::new(exit, vec![]),
            },
        )
        .unwrap();
    builder
        .push_operation(body, CleanupOp::MoveOut(value_path))
        .unwrap();
    builder
        .set_terminator(body, Terminator::Goto(CleanupEdge::new(header, vec![])))
        .unwrap();
    builder
        .set_terminator(
            exit,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    assert!(messages(&builder.into_unverified())
        .iter()
        .any(|message| { message.contains("MoveOut") && message.contains("MaybeOrPartial") }));
}

#[test]
fn joins_different_enum_discriminants_while_preserving_a_full_root() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let first = builder.new_block(scope).unwrap();
    let second = builder.new_block(scope).unwrap();
    let join = builder.new_block(scope).unwrap();
    let condition = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let value = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, true)
        .unwrap();
    let condition_path = builder
        .new_move_path(Place::local(condition), None)
        .unwrap();
    let root = builder.new_move_path(Place::local(value), None).unwrap();
    let first_variant = builder
        .new_move_path(
            Place::local(value).project(Projection::Downcast(0)),
            Some(root),
        )
        .unwrap();
    let second_variant = builder
        .new_move_path(
            Place::local(value).project(Projection::Downcast(1)),
            Some(root),
        )
        .unwrap();
    for local in [condition, value] {
        builder
            .push_operation(entry, CleanupOp::StorageLive(local))
            .unwrap();
    }
    builder
        .push_operation(entry, CleanupOp::Init(condition_path))
        .unwrap();
    builder
        .set_terminator(
            entry,
            Terminator::Branch {
                condition,
                then_edge: CleanupEdge::new(first, vec![]),
                else_edge: CleanupEdge::new(second, vec![]),
            },
        )
        .unwrap();
    for (block, variant, variant_path) in [(first, 0, first_variant), (second, 1, second_variant)] {
        for operation in [
            CleanupOp::SetDiscriminant {
                destination: root,
                variant,
            },
            CleanupOp::Init(variant_path),
            CleanupOp::Init(root),
        ] {
            builder.push_operation(block, operation).unwrap();
        }
        builder
            .set_terminator(block, Terminator::Goto(CleanupEdge::new(join, vec![])))
            .unwrap();
    }
    builder
        .push_operation(join, CleanupOp::MoveOut(root))
        .unwrap();
    builder
        .set_terminator(
            join,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder
        .finish()
        .expect("both initialized enum variants must join as a full value");
    assert_eq!(
        plan.path_initialization_before(join, 0, root),
        Some(PathInitialization::Initialized)
    );
    assert_eq!(
        plan.possible_variants_before(join, 0, root),
        Some((false, vec![0, 1]))
    );
}

#[test]
fn scope_exit_edges_clear_move_state_before_the_target() {
    let mut builder = CleanupPlanBuilder::new();
    let root_scope = builder.root_scope();
    let child_scope = builder.new_scope(root_scope, ScopeKind::Lexical).unwrap();
    let entry = builder.entry_block();
    let child = builder.new_block(child_scope).unwrap();
    let after = builder.new_block(root_scope).unwrap();
    let local = builder
        .new_local(child_scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let path = builder.new_move_path(Place::local(local), None).unwrap();
    builder
        .set_terminator(entry, Terminator::Goto(CleanupEdge::new(child, vec![])))
        .unwrap();
    builder
        .push_operation(child, CleanupOp::StorageLive(local))
        .unwrap();
    builder
        .push_operation(child, CleanupOp::Init(path))
        .unwrap();
    builder
        .set_terminator(
            child,
            Terminator::Goto(CleanupEdge::new(after, vec![child_scope])),
        )
        .unwrap();
    builder
        .set_terminator(
            after,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder.finish().expect("scope exit must clear its locals");
    assert_eq!(
        plan.path_initialization_before(after, 0, path),
        Some(PathInitialization::Uninitialized)
    );
}

#[test]
fn plans_static_drop_without_a_runtime_flag() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let block = builder.entry_block();
    let local = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let path = builder
        .new_move_path_with_drop(Place::local(local), None, true)
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(local),
        CleanupOp::Init(path),
        CleanupOp::StorageDead(local),
    ] {
        builder.push_operation(block, operation).unwrap();
    }
    builder
        .set_terminator(
            block,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder.finish().expect("static drop plan");
    assert!(plan.drop_flags.flags.is_empty());
    assert_eq!(plan.drop_flags.sites.len(), 1);
    assert_eq!(
        plan.drop_flags.sites[0].obligations,
        vec![DropObligation {
            path,
            condition: DropCondition::Static,
            children_when_clear: Vec::new(),
        }]
    );
}

#[test]
fn allocates_a_flag_for_conditionally_initialized_drop_storage() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let entry = builder.entry_block();
    let then_block = builder.new_block(scope).unwrap();
    let else_block = builder.new_block(scope).unwrap();
    let join = builder.new_block(scope).unwrap();
    let condition = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let condition_path = builder
        .new_move_path(Place::local(condition), None)
        .unwrap();
    let local = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let path = builder
        .new_move_path_with_drop(Place::local(local), None, true)
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(condition),
        CleanupOp::Init(condition_path),
        CleanupOp::StorageLive(local),
    ] {
        builder.push_operation(entry, operation).unwrap();
    }
    builder
        .set_terminator(
            entry,
            Terminator::Branch {
                condition,
                then_edge: CleanupEdge::new(then_block, vec![]),
                else_edge: CleanupEdge::new(else_block, vec![]),
            },
        )
        .unwrap();
    builder
        .push_operation(then_block, CleanupOp::Init(path))
        .unwrap();
    for block in [then_block, else_block] {
        builder
            .set_terminator(block, Terminator::Goto(CleanupEdge::new(join, vec![])))
            .unwrap();
    }
    builder
        .push_operation(join, CleanupOp::StorageDead(local))
        .unwrap();
    builder
        .set_terminator(
            join,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder.finish().expect("conditional drop plan");
    assert_eq!(plan.drop_flags.flags.len(), 1);
    let flag = plan.drop_flags.flag_for_path(path).expect("path flag");
    assert!(plan.drop_flags.actions.iter().any(|action| {
        action.flag == flag && action.block == then_block && action.value == DropFlagValue::Set
    }));
    assert!(plan.drop_flags.actions.iter().any(|action| {
        action.flag == flag && action.block == join && action.value == DropFlagValue::Clear
    }));
    assert_eq!(plan.drop_flags.sites.len(), 1);
    assert_eq!(
        plan.drop_flags.sites[0].obligations[0].condition,
        DropCondition::Flag { value: flag }
    );
}

#[test]
fn drops_only_initialized_children_of_a_partial_aggregate() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let block = builder.entry_block();
    let local = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let root = builder
        .new_move_path_with_drop(Place::local(local), None, true)
        .unwrap();
    let left = builder
        .new_move_path_with_drop(
            Place::local(local).project(Projection::Field(0)),
            Some(root),
            true,
        )
        .unwrap();
    builder
        .new_move_path_with_drop(
            Place::local(local).project(Projection::Field(1)),
            Some(root),
            true,
        )
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(local),
        CleanupOp::Init(root),
        CleanupOp::MoveOut(root),
        CleanupOp::Init(left),
        CleanupOp::StorageDead(local),
    ] {
        builder.push_operation(block, operation).unwrap();
    }
    builder
        .set_terminator(
            block,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let plan = builder.finish().expect("partial aggregate drop plan");
    assert!(plan.drop_flags.flags.is_empty());
    assert_eq!(
        plan.drop_flags.sites[0].obligations,
        vec![DropObligation {
            path: left,
            condition: DropCondition::Static,
            children_when_clear: Vec::new(),
        }]
    );
}

#[test]
fn rejects_a_stale_cached_drop_flag_analysis() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let block = builder.entry_block();
    let local = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let path = builder
        .new_move_path_with_drop(Place::local(local), None, true)
        .unwrap();
    for operation in [
        CleanupOp::StorageLive(local),
        CleanupOp::Init(path),
        CleanupOp::StorageDead(local),
    ] {
        builder.push_operation(block, operation).unwrap();
    }
    builder
        .set_terminator(
            block,
            Terminator::Return {
                exited_scopes: vec![],
            },
        )
        .unwrap();

    let mut plan = builder.finish().expect("drop analysis must verify");
    plan.drop_flags.sites.clear();
    let errors = messages(&plan);
    assert!(errors
        .iter()
        .any(|message| message.contains("cached drop-flag analysis")));
}

#[test]
fn rejects_a_drop_child_beneath_a_non_dropping_parent() {
    let mut builder = CleanupPlanBuilder::new();
    let scope = builder.root_scope();
    let local = builder
        .new_local(scope, LocalKind::User, LocalOwnership::Owned, false)
        .unwrap();
    let root = builder.new_move_path(Place::local(local), None).unwrap();
    builder
        .new_move_path_with_drop(
            Place::local(local).project(Projection::Field(0)),
            Some(root),
            true,
        )
        .unwrap();
    let plan = builder.into_unverified();
    let errors = messages(&plan);
    assert!(errors
        .iter()
        .any(|message| message.contains("non-dropping parent")));
}
