use super::*;
use crate::cleanup::{
    BasicBlockId as CleanupBlockId, CleanupEdge, CleanupOp, CleanupPlan, CleanupPlanBuilder,
    LocalId as CleanupLocalId, LocalKind as CleanupLocalKind,
    LocalOwnership as CleanupLocalOwnership, MovePathId as CleanupMovePathId,
    Place as CleanupPlace, Projection as CleanupProjection, ScopeId as CleanupScopeId,
    ScopeKind as CleanupScopeKind, Terminator as CleanupTerminator, TransferKind,
};

pub(super) fn build_and_verify_cleanup_plans(
    program: &HirProgram,
) -> Result<Vec<CleanupPlan>, Vec<Diagnostic>> {
    let mut plans = Vec::with_capacity(program.functions.len());
    let mut diagnostics = Vec::new();
    for function in &program.functions {
        match HirCleanupPlanner::build(program, function) {
            Ok(plan) => plans.push(plan),
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }
    if diagnostics.is_empty() {
        Ok(plans)
    } else {
        Err(diagnostics)
    }
}

#[derive(Clone, Copy)]
struct CleanupCursor {
    block: CleanupBlockId,
    scope: CleanupScopeId,
}

#[derive(Clone, Copy)]
struct CleanupTrackedLocal {
    id: CleanupLocalId,
    ownership: CleanupLocalOwnership,
    kind: CleanupLocalKind,
}

struct CleanupSourceLocal<'a> {
    id: LocalId,
    name: &'a str,
    ty: &'a Ty,
}

#[derive(Clone)]
struct CleanupLoopFrame {
    break_target: CleanupBlockId,
    continue_target: CleanupBlockId,
    exit_scope: CleanupScopeId,
    continue_scope: CleanupScopeId,
    result_destination: Option<CleanupDestination>,
    saw_break: bool,
}

#[derive(Clone, Debug)]
struct CleanupDestination {
    place: CleanupPlace,
    path: CleanupMovePathId,
}

#[derive(Clone, Debug)]
enum ResultUse {
    Store(CleanupDestination),
    Discard,
}

pub(super) const MAX_CLEANUP_MOVE_PATHS: usize = 65_536;

/// Builds a type-independent ownership CFG from typed HIR. This is not drop
/// lowering: it records storage, initialization, moves and scope exits, while
/// retaining explicit pending capabilities for cases which still need stable
/// temporary identities or runtime liveness before drop glue can be emitted.
pub(super) struct HirCleanupPlanner<'a> {
    program: &'a HirProgram,
    function: &'a HirFunction,
    builder: CleanupPlanBuilder,
    root_scope: CleanupScopeId,
    scope_parents: HashMap<CleanupScopeId, Option<CleanupScopeId>>,
    scope_locals: HashMap<CleanupScopeId, Vec<CleanupTrackedLocal>>,
    hir_locals: HashMap<LocalId, CleanupLocalId>,
    local_ownership: HashMap<CleanupLocalId, CleanupLocalOwnership>,
    move_paths: HashMap<CleanupPlace, CleanupMovePathId>,
    return_destination: Option<CleanupDestination>,
    loops: Vec<CleanupLoopFrame>,
}

impl<'a> HirCleanupPlanner<'a> {
    pub(super) fn build(
        program: &'a HirProgram,
        function: &'a HirFunction,
    ) -> Result<CleanupPlan, Vec<Diagnostic>> {
        let mut builder = CleanupPlanBuilder::new();
        let root_scope = builder.root_scope();
        let function_scope = builder
            .new_scope(root_scope, CleanupScopeKind::FunctionBody)
            .map_err(|error| vec![Self::build_diagnostic(function, error)])?;
        let mut scope_parents = HashMap::new();
        scope_parents.insert(root_scope, None);
        scope_parents.insert(function_scope, Some(root_scope));
        let mut planner = Self {
            program,
            function,
            builder,
            root_scope,
            scope_parents,
            scope_locals: HashMap::new(),
            hir_locals: HashMap::new(),
            local_ownership: HashMap::new(),
            move_paths: HashMap::new(),
            return_destination: None,
            loops: Vec::new(),
        };
        let function_entry = planner
            .new_block(function_scope)
            .map_err(|error| vec![error])?;
        planner
            .terminate(
                planner.builder.entry_block(),
                CleanupTerminator::Goto(CleanupEdge::new(function_entry, Vec::new())),
            )
            .map_err(|error| vec![error])?;
        let mut cursor = CleanupCursor {
            block: function_entry,
            scope: function_scope,
        };

        let uninhabited_entry = function
            .params
            .iter()
            .any(|parameter| program.is_uninhabited(&parameter.ty));

        for param in &function.params {
            let ownership = match param.mode {
                PassMode::Borrow => CleanupLocalOwnership::SharedBorrow,
                PassMode::MutBorrow => CleanupLocalOwnership::MutableBorrow,
                PassMode::Inferred | PassMode::Copy | PassMode::Move => {
                    CleanupLocalOwnership::Owned
                }
            };
            let local = planner
                .declare_source_local(
                    function_scope,
                    CleanupLocalKind::Argument,
                    ownership,
                    param.mode == PassMode::MutBorrow,
                    CleanupSourceLocal {
                        id: param.id,
                        name: &param.name,
                        ty: &param.ty,
                    },
                )
                .map_err(|error| vec![error])?;
            if uninhabited_entry {
                continue;
            }
            planner
                .operation(cursor.block, CleanupOp::StorageLive(local))
                .map_err(|error| vec![error])?;
            if ownership == CleanupLocalOwnership::Owned {
                let path = planner.root_move_path(local).map_err(|error| vec![error])?;
                planner
                    .operation(cursor.block, CleanupOp::Init(path))
                    .map_err(|error| vec![error])?;
            }
        }

        if uninhabited_entry {
            planner
                .terminate(cursor.block, CleanupTerminator::Unreachable)
                .map_err(|error| vec![error])?;
            return planner.finish();
        }

        if !matches!(function.result, Ty::Unit | Ty::Never | Ty::Error) {
            let return_local = planner
                .declare_generated_local(
                    function_scope,
                    CleanupLocalKind::ReturnPlace,
                    CleanupLocalOwnership::Owned,
                    true,
                    &function.result,
                )
                .map_err(|error| vec![error])?;
            planner
                .operation(cursor.block, CleanupOp::StorageLive(return_local))
                .map_err(|error| vec![error])?;
            let place = CleanupPlace::local(return_local);
            let path = planner
                .lookup_move_path(&place)
                .map_err(|error| vec![error])?;
            planner.return_destination = Some(CleanupDestination { place, path });
        }

        let body_use =
            if planner.return_destination.is_some() && !program.is_uninhabited(&function.body.ty) {
                let (_, stage) = planner
                    .prepare_temporary_destination(cursor, &function.body.ty)
                    .map_err(|error| vec![error])?;
                ResultUse::Store(stage)
            } else {
                ResultUse::Discard
            };
        let body_stage = match &body_use {
            ResultUse::Store(destination) => Some(destination.clone()),
            ResultUse::Discard => None,
        };
        let end = planner
            .walk_expr(&function.body, cursor, body_use)
            .map_err(|error| vec![error])?;
        if let Some(end) = end {
            cursor = end;
            if let (Some(source), Some(return_destination)) =
                (body_stage, planner.return_destination.clone())
            {
                planner
                    .transfer(
                        cursor,
                        &source,
                        &return_destination,
                        TransferKind::Initialize,
                    )
                    .map_err(|error| vec![error])?;
            }
            planner
                .emit_storage_dead_to(cursor.block, cursor.scope, planner.root_scope)
                .map_err(|error| vec![error])?;
            let exited_scopes = planner
                .exit_chain(cursor.scope, planner.root_scope)
                .map_err(|error| vec![error])?;
            planner
                .terminate(cursor.block, CleanupTerminator::Return { exited_scopes })
                .map_err(|error| vec![error])?;
        }

        planner.finish()
    }

    fn finish(self) -> Result<CleanupPlan, Vec<Diagnostic>> {
        let function_name = self.function.name.clone();
        self.builder.finish().map_err(|errors| {
            errors
                .into_iter()
                .map(|error| {
                    Diagnostic::new(format!(
                        "internal cleanup plan error in function `{}`: {}",
                        function_name, error
                    ))
                })
                .collect()
        })
    }

    fn build_diagnostic(function: &HirFunction, error: impl fmt::Display) -> Diagnostic {
        Diagnostic::new(format!(
            "internal cleanup planner error in function `{}`: {error}",
            function.name
        ))
    }

    fn diagnostic(&self, error: impl fmt::Display) -> Diagnostic {
        Self::build_diagnostic(self.function, error)
    }

    fn new_scope(
        &mut self,
        parent: CleanupScopeId,
        kind: CleanupScopeKind,
    ) -> Result<CleanupScopeId, Diagnostic> {
        let scope = self
            .builder
            .new_scope(parent, kind)
            .map_err(|error| self.diagnostic(error))?;
        self.scope_parents.insert(scope, Some(parent));
        Ok(scope)
    }

    fn new_block(&mut self, scope: CleanupScopeId) -> Result<CleanupBlockId, Diagnostic> {
        self.builder
            .new_block(scope)
            .map_err(|error| self.diagnostic(error))
    }

    fn operation(&mut self, block: CleanupBlockId, operation: CleanupOp) -> Result<(), Diagnostic> {
        self.builder
            .push_operation(block, operation)
            .map_err(|error| self.diagnostic(error))
    }

    fn terminate(
        &mut self,
        block: CleanupBlockId,
        terminator: CleanupTerminator,
    ) -> Result<(), Diagnostic> {
        self.builder
            .set_terminator(block, terminator)
            .map_err(|error| self.diagnostic(error))
    }

    fn declare_source_local(
        &mut self,
        scope: CleanupScopeId,
        kind: CleanupLocalKind,
        ownership: CleanupLocalOwnership,
        mutable: bool,
        source: CleanupSourceLocal<'_>,
    ) -> Result<CleanupLocalId, Diagnostic> {
        let local = self
            .builder
            .new_source_local(scope, kind, ownership, mutable, source.id, source.name)
            .map_err(|error| self.diagnostic(error))?;
        if self.hir_locals.insert(source.id, local).is_some() {
            return Err(self.diagnostic(format!(
                "HIR local {} was declared more than once",
                source.id
            )));
        }
        self.track_local(scope, local, kind, ownership);
        if ownership == CleanupLocalOwnership::Owned {
            self.register_move_path_forest(local, source.ty)?;
        }
        Ok(local)
    }

    fn declare_generated_local(
        &mut self,
        scope: CleanupScopeId,
        kind: CleanupLocalKind,
        ownership: CleanupLocalOwnership,
        mutable: bool,
        ty: &Ty,
    ) -> Result<CleanupLocalId, Diagnostic> {
        let local = self
            .builder
            .new_local(scope, kind, ownership, mutable)
            .map_err(|error| self.diagnostic(error))?;
        self.track_local(scope, local, kind, ownership);
        if ownership == CleanupLocalOwnership::Owned {
            self.register_move_path_forest(local, ty)?;
        }
        Ok(local)
    }

    fn track_local(
        &mut self,
        scope: CleanupScopeId,
        local: CleanupLocalId,
        kind: CleanupLocalKind,
        ownership: CleanupLocalOwnership,
    ) {
        self.scope_locals
            .entry(scope)
            .or_default()
            .push(CleanupTrackedLocal {
                id: local,
                ownership,
                kind,
            });
        self.local_ownership.insert(local, ownership);
    }

    fn root_move_path(&self, local: CleanupLocalId) -> Result<CleanupMovePathId, Diagnostic> {
        self.lookup_move_path(&CleanupPlace::local(local))
    }

    fn register_move_path(
        &mut self,
        place: CleanupPlace,
        parent: Option<CleanupMovePathId>,
        needs_drop: bool,
    ) -> Result<CleanupMovePathId, Diagnostic> {
        if let Some(path) = self.move_paths.get(&place) {
            return Ok(*path);
        }
        if self.move_paths.len() >= MAX_CLEANUP_MOVE_PATHS {
            return Err(self.diagnostic(format!(
                "cleanup move-path limit of {MAX_CLEANUP_MOVE_PATHS} exceeded"
            )));
        }
        let path = self
            .builder
            .new_move_path_with_drop(place.clone(), parent, needs_drop)
            .map_err(|error| self.diagnostic(error))?;
        self.move_paths.insert(place, path);
        Ok(path)
    }

    fn register_move_path_forest(
        &mut self,
        local: CleanupLocalId,
        ty: &Ty,
    ) -> Result<CleanupMovePathId, Diagnostic> {
        let mut visiting = HashSet::new();
        self.register_typed_move_path(CleanupPlace::local(local), None, ty, &mut visiting)
    }

    fn register_typed_move_path(
        &mut self,
        place: CleanupPlace,
        parent: Option<CleanupMovePathId>,
        ty: &Ty,
        visiting: &mut HashSet<(NominalKind, String)>,
    ) -> Result<CleanupMovePathId, Diagnostic> {
        let path = self.register_move_path(place.clone(), parent, self.program.needs_drop(ty))?;
        match ty {
            Ty::Array(element, length) => {
                for index in 0..*length {
                    let child = place
                        .clone()
                        .project(CleanupProjection::ConstantIndex(index));
                    self.register_typed_move_path(child, Some(path), element, visiting)?;
                }
            }
            Ty::Struct(name) => {
                let key = (NominalKind::Struct, name.clone());
                if !visiting.insert(key.clone()) {
                    return Err(self.diagnostic(format!(
                        "recursive cleanup move-path layout for struct `{name}`"
                    )));
                }
                let fields = self
                    .program
                    .struct_layout(name)
                    .ok_or_else(|| {
                        self.diagnostic(format!(
                            "missing struct layout `{name}` while building cleanup move paths"
                        ))
                    })?
                    .fields
                    .iter()
                    .map(|field| field.ty.clone())
                    .collect::<Vec<_>>();
                for (index, field_ty) in fields.iter().enumerate() {
                    let field = u32::try_from(index).map_err(|_| {
                        self.diagnostic(format!(
                            "field index {index} in struct `{name}` does not fit in u32"
                        ))
                    })?;
                    let child = place.clone().project(CleanupProjection::Field(field));
                    self.register_typed_move_path(child, Some(path), field_ty, visiting)?;
                }
                visiting.remove(&key);
            }
            Ty::Enum(name) => {
                let key = (NominalKind::Enum, name.clone());
                if !visiting.insert(key.clone()) {
                    return Err(self.diagnostic(format!(
                        "recursive cleanup move-path layout for enum `{name}`"
                    )));
                }
                let variants = self
                    .program
                    .enum_layout(name)
                    .ok_or_else(|| {
                        self.diagnostic(format!(
                            "missing enum layout `{name}` while building cleanup move paths"
                        ))
                    })?
                    .variants
                    .iter()
                    .map(|variant| {
                        variant
                            .fields
                            .iter()
                            .map(|field| field.ty.clone())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                for (variant_index, fields) in variants.iter().enumerate() {
                    let variant = u32::try_from(variant_index).map_err(|_| {
                        self.diagnostic(format!(
                            "variant index {variant_index} in enum `{name}` does not fit in u32"
                        ))
                    })?;
                    let variant_place = place.clone().project(CleanupProjection::Downcast(variant));
                    let variant_needs_drop = fields
                        .iter()
                        .any(|field_ty| self.program.needs_drop(field_ty));
                    let variant_path = self.register_move_path(
                        variant_place.clone(),
                        Some(path),
                        variant_needs_drop,
                    )?;
                    for (field_index, field_ty) in fields.iter().enumerate() {
                        let field = u32::try_from(field_index).map_err(|_| {
                            self.diagnostic(format!(
                                "field index {field_index} in enum `{name}` variant {variant_index} does not fit in u32"
                            ))
                        })?;
                        let child = variant_place
                            .clone()
                            .project(CleanupProjection::Field(field));
                        self.register_typed_move_path(
                            child,
                            Some(variant_path),
                            field_ty,
                            visiting,
                        )?;
                    }
                }
                visiting.remove(&key);
            }
            Ty::Callable(callable) => {
                for (index, capture) in callable.captures.iter().enumerate() {
                    let capture_index = u32::try_from(index).map_err(|_| {
                        self.diagnostic(format!(
                            "callable capture index {index} does not fit in u32"
                        ))
                    })?;
                    let capture_place = place
                        .clone()
                        .project(CleanupProjection::Capture(capture_index));
                    if matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow) {
                        self.register_move_path(capture_place, Some(path), false)?;
                    } else {
                        self.register_typed_move_path(
                            capture_place,
                            Some(path),
                            &capture.ty,
                            visiting,
                        )?;
                    }
                }
            }
            Ty::Continuation { .. } | Ty::EffectCallable { .. } => {}
            Ty::I32
            | Ty::I64
            | Ty::U32
            | Ty::U64
            | Ty::Bool
            | Ty::Unit
            | Ty::Pointer { .. }
            | Ty::Reference { .. }
            | Ty::Never
            | Ty::Function(_)
            | Ty::EffectRow { .. }
            | Ty::Error => {}
        }
        Ok(path)
    }

    fn lookup_move_path(&self, place: &CleanupPlace) -> Result<CleanupMovePathId, Diagnostic> {
        self.move_paths.get(place).copied().ok_or_else(|| {
            self.diagnostic(format!(
                "cleanup place {place:?} is absent from its pre-registered move-path forest"
            ))
        })
    }

    fn move_path_for_hir_place(
        &mut self,
        place: &HirPlace,
    ) -> Result<Option<CleanupMovePathId>, Diagnostic> {
        if place.capability != LocalCapability::Owned {
            return Ok(None);
        }
        let Some(local) = self.hir_locals.get(&place.local).copied() else {
            return Err(self.diagnostic(format!(
                "HIR place refers to unmapped local {}",
                place.local
            )));
        };
        if self.local_ownership.get(&local) != Some(&CleanupLocalOwnership::Owned) {
            return Ok(None);
        }
        let mut cleanup_place = CleanupPlace::local(local);
        let mut ty = &place.root_ty;
        for projection in &place.projections {
            match ty {
                Ty::Struct(name) => {
                    let field = u32::try_from(*projection).map_err(|_| {
                        self.diagnostic(format!(
                            "field projection {projection} does not fit in u32"
                        ))
                    })?;
                    cleanup_place = cleanup_place.project(CleanupProjection::Field(field));
                    ty = &self
                        .program
                        .struct_layout(name)
                        .and_then(|layout| layout.fields.get(*projection))
                        .ok_or_else(|| {
                            self.diagnostic(format!(
                                "field projection {projection} is invalid for `{name}`"
                            ))
                        })?
                        .ty;
                }
                Ty::Array(element, length) => {
                    let index = u64::try_from(*projection).map_err(|_| {
                        self.diagnostic(format!(
                            "array projection {projection} does not fit in u64"
                        ))
                    })?;
                    if index >= *length {
                        return Err(self.diagnostic(format!(
                            "array projection {index} is out of bounds for length {length}"
                        )));
                    }
                    cleanup_place = cleanup_place.project(CleanupProjection::ConstantIndex(index));
                    ty = element;
                }
                _ => {
                    return Err(self.diagnostic(format!(
                        "projection {projection} continues through non-aggregate `{ty}`"
                    )));
                }
            }
        }
        self.lookup_move_path(&cleanup_place).map(Some)
    }

    fn exit_chain(
        &self,
        from: CleanupScopeId,
        target: CleanupScopeId,
    ) -> Result<Vec<CleanupScopeId>, Diagnostic> {
        let mut chain = Vec::new();
        let mut cursor = from;
        while cursor != target {
            if cursor == self.root_scope {
                return Err(self.diagnostic("scope exit target is not an ancestor"));
            }
            chain.push(cursor);
            cursor = self
                .scope_parents
                .get(&cursor)
                .copied()
                .flatten()
                .ok_or_else(|| self.diagnostic("cleanup scope has no recorded parent"))?;
        }
        Ok(chain)
    }

    fn emit_storage_dead_to(
        &mut self,
        block: CleanupBlockId,
        from: CleanupScopeId,
        target: CleanupScopeId,
    ) -> Result<(), Diagnostic> {
        for scope in self.exit_chain(from, target)? {
            let locals = self.scope_locals.get(&scope).cloned().unwrap_or_default();
            for local in locals.into_iter().rev() {
                if local.ownership == CleanupLocalOwnership::Owned
                    && local.kind != CleanupLocalKind::ReturnPlace
                {
                    self.operation(block, CleanupOp::StorageDead(local.id))?;
                }
            }
        }
        Ok(())
    }

    fn goto_exiting(
        &mut self,
        cursor: CleanupCursor,
        target_block: CleanupBlockId,
        target_scope: CleanupScopeId,
    ) -> Result<(), Diagnostic> {
        self.emit_storage_dead_to(cursor.block, cursor.scope, target_scope)?;
        let exited_scopes = self.exit_chain(cursor.scope, target_scope)?;
        self.terminate(
            cursor.block,
            CleanupTerminator::Goto(CleanupEdge::new(target_block, exited_scopes)),
        )
    }

    fn prepare_temporary(
        &mut self,
        cursor: CleanupCursor,
        ty: &Ty,
    ) -> Result<(CleanupLocalId, CleanupMovePathId), Diagnostic> {
        let (local, destination) = self.prepare_temporary_destination(cursor, ty)?;
        Ok((local, destination.path))
    }

    fn prepare_temporary_destination(
        &mut self,
        cursor: CleanupCursor,
        ty: &Ty,
    ) -> Result<(CleanupLocalId, CleanupDestination), Diagnostic> {
        let local = self.declare_generated_local(
            cursor.scope,
            CleanupLocalKind::Temporary,
            CleanupLocalOwnership::Owned,
            false,
            ty,
        )?;
        self.operation(cursor.block, CleanupOp::StorageLive(local))?;
        let path = self.root_move_path(local)?;
        Ok((
            local,
            CleanupDestination {
                place: CleanupPlace::local(local),
                path,
            },
        ))
    }

    fn project_destination(
        &self,
        destination: &CleanupDestination,
        projection: CleanupProjection,
    ) -> Result<CleanupDestination, Diagnostic> {
        let place = destination.place.clone().project(projection);
        let path = self.lookup_move_path(&place)?;
        Ok(CleanupDestination { place, path })
    }

    fn register_callable_capture_destination(
        &mut self,
        destination: &CleanupDestination,
        index: usize,
        value_ty: Option<&Ty>,
    ) -> Result<CleanupDestination, Diagnostic> {
        let capture = u32::try_from(index).map_err(|_| {
            self.diagnostic(format!(
                "callable capture index {index} does not fit in u32"
            ))
        })?;
        let place = destination
            .place
            .clone()
            .project(CleanupProjection::Capture(capture));
        let path = if let Some(path) = self.move_paths.get(&place).copied() {
            path
        } else if let Some(value_ty) = value_ty {
            self.register_typed_move_path(
                place.clone(),
                Some(destination.path),
                value_ty,
                &mut HashSet::new(),
            )?
        } else {
            self.register_move_path(place.clone(), Some(destination.path), false)?
        };
        Ok(CleanupDestination { place, path })
    }

    fn is_resource_ty(ty: &Ty) -> bool {
        matches!(
            ty,
            Ty::Array(_, _) | Ty::Struct(_) | Ty::Enum(_) | Ty::Function(_) | Ty::Callable(_)
        )
    }

    fn materialize_discarded_resource(
        &mut self,
        expression: &HirExpr,
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<ResultUse, Diagnostic> {
        let produces_owned_resource = Self::is_resource_ty(&expression.ty)
            && !matches!(
                expression.kind,
                HirExprKind::Borrow { .. } | HirExprKind::Function(_)
            );
        if matches!(result_use, ResultUse::Discard) && produces_owned_resource {
            let (_, destination) = self.prepare_temporary_destination(cursor, &expression.ty)?;
            Ok(ResultUse::Store(destination))
        } else {
            Ok(result_use)
        }
    }

    fn initialize_result(
        &mut self,
        cursor: CleanupCursor,
        result_use: &ResultUse,
    ) -> Result<(), Diagnostic> {
        if let ResultUse::Store(destination) = result_use {
            self.operation(cursor.block, CleanupOp::Init(destination.path))?;
        }
        Ok(())
    }

    fn transfer(
        &mut self,
        cursor: CleanupCursor,
        source: &CleanupDestination,
        destination: &CleanupDestination,
        kind: TransferKind,
    ) -> Result<(), Diagnostic> {
        self.operation(
            cursor.block,
            CleanupOp::Transfer {
                source: source.path,
                destination: destination.path,
                kind,
            },
        )
    }

    fn move_out(
        &mut self,
        cursor: CleanupCursor,
        path: CleanupMovePathId,
    ) -> Result<(), Diagnostic> {
        self.operation(cursor.block, CleanupOp::MoveOut(path))
    }

    fn walk_expr(
        &mut self,
        expression: &HirExpr,
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        // Mirror `FunctionEmitter::emit_expr`: operands still execute, but an
        // uninhabited expression can never install a normal result. Passing a
        // discard use prevents a fabricated Init/Transfer before the block is
        // made unreachable below.
        let uninhabited = self.program.is_uninhabited(&expression.ty);
        let result_use = if uninhabited {
            ResultUse::Discard
        } else {
            self.materialize_discarded_resource(expression, cursor, result_use)?
        };
        let result = match &expression.kind {
            HirExprKind::Integer(_)
            | HirExprKind::Bool(_)
            | HirExprKind::Unit
            | HirExprKind::LayoutQuery { .. }
            | HirExprKind::Global(_)
            | HirExprKind::Function(_) => {
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawBorrow { pointer, .. } => {
                let Some(cursor) = self.walk_expr(pointer, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::EraseContinuation { binding, .. } => {
                let cleanup_local = self.hir_locals.get(binding).copied().ok_or_else(|| {
                    self.diagnostic(format!(
                        "erased continuation refers to unmapped callable local {binding}"
                    ))
                })?;
                self.move_out(cursor, self.root_move_path(cleanup_local)?)?;
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::EraseEffectCallable { binding, .. } => {
                let cleanup_local = self.hir_locals.get(binding).copied().ok_or_else(|| {
                    self.diagnostic(format!(
                        "erased effect callable refers to unmapped callable local {binding}"
                    ))
                })?;
                self.move_out(cursor, self.root_move_path(cleanup_local)?)?;
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::InvokeContinuation {
                continuation,
                argument,
            } => {
                let (_, continuation_stage) =
                    self.prepare_temporary_destination(cursor, &continuation.ty)?;
                let Some(cursor) = self.walk_expr(
                    continuation,
                    cursor,
                    ResultUse::Store(continuation_stage.clone()),
                )?
                else {
                    return Ok(None);
                };
                let (_, argument_stage) =
                    self.prepare_temporary_destination(cursor, &argument.ty)?;
                let Some(cursor) =
                    self.walk_expr(argument, cursor, ResultUse::Store(argument_stage.clone()))?
                else {
                    return Ok(None);
                };
                self.move_out(cursor, continuation_stage.path)?;
                self.move_out(cursor, argument_stage.path)?;
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::InvokeEffectCallable {
                action,
                input,
                continuation,
            } => {
                let (_, action_stage) = self.prepare_temporary_destination(cursor, &action.ty)?;
                let Some(cursor) =
                    self.walk_expr(action, cursor, ResultUse::Store(action_stage.clone()))?
                else {
                    return Ok(None);
                };
                let (_, input_stage) = self.prepare_temporary_destination(cursor, &input.ty)?;
                let Some(cursor) =
                    self.walk_expr(input, cursor, ResultUse::Store(input_stage.clone()))?
                else {
                    return Ok(None);
                };
                let (_, continuation_stage) =
                    self.prepare_temporary_destination(cursor, &continuation.ty)?;
                let Some(cursor) = self.walk_expr(
                    continuation,
                    cursor,
                    ResultUse::Store(continuation_stage.clone()),
                )?
                else {
                    return Ok(None);
                };
                self.move_out(cursor, action_stage.path)?;
                self.move_out(cursor, input_stage.path)?;
                self.move_out(cursor, continuation_stage.path)?;
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::Array(elements) => {
                let mut current = Some(cursor);
                for (index, element) in elements.iter().enumerate() {
                    let element_use = match &result_use {
                        ResultUse::Store(destination) => {
                            ResultUse::Store(self.project_destination(
                                destination,
                                CleanupProjection::ConstantIndex(index as u64),
                            )?)
                        }
                        ResultUse::Discard => ResultUse::Discard,
                    };
                    current = match current {
                        Some(cursor) => self.walk_expr(element, cursor, element_use)?,
                        None => None,
                    };
                }
                if let Some(cursor) = current {
                    self.initialize_result(cursor, &result_use)?;
                }
                current
            }
            HirExprKind::Index {
                base, index, moves, ..
            } => {
                let (_, base_destination) = self.prepare_temporary_destination(cursor, &base.ty)?;
                let Some(cursor) =
                    self.walk_expr(base, cursor, ResultUse::Store(base_destination.clone()))?
                else {
                    return Ok(None);
                };
                let cursor = match index {
                    HirIndex::Static(_) => cursor,
                    HirIndex::Dynamic(index) => {
                        let (_, index_destination) =
                            self.prepare_temporary_destination(cursor, &index.ty)?;
                        let Some(cursor) =
                            self.walk_expr(index, cursor, ResultUse::Store(index_destination))?
                        else {
                            return Ok(None);
                        };
                        cursor
                    }
                };
                if *moves {
                    let HirIndex::Static(index) = index else {
                        return Err(self.diagnostic(
                            "resource array index reached cleanup without a constant path",
                        ));
                    };
                    let source = self.project_destination(
                        &base_destination,
                        CleanupProjection::ConstantIndex(*index),
                    )?;
                    match &result_use {
                        ResultUse::Store(destination) => {
                            self.transfer(cursor, &source, destination, TransferKind::Initialize)?
                        }
                        ResultUse::Discard => self.move_out(cursor, source.path)?,
                    }
                } else {
                    // Copy indexing leaves the staged array initialized; a
                    // runtime index is not a finite move path.
                    self.initialize_result(cursor, &result_use)?;
                }
                Some(cursor)
            }
            HirExprKind::Read { place, kind } => {
                if let Some(source_path) = self.move_path_for_hir_place(place)? {
                    if let ResultUse::Store(destination) = &result_use {
                        if *kind == HirReadKind::Move {
                            let source = CleanupDestination {
                                place: self
                                    .move_paths
                                    .iter()
                                    .find_map(|(place, path)| {
                                        (*path == source_path).then_some(place.clone())
                                    })
                                    .expect("HIR place move path is indexed"),
                                path: source_path,
                            };
                            self.transfer(cursor, &source, destination, TransferKind::Initialize)?;
                        } else {
                            self.initialize_result(cursor, &result_use)?;
                        }
                    } else if *kind == HirReadKind::Move {
                        self.move_out(cursor, source_path)?;
                    }
                } else {
                    self.initialize_result(cursor, &result_use)?;
                }
                Some(cursor)
            }
            HirExprKind::RawAddress { .. } => {
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawTrap => Some(cursor),
            HirExprKind::RawOffset { pointer, index } => {
                let Some(cursor) = self.walk_expr(pointer, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let Some(cursor) = self.walk_expr(index, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawLoad(pointer) => {
                let Some(cursor) = self.walk_expr(pointer, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawStore { pointer, value } => {
                let Some(cursor) = self.walk_expr(pointer, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let Some(cursor) = self.walk_expr(value, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawInit { pointer, value } => {
                let Some(cursor) = self.walk_expr(pointer, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let (_, stage) = self.prepare_temporary_destination(cursor, &value.ty)?;
                let Some(cursor) =
                    self.walk_expr(value, cursor, ResultUse::Store(stage.clone()))?
                else {
                    return Ok(None);
                };
                self.move_out(cursor, stage.path)?;
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawTake(pointer) => {
                let Some(cursor) = self.walk_expr(pointer, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::Forget(value) => {
                let (_, stage) = self.prepare_temporary_destination(cursor, &value.ty)?;
                let Some(cursor) =
                    self.walk_expr(value, cursor, ResultUse::Store(stage.clone()))?
                else {
                    return Ok(None);
                };
                self.move_out(cursor, stage.path)?;
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawAlloc { size, align } => {
                let Some(cursor) = self.walk_expr(size, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let Some(cursor) = self.walk_expr(align, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::RawDealloc {
                pointer,
                size,
                align,
            } => {
                let Some(cursor) = self.walk_expr(pointer, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let Some(cursor) = self.walk_expr(size, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let Some(cursor) = self.walk_expr(align, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::Unary(_, operand) => {
                let Some(cursor) = self.walk_expr(operand, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::Binary(left, operator @ (BinaryOp::And | BinaryOp::Or), right) => {
                self.walk_short_circuit(left, *operator, right, cursor, result_use)?
            }
            HirExprKind::Binary(left, _, right) => {
                let Some(cursor) = self.walk_expr(left, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let Some(cursor) = self.walk_expr(right, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::Assign {
                place,
                value,
                assignment,
                ..
            } => {
                let (_, stage) = self.prepare_temporary_destination(cursor, &value.ty)?;
                let Some(cursor) =
                    self.walk_expr(value, cursor, ResultUse::Store(stage.clone()))?
                else {
                    return Ok(None);
                };
                if let Some(path) = self.move_path_for_hir_place(place)? {
                    let kind = match assignment {
                        AssignmentKind::Initialize => TransferKind::Initialize,
                        AssignmentKind::Overwrite => TransferKind::Overwrite,
                        AssignmentKind::MaybeOverwrite => TransferKind::MaybeOverwrite,
                    };
                    let destination_place = self
                        .move_paths
                        .iter()
                        .find_map(|(place, candidate)| {
                            (*candidate == path).then_some(place.clone())
                        })
                        .expect("assignment move path is indexed");
                    self.transfer(
                        cursor,
                        &stage,
                        &CleanupDestination {
                            place: destination_place,
                            path,
                        },
                        kind,
                    )?;
                } else {
                    let alias = self.hir_locals.get(&place.local).copied().ok_or_else(|| {
                        self.diagnostic(format!(
                            "assignment place refers to unmapped HIR local {}",
                            place.local
                        ))
                    })?;
                    if matches!(
                        self.local_ownership.get(&alias),
                        Some(CleanupLocalOwnership::MutableBorrow)
                    ) {
                        self.move_out(cursor, stage.path)?;
                    }
                }
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::Call {
                arguments,
                consumed_callable,
                diverges,
                ..
            } => {
                let mut current = Some(cursor);
                let mut by_value = Vec::new();
                for argument in arguments {
                    let Some(cursor) = current else {
                        break;
                    };
                    current = match argument {
                        HirArgument::Copy(value) | HirArgument::Move(value) => {
                            let (_, stage) =
                                self.prepare_temporary_destination(cursor, &value.ty)?;
                            let result =
                                self.walk_expr(value, cursor, ResultUse::Store(stage.clone()))?;
                            if result.is_some() {
                                by_value.push(stage);
                            }
                            result
                        }
                        HirArgument::SharedBorrow(_)
                        | HirArgument::MutBorrow(_)
                        | HirArgument::CallableCaptureBorrow { .. } => Some(cursor),
                    };
                }
                if let Some(cursor) = current {
                    if let Some(source_local) = consumed_callable {
                        let cleanup_local =
                            self.hir_locals.get(source_local).copied().ok_or_else(|| {
                                self.diagnostic(format!(
                                    "consumed callable refers to unmapped HIR local {source_local}"
                                ))
                            })?;
                        self.move_out(cursor, self.root_move_path(cleanup_local)?)?;
                    }
                    for argument in by_value {
                        self.move_out(cursor, argument.path)?;
                    }
                    if *diverges {
                        self.terminate(cursor.block, CleanupTerminator::Unreachable)?;
                        None
                    } else {
                        self.initialize_result(cursor, &result_use)?;
                        Some(cursor)
                    }
                } else {
                    None
                }
            }
            HirExprKind::TailCall {
                arguments,
                consumed_callable,
                ..
            } => {
                let mut current = Some(cursor);
                let mut by_value = Vec::new();
                for argument in arguments {
                    let Some(cursor) = current else {
                        break;
                    };
                    current = match argument {
                        HirArgument::Copy(value) | HirArgument::Move(value) => {
                            let (_, stage) =
                                self.prepare_temporary_destination(cursor, &value.ty)?;
                            let result =
                                self.walk_expr(value, cursor, ResultUse::Store(stage.clone()))?;
                            if result.is_some() {
                                by_value.push(stage);
                            }
                            result
                        }
                        HirArgument::SharedBorrow(_)
                        | HirArgument::MutBorrow(_)
                        | HirArgument::CallableCaptureBorrow { .. } => Some(cursor),
                    };
                }
                if let Some(cursor) = current {
                    if let Some(source_local) = consumed_callable {
                        let cleanup_local =
                            self.hir_locals.get(source_local).copied().ok_or_else(|| {
                                self.diagnostic(format!(
                                    "tail continuation refers to unmapped callable local {source_local}"
                                ))
                            })?;
                        self.move_out(cursor, self.root_move_path(cleanup_local)?)?;
                    }
                    for argument in by_value {
                        self.move_out(cursor, argument.path)?;
                    }
                    if let Some(destination) = self.return_destination.clone() {
                        self.operation(cursor.block, CleanupOp::Init(destination.path))?;
                    }
                    self.emit_storage_dead_to(cursor.block, cursor.scope, self.root_scope)?;
                    let exited_scopes = self.exit_chain(cursor.scope, self.root_scope)?;
                    self.terminate(cursor.block, CleanupTerminator::Return { exited_scopes })?;
                }
                None
            }
            HirExprKind::TailInvokeContinuation {
                continuation,
                argument,
                ..
            } => {
                let (_, continuation_stage) =
                    self.prepare_temporary_destination(cursor, &continuation.ty)?;
                let Some(cursor) = self.walk_expr(
                    continuation,
                    cursor,
                    ResultUse::Store(continuation_stage.clone()),
                )?
                else {
                    return Ok(None);
                };
                let (_, argument_stage) =
                    self.prepare_temporary_destination(cursor, &argument.ty)?;
                let Some(cursor) =
                    self.walk_expr(argument, cursor, ResultUse::Store(argument_stage.clone()))?
                else {
                    return Ok(None);
                };
                self.move_out(cursor, continuation_stage.path)?;
                self.move_out(cursor, argument_stage.path)?;
                if let Some(destination) = self.return_destination.clone() {
                    self.operation(cursor.block, CleanupOp::Init(destination.path))?;
                }
                self.emit_storage_dead_to(cursor.block, cursor.scope, self.root_scope)?;
                let exited_scopes = self.exit_chain(cursor.scope, self.root_scope)?;
                self.terminate(cursor.block, CleanupTerminator::Return { exited_scopes })?;
                None
            }
            HirExprKind::IndirectCall {
                callee,
                arguments,
                diverges,
            } => {
                let Some(mut cursor) = self.walk_expr(callee, cursor, ResultUse::Discard)? else {
                    return Ok(None);
                };
                let mut by_value = Vec::new();
                for argument in arguments {
                    match argument {
                        HirArgument::Copy(value) | HirArgument::Move(value) => {
                            let (_, stage) =
                                self.prepare_temporary_destination(cursor, &value.ty)?;
                            let Some(next) =
                                self.walk_expr(value, cursor, ResultUse::Store(stage.clone()))?
                            else {
                                return Ok(None);
                            };
                            cursor = next;
                            by_value.push(stage);
                        }
                        HirArgument::SharedBorrow(_)
                        | HirArgument::MutBorrow(_)
                        | HirArgument::CallableCaptureBorrow { .. } => {}
                    }
                }
                for argument in by_value {
                    self.move_out(cursor, argument.path)?;
                }
                if *diverges {
                    self.terminate(cursor.block, CleanupTerminator::Unreachable)?;
                    None
                } else {
                    self.initialize_result(cursor, &result_use)?;
                    Some(cursor)
                }
            }
            HirExprKind::Partial { captures, .. } => {
                let mut current = Some(cursor);
                for (index, capture) in captures.iter().enumerate() {
                    let Some(cursor) = current else {
                        break;
                    };
                    let capture_use = match (&result_use, capture) {
                        (
                            ResultUse::Store(destination),
                            HirArgument::Copy(value) | HirArgument::Move(value),
                        ) => ResultUse::Store(self.register_callable_capture_destination(
                            destination,
                            index,
                            Some(&value.ty),
                        )?),
                        (
                            ResultUse::Store(destination),
                            HirArgument::SharedBorrow(_)
                            | HirArgument::MutBorrow(_)
                            | HirArgument::CallableCaptureBorrow { .. },
                        ) => ResultUse::Store(self.register_callable_capture_destination(
                            destination,
                            index,
                            None,
                        )?),
                        (ResultUse::Discard, _) => ResultUse::Discard,
                    };
                    current = match capture {
                        HirArgument::Copy(value) | HirArgument::Move(value) => {
                            self.walk_expr(value, cursor, capture_use)?
                        }
                        HirArgument::SharedBorrow(_)
                        | HirArgument::MutBorrow(_)
                        | HirArgument::CallableCaptureBorrow { .. } => {
                            self.initialize_result(cursor, &capture_use)?;
                            Some(cursor)
                        }
                    };
                }
                if let Some(cursor) = current {
                    self.initialize_result(cursor, &result_use)?;
                    Some(cursor)
                } else {
                    None
                }
            }
            HirExprKind::PartialCapture { .. } => {
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::LocalClosure(closure) => {
                let mut current = Some(cursor);
                for (index, capture) in closure.captures.iter().enumerate() {
                    let Some(cursor) = current else {
                        break;
                    };
                    let capture_use = match &result_use {
                        ResultUse::Store(destination) => {
                            ResultUse::Store(self.register_callable_capture_destination(
                                destination,
                                index,
                                capture.value.as_deref().map(|value| &value.ty),
                            )?)
                        }
                        ResultUse::Discard => ResultUse::Discard,
                    };
                    if let Some(value) = &capture.value {
                        current = self.walk_expr(value, cursor, capture_use)?;
                    } else {
                        self.initialize_result(cursor, &capture_use)?;
                    }
                }
                if let Some(cursor) = current {
                    self.initialize_result(cursor, &result_use)?;
                    Some(cursor)
                } else {
                    None
                }
            }
            HirExprKind::ConstructStruct { fields, .. } => {
                let mut current = Some(cursor);
                for (field_index, field) in fields {
                    let field_use = match &result_use {
                        ResultUse::Store(destination) => {
                            ResultUse::Store(self.project_destination(
                                destination,
                                CleanupProjection::Field(*field_index as u32),
                            )?)
                        }
                        ResultUse::Discard => ResultUse::Discard,
                    };
                    current = match current {
                        Some(cursor) => self.walk_expr(field, cursor, field_use)?,
                        None => None,
                    };
                }
                if let Some(cursor) = current {
                    self.initialize_result(cursor, &result_use)?;
                }
                current
            }
            HirExprKind::ConstructEnum {
                variant, fields, ..
            } => {
                let variant = u32::try_from(*variant)
                    .map_err(|_| self.diagnostic("enum variant does not fit in u32"))?;
                let variant_destination = match &result_use {
                    ResultUse::Store(destination) => {
                        self.operation(
                            cursor.block,
                            CleanupOp::SetDiscriminant {
                                destination: destination.path,
                                variant,
                            },
                        )?;
                        Some(self.project_destination(
                            destination,
                            CleanupProjection::Downcast(variant),
                        )?)
                    }
                    ResultUse::Discard => None,
                };
                let mut current = Some(cursor);
                for (field_index, field) in fields {
                    let field_use = match &variant_destination {
                        Some(destination) => ResultUse::Store(self.project_destination(
                            destination,
                            CleanupProjection::Field(*field_index as u32),
                        )?),
                        None => ResultUse::Discard,
                    };
                    current = match current {
                        Some(cursor) => self.walk_expr(field, cursor, field_use)?,
                        None => None,
                    };
                }
                if let (Some(cursor), Some(variant_destination)) =
                    (current, variant_destination.as_ref())
                {
                    self.operation(cursor.block, CleanupOp::Init(variant_destination.path))?;
                    self.initialize_result(cursor, &result_use)?;
                }
                current
            }
            HirExprKind::Field { base, index } => {
                let (base_local, base_destination) =
                    self.prepare_temporary_destination(cursor, &base.ty)?;
                let Some(cursor) =
                    self.walk_expr(base, cursor, ResultUse::Store(base_destination))?
                else {
                    return Ok(None);
                };
                let source_place = CleanupPlace::local(base_local)
                    .project(CleanupProjection::Field(*index as u32));
                let source = CleanupDestination {
                    path: self.lookup_move_path(&source_place)?,
                    place: source_place,
                };
                if let ResultUse::Store(destination) = &result_use {
                    self.transfer(cursor, &source, destination, TransferKind::Initialize)?;
                } else if uninhabited {
                    self.move_out(cursor, source.path)?;
                }
                Some(cursor)
            }
            HirExprKind::Borrow { .. } => {
                self.initialize_result(cursor, &result_use)?;
                Some(cursor)
            }
            HirExprKind::Block(statements, tail) => {
                self.walk_block(statements, tail.as_deref(), cursor, result_use)?
            }
            HirExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.walk_if(
                condition,
                then_branch,
                else_branch.as_deref(),
                cursor,
                result_use,
            )?,
            HirExprKind::Return(value) => {
                let mut current = Some(cursor);
                let mut stage = None;
                if let Some(value) = value {
                    let (_, destination) = self.prepare_temporary_destination(cursor, &value.ty)?;
                    stage = Some(destination.clone());
                    current = self.walk_expr(value, cursor, ResultUse::Store(destination))?;
                }
                if let Some(cursor) = current {
                    if let (Some(source), Some(destination)) =
                        (stage.as_ref(), self.return_destination.clone())
                    {
                        self.transfer(cursor, source, &destination, TransferKind::Initialize)?;
                    }
                    self.emit_storage_dead_to(cursor.block, cursor.scope, self.root_scope)?;
                    let exited_scopes = self.exit_chain(cursor.scope, self.root_scope)?;
                    self.terminate(cursor.block, CleanupTerminator::Return { exited_scopes })?;
                }
                None
            }
            HirExprKind::While { condition, body } => {
                self.walk_while(condition, body, cursor, result_use)?
            }
            HirExprKind::Loop { body } => self.walk_loop(body, cursor, result_use)?,
            HirExprKind::Break(value) => {
                let mut current = Some(cursor);
                let mut stage = None;
                if let Some(value) = value {
                    let (_, destination) = self.prepare_temporary_destination(cursor, &value.ty)?;
                    stage = Some(destination.clone());
                    current = self.walk_expr(value, cursor, ResultUse::Store(destination))?;
                }
                let Some(cursor) = current else {
                    return Ok(None);
                };
                let Some(frame) = self.loops.last().cloned() else {
                    return Err(self.diagnostic("HIR break has no cleanup loop frame"));
                };
                if let (Some(source), Some(destination)) =
                    (stage.as_ref(), frame.result_destination.as_ref())
                {
                    self.transfer(cursor, source, destination, TransferKind::Initialize)?;
                } else if value.is_none() {
                    if let Some(destination) = frame.result_destination.as_ref() {
                        self.initialize_result(cursor, &ResultUse::Store(destination.clone()))?;
                    }
                } else if value
                    .as_ref()
                    .is_some_and(|value| Self::is_resource_ty(&value.ty))
                {
                    return Err(self.diagnostic(
                        "resource-valued break has no cleanup loop result destination",
                    ));
                }
                self.goto_exiting(cursor, frame.break_target, frame.exit_scope)?;
                self.loops.last_mut().expect("loop frame exists").saw_break = true;
                None
            }
            HirExprKind::Continue => {
                let Some(frame) = self.loops.last().cloned() else {
                    return Err(self.diagnostic("HIR continue has no cleanup loop frame"));
                };
                self.goto_exiting(cursor, frame.continue_target, frame.continue_scope)?;
                None
            }
            HirExprKind::Match { scrutinee, arms } => {
                self.walk_match(scrutinee, arms, cursor, result_use)?
            }
        };

        if uninhabited {
            if let Some(cursor) = result {
                self.terminate(cursor.block, CleanupTerminator::Unreachable)?;
            }
            Ok(None)
        } else {
            Ok(result)
        }
    }

    fn walk_block(
        &mut self,
        statements: &[HirStmt],
        tail: Option<&HirExpr>,
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        let scope = self.new_scope(cursor.scope, CleanupScopeKind::Lexical)?;
        let entry = self.new_block(scope)?;
        self.terminate(
            cursor.block,
            CleanupTerminator::Goto(CleanupEdge::new(entry, Vec::new())),
        )?;
        let mut current = Some(CleanupCursor {
            block: entry,
            scope,
        });
        for statement in statements {
            let Some(cursor) = current else {
                break;
            };
            current = match statement {
                HirStmt::Let(binding) => self.walk_binding(binding, cursor)?,
                HirStmt::Expr(expression) => {
                    self.walk_expr(expression, cursor, ResultUse::Discard)?
                }
            };
        }
        if let (Some(cursor), Some(tail)) = (current, tail) {
            current = self.walk_expr(tail, cursor, result_use.clone())?;
        } else if let Some(cursor) = current {
            self.initialize_result(cursor, &result_use)?;
        }
        let Some(cursor) = current else {
            return Ok(None);
        };
        let continuation = self.new_block(
            self.scope_parents
                .get(&scope)
                .copied()
                .flatten()
                .expect("lexical scope has a parent"),
        )?;
        let parent = self.scope_parents[&scope].expect("lexical parent");
        self.goto_exiting(cursor, continuation, parent)?;
        Ok(Some(CleanupCursor {
            block: continuation,
            scope: parent,
        }))
    }

    fn walk_binding(
        &mut self,
        binding: &HirBinding,
        cursor: CleanupCursor,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        let ownership = match &binding.value.kind {
            HirExprKind::Borrow { mutable: true, .. } => CleanupLocalOwnership::MutableBorrow,
            HirExprKind::Borrow { mutable: false, .. } => CleanupLocalOwnership::SharedBorrow,
            _ if matches!(binding.ty, Ty::Reference { mutable: true, .. }) => {
                CleanupLocalOwnership::MutableBorrow
            }
            _ if matches!(binding.ty, Ty::Reference { mutable: false, .. }) => {
                CleanupLocalOwnership::SharedBorrow
            }
            _ => CleanupLocalOwnership::Owned,
        };
        let local = self.declare_source_local(
            cursor.scope,
            CleanupLocalKind::User,
            ownership,
            binding.mutable,
            CleanupSourceLocal {
                id: binding.id,
                name: &binding.name,
                ty: &binding.ty,
            },
        )?;
        self.operation(cursor.block, CleanupOp::StorageLive(local))?;
        let destination = if ownership == CleanupLocalOwnership::Owned {
            let place = CleanupPlace::local(local);
            Some(CleanupDestination {
                path: self.lookup_move_path(&place)?,
                place,
            })
        } else {
            None
        };
        self.walk_expr(
            &binding.value,
            cursor,
            destination.map_or(ResultUse::Discard, ResultUse::Store),
        )
    }

    fn walk_if(
        &mut self,
        condition: &HirExpr,
        then_branch: &HirExpr,
        else_branch: Option<&HirExpr>,
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        let (condition_local, condition_path) = self.prepare_temporary(cursor, &condition.ty)?;
        let condition_destination = CleanupDestination {
            place: CleanupPlace::local(condition_local),
            path: condition_path,
        };
        let Some(cursor) =
            self.walk_expr(condition, cursor, ResultUse::Store(condition_destination))?
        else {
            return Ok(None);
        };
        let then_block = self.new_block(cursor.scope)?;
        let else_block = self.new_block(cursor.scope)?;
        let join = self.new_block(cursor.scope)?;
        self.terminate(
            cursor.block,
            CleanupTerminator::Branch {
                condition: condition_local,
                then_edge: CleanupEdge::new(then_block, Vec::new()),
                else_edge: CleanupEdge::new(else_block, Vec::new()),
            },
        )?;
        let then_end = self.walk_expr(
            then_branch,
            CleanupCursor {
                block: then_block,
                scope: cursor.scope,
            },
            result_use.clone(),
        )?;
        if let Some(end) = then_end {
            self.terminate(
                end.block,
                CleanupTerminator::Goto(CleanupEdge::new(join, Vec::new())),
            )?;
        }
        let else_end = if let Some(else_branch) = else_branch {
            self.walk_expr(
                else_branch,
                CleanupCursor {
                    block: else_block,
                    scope: cursor.scope,
                },
                result_use.clone(),
            )?
        } else {
            self.initialize_result(
                CleanupCursor {
                    block: else_block,
                    scope: cursor.scope,
                },
                &result_use,
            )?;
            Some(CleanupCursor {
                block: else_block,
                scope: cursor.scope,
            })
        };
        if let Some(end) = else_end {
            self.terminate(
                end.block,
                CleanupTerminator::Goto(CleanupEdge::new(join, Vec::new())),
            )?;
        }
        if then_end.is_none() && else_end.is_none() {
            self.terminate(join, CleanupTerminator::Unreachable)?;
            Ok(None)
        } else {
            Ok(Some(CleanupCursor {
                block: join,
                scope: cursor.scope,
            }))
        }
    }

    fn walk_short_circuit(
        &mut self,
        left: &HirExpr,
        operator: BinaryOp,
        right: &HirExpr,
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        let (condition_local, condition_path) = self.prepare_temporary(cursor, &left.ty)?;
        let condition_destination = CleanupDestination {
            place: CleanupPlace::local(condition_local),
            path: condition_path,
        };
        let Some(cursor) = self.walk_expr(left, cursor, ResultUse::Store(condition_destination))?
        else {
            return Ok(None);
        };
        let right_block = self.new_block(cursor.scope)?;
        let join = self.new_block(cursor.scope)?;
        let (then_target, else_target) = match operator {
            BinaryOp::And => (right_block, join),
            BinaryOp::Or => (join, right_block),
            _ => unreachable!("only short-circuit operators reach cleanup CFG lowering"),
        };
        self.terminate(
            cursor.block,
            CleanupTerminator::Branch {
                condition: condition_local,
                then_edge: CleanupEdge::new(then_target, Vec::new()),
                else_edge: CleanupEdge::new(else_target, Vec::new()),
            },
        )?;
        if let Some(end) = self.walk_expr(
            right,
            CleanupCursor {
                block: right_block,
                scope: cursor.scope,
            },
            ResultUse::Discard,
        )? {
            self.terminate(
                end.block,
                CleanupTerminator::Goto(CleanupEdge::new(join, Vec::new())),
            )?;
        }
        let result = CleanupCursor {
            block: join,
            scope: cursor.scope,
        };
        self.initialize_result(result, &result_use)?;
        Ok(Some(result))
    }

    fn walk_while(
        &mut self,
        condition: &HirExpr,
        body: &HirExpr,
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        let loop_scope = self.new_scope(cursor.scope, CleanupScopeKind::Loop)?;
        let condition_scope = self.new_scope(loop_scope, CleanupScopeKind::Temporary)?;
        let body_scope = self.new_scope(loop_scope, CleanupScopeKind::Temporary)?;
        let condition_block = self.new_block(condition_scope)?;
        let body_block = self.new_block(body_scope)?;
        let false_exit = self.new_block(loop_scope)?;
        let after = self.new_block(cursor.scope)?;
        self.terminate(
            cursor.block,
            CleanupTerminator::Goto(CleanupEdge::new(condition_block, Vec::new())),
        )?;
        self.loops.push(CleanupLoopFrame {
            break_target: after,
            continue_target: condition_block,
            exit_scope: cursor.scope,
            continue_scope: loop_scope,
            result_destination: match &result_use {
                ResultUse::Store(destination) => Some(destination.clone()),
                ResultUse::Discard => None,
            },
            saw_break: false,
        });
        let condition_cursor = CleanupCursor {
            block: condition_block,
            scope: condition_scope,
        };
        let (condition_local, condition_path) =
            self.prepare_temporary(condition_cursor, &condition.ty)?;
        let condition_destination = CleanupDestination {
            place: CleanupPlace::local(condition_local),
            path: condition_path,
        };
        let condition_end = self.walk_expr(
            condition,
            condition_cursor,
            ResultUse::Store(condition_destination),
        )?;
        if let Some(condition_end) = condition_end {
            self.terminate(
                condition_end.block,
                CleanupTerminator::Branch {
                    condition: condition_local,
                    then_edge: CleanupEdge::new(body_block, vec![condition_scope]),
                    else_edge: CleanupEdge::new(false_exit, vec![condition_scope]),
                },
            )?;
            self.initialize_result(
                CleanupCursor {
                    block: false_exit,
                    scope: loop_scope,
                },
                &result_use,
            )?;
            self.goto_exiting(
                CleanupCursor {
                    block: false_exit,
                    scope: loop_scope,
                },
                after,
                cursor.scope,
            )?;
            if let Some(body_end) = self.walk_expr(
                body,
                CleanupCursor {
                    block: body_block,
                    scope: body_scope,
                },
                ResultUse::Discard,
            )? {
                self.goto_exiting(body_end, condition_block, loop_scope)?;
            }
        } else {
            self.terminate(body_block, CleanupTerminator::Unreachable)?;
            self.terminate(false_exit, CleanupTerminator::Unreachable)?;
        }
        let frame = self.loops.pop().expect("while loop frame exists");
        if condition_end.is_some() || frame.saw_break {
            Ok(Some(CleanupCursor {
                block: after,
                scope: cursor.scope,
            }))
        } else {
            self.terminate(after, CleanupTerminator::Unreachable)?;
            Ok(None)
        }
    }

    fn walk_loop(
        &mut self,
        body: &HirExpr,
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        let loop_scope = self.new_scope(cursor.scope, CleanupScopeKind::Loop)?;
        let body_scope = self.new_scope(loop_scope, CleanupScopeKind::Temporary)?;
        let body_block = self.new_block(body_scope)?;
        let after = self.new_block(cursor.scope)?;
        self.terminate(
            cursor.block,
            CleanupTerminator::Goto(CleanupEdge::new(body_block, Vec::new())),
        )?;
        self.loops.push(CleanupLoopFrame {
            break_target: after,
            continue_target: body_block,
            exit_scope: cursor.scope,
            continue_scope: loop_scope,
            result_destination: match result_use {
                ResultUse::Store(destination) => Some(destination),
                ResultUse::Discard => None,
            },
            saw_break: false,
        });
        if let Some(body_end) = self.walk_expr(
            body,
            CleanupCursor {
                block: body_block,
                scope: body_scope,
            },
            ResultUse::Discard,
        )? {
            self.goto_exiting(body_end, body_block, loop_scope)?;
        }
        let frame = self.loops.pop().expect("loop frame exists");
        if frame.saw_break {
            Ok(Some(CleanupCursor {
                block: after,
                scope: cursor.scope,
            }))
        } else {
            self.terminate(after, CleanupTerminator::Unreachable)?;
            Ok(None)
        }
    }

    fn commit_pattern_binding(
        &mut self,
        cursor: CleanupCursor,
        scrutinee_local: CleanupLocalId,
        scrutinee_ty: &Ty,
        matcher: HirMatcher,
        binding: &HirPatternBinding,
        destination: &CleanupDestination,
    ) -> Result<(), Diagnostic> {
        if !binding.moves {
            return self.operation(cursor.block, CleanupOp::Init(destination.path));
        }

        let mut source_place = CleanupPlace::local(scrutinee_local);
        if !binding.path.is_empty() {
            let HirMatcher::Variant(variant) = matcher else {
                return Err(self.diagnostic(
                    "moving a projected pattern binding requires a concrete enum variant",
                ));
            };
            let Ty::Enum(enum_name) = scrutinee_ty else {
                return Err(self.diagnostic("pattern transfer scrutinee is not an enum"));
            };
            let layout = self.program.enum_layout(enum_name).ok_or_else(|| {
                self.diagnostic(format!(
                    "missing enum layout `{enum_name}` for pattern transfer"
                ))
            })?;
            let variant_layout = layout.variants.get(variant).ok_or_else(|| {
                self.diagnostic(format!(
                    "invalid enum variant {variant} for pattern transfer"
                ))
            })?;
            let payload_start = 1 + variant_layout.payload_offset;
            let physical_field = binding.path[0];
            let field = physical_field.checked_sub(payload_start).ok_or_else(|| {
                self.diagnostic("pattern binding path precedes its variant payload")
            })?;
            if field >= variant_layout.fields.len() {
                return Err(self.diagnostic("pattern binding path exceeds its variant payload"));
            }
            let variant = u32::try_from(variant)
                .map_err(|_| self.diagnostic("pattern variant index does not fit in u32"))?;
            let field = u32::try_from(field)
                .map_err(|_| self.diagnostic("pattern field index does not fit in u32"))?;
            source_place = source_place
                .project(CleanupProjection::Downcast(variant))
                .project(CleanupProjection::Field(field));
            for field in &binding.path[1..] {
                let field = u32::try_from(*field)
                    .map_err(|_| self.diagnostic("nested pattern field does not fit in u32"))?;
                source_place = source_place.project(CleanupProjection::Field(field));
            }
        }
        let source = CleanupDestination {
            path: self.lookup_move_path(&source_place)?,
            place: source_place,
        };
        self.transfer(cursor, &source, destination, TransferKind::Initialize)
    }

    fn walk_match(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
        cursor: CleanupCursor,
        result_use: ResultUse,
    ) -> Result<Option<CleanupCursor>, Diagnostic> {
        let inspects_borrowed_storage = matches!(
            scrutinee.kind,
            HirExprKind::Read {
                kind: HirReadKind::Inspect,
                ..
            }
        );
        let (scrutinee_local, scrutinee_path) = if inspects_borrowed_storage {
            let local = self
                .builder
                .new_local(
                    cursor.scope,
                    CleanupLocalKind::Temporary,
                    CleanupLocalOwnership::Owned,
                    false,
                )
                .map_err(|error| self.diagnostic(error))?;
            self.track_local(
                cursor.scope,
                local,
                CleanupLocalKind::Temporary,
                CleanupLocalOwnership::Owned,
            );
            self.operation(cursor.block, CleanupOp::StorageLive(local))?;
            let place = CleanupPlace::local(local);
            let path = self.register_move_path(place.clone(), None, false)?;
            if let Ty::Enum(name) = &scrutinee.ty {
                let variant_count = self
                    .program
                    .enum_layout(name)
                    .ok_or_else(|| self.diagnostic(format!("missing enum layout `{name}`")))?
                    .variants
                    .len();
                for variant in 0..variant_count {
                    let variant = u32::try_from(variant)
                        .map_err(|_| self.diagnostic("inspection variant does not fit in u32"))?;
                    self.register_move_path(
                        place.clone().project(CleanupProjection::Downcast(variant)),
                        Some(path),
                        false,
                    )?;
                }
            }
            (local, path)
        } else {
            self.prepare_temporary(cursor, &scrutinee.ty)?
        };
        let scrutinee_destination = CleanupDestination {
            place: CleanupPlace::local(scrutinee_local),
            path: scrutinee_path,
        };
        let Some(cursor) =
            self.walk_expr(scrutinee, cursor, ResultUse::Store(scrutinee_destination))?
        else {
            return Ok(None);
        };
        let join = self.new_block(cursor.scope)?;
        let mut dispatch = cursor;
        let mut reaches_join = false;
        for arm in arms {
            let arm_scope = self.new_scope(cursor.scope, CleanupScopeKind::MatchArm)?;
            let arm_entry = self.new_block(arm_scope)?;
            let next_dispatch = self.new_block(cursor.scope)?;
            let (matcher_local, matcher_path) = self.prepare_temporary(dispatch, &Ty::Bool)?;
            self.operation(dispatch.block, CleanupOp::Init(matcher_path))?;
            self.terminate(
                dispatch.block,
                CleanupTerminator::Branch {
                    condition: matcher_local,
                    then_edge: CleanupEdge::new(arm_entry, Vec::new()),
                    else_edge: CleanupEdge::new(next_dispatch, Vec::new()),
                },
            )?;
            let arm_cursor = CleanupCursor {
                block: arm_entry,
                scope: arm_scope,
            };
            if let HirMatcher::Variant(variant) = arm.matcher {
                let variant = u32::try_from(variant)
                    .map_err(|_| self.diagnostic("match variant index does not fit in u32"))?;
                self.operation(
                    arm_cursor.block,
                    CleanupOp::AssumeDiscriminant {
                        source: scrutinee_path,
                        variant,
                    },
                )?;
            }
            let mut declared_bindings = Vec::new();
            for binding in &arm.bindings {
                let local = self.declare_source_local(
                    arm_scope,
                    CleanupLocalKind::Pattern,
                    CleanupLocalOwnership::Owned,
                    false,
                    CleanupSourceLocal {
                        id: binding.id,
                        name: &binding.name,
                        ty: &binding.ty,
                    },
                )?;
                self.operation(arm_cursor.block, CleanupOp::StorageLive(local))?;
                let path = self.root_move_path(local)?;
                let destination = CleanupDestination {
                    place: CleanupPlace::local(local),
                    path,
                };
                if arm.guard.is_none() || !binding.moves {
                    self.commit_pattern_binding(
                        arm_cursor,
                        scrutinee_local,
                        &scrutinee.ty,
                        arm.matcher,
                        binding,
                        &destination,
                    )?;
                }
                declared_bindings.push((binding, destination));
            }
            let body_start = if let Some(guard) = &arm.guard {
                let (guard_local, guard_path) = self.prepare_temporary(arm_cursor, &guard.ty)?;
                let guard_destination = CleanupDestination {
                    place: CleanupPlace::local(guard_local),
                    path: guard_path,
                };
                match self.walk_expr(guard, arm_cursor, ResultUse::Store(guard_destination))? {
                    Some(guard_end) => {
                        let body_block = self.new_block(arm_scope)?;
                        let guard_false = self.new_block(arm_scope)?;
                        self.terminate(
                            guard_end.block,
                            CleanupTerminator::Branch {
                                condition: guard_local,
                                then_edge: CleanupEdge::new(body_block, Vec::new()),
                                else_edge: CleanupEdge::new(guard_false, Vec::new()),
                            },
                        )?;
                        let body_cursor = CleanupCursor {
                            block: body_block,
                            scope: arm_scope,
                        };
                        for (binding, destination) in &declared_bindings {
                            if binding.moves {
                                self.commit_pattern_binding(
                                    body_cursor,
                                    scrutinee_local,
                                    &scrutinee.ty,
                                    arm.matcher,
                                    binding,
                                    destination,
                                )?;
                            }
                        }
                        self.goto_exiting(
                            CleanupCursor {
                                block: guard_false,
                                scope: arm_scope,
                            },
                            next_dispatch,
                            cursor.scope,
                        )?;
                        Some(body_cursor)
                    }
                    None => None,
                }
            } else {
                Some(arm_cursor)
            };
            if let Some(body_start) = body_start {
                if let Some(body_end) = self.walk_expr(&arm.body, body_start, result_use.clone())? {
                    self.goto_exiting(body_end, join, cursor.scope)?;
                    reaches_join = true;
                }
            }
            dispatch = CleanupCursor {
                block: next_dispatch,
                scope: cursor.scope,
            };
        }
        self.terminate(dispatch.block, CleanupTerminator::Unreachable)?;
        if reaches_join {
            Ok(Some(CleanupCursor {
                block: join,
                scope: cursor.scope,
            }))
        } else {
            self.terminate(join, CleanupTerminator::Unreachable)?;
            Ok(None)
        }
    }
}
