//! Type-independent ownership and cleanup control-flow skeleton.
//!
//! This module deliberately does not know about Salicin types. In particular,
//! it must not infer either `Copy` or `needs_drop`; those decisions belong to
//! semantic analysis and are inputs to later cleanup lowering. It also does
//! not contain drop glue or runtime drop flags.

#![allow(dead_code)]

use std::collections::HashSet;
use std::fmt;

macro_rules! index_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub(crate) struct $name(usize);

        impl $name {
            pub(crate) const fn index(self) -> usize {
                self.0
            }
        }
    };
}

index_id!(ScopeId);
index_id!(LocalId);
index_id!(MovePathId);
index_id!(BasicBlockId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    Root,
    FunctionBody,
    Lexical,
    Loop,
    MatchArm,
    Temporary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeData {
    pub(crate) id: ScopeId,
    pub(crate) parent: Option<ScopeId>,
    pub(crate) kind: ScopeKind,
    pub(crate) locals: Vec<LocalId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalKind {
    Argument,
    User,
    Pattern,
    Temporary,
    ReturnPlace,
}

/// Describes whether storage owns a value or aliases storage owned elsewhere.
///
/// This is intentionally not a `Copy` or `needs_drop` classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalOwnership {
    Owned,
    SharedBorrow,
    MutableBorrow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalDecl {
    pub(crate) id: LocalId,
    /// The numeric local identity assigned by HIR lowering, when this storage
    /// corresponds to a source-visible HIR local. Planner-created temporaries
    /// and the return place deliberately have no HIR identity.
    pub(crate) source_local: Option<usize>,
    pub(crate) debug_name: Option<String>,
    pub(crate) scope: ScopeId,
    pub(crate) kind: LocalKind,
    pub(crate) ownership: LocalOwnership,
    pub(crate) mutable: bool,
    pub(crate) declaration_order: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Projection {
    Deref,
    Field(u32),
    TupleIndex(u32),
    Index(LocalId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Place {
    pub(crate) local: LocalId,
    pub(crate) projections: Vec<Projection>,
}

impl Place {
    pub(crate) fn local(local: LocalId) -> Self {
        Self {
            local,
            projections: Vec::new(),
        }
    }

    pub(crate) fn project(mut self, projection: Projection) -> Self {
        self.projections.push(projection);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MovePath {
    pub(crate) id: MovePathId,
    pub(crate) place: Place,
    pub(crate) parent: Option<MovePathId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CleanupOp {
    StorageLive(LocalId),
    Init(MovePathId),
    MoveOut(MovePathId),
    Overwrite(MovePathId),
    StorageDead(LocalId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CleanupEdge {
    pub(crate) target: BasicBlockId,
    /// Scopes left by this edge, innermost first. The root scope is never
    /// included.
    pub(crate) exited_scopes: Vec<ScopeId>,
}

impl CleanupEdge {
    pub(crate) fn new(target: BasicBlockId, exited_scopes: Vec<ScopeId>) -> Self {
        Self {
            target,
            exited_scopes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Terminator {
    Goto(CleanupEdge),
    Branch {
        condition: LocalId,
        then_edge: CleanupEdge,
        else_edge: CleanupEdge,
    },
    /// Return has no CFG target. Its scope list must contain every non-root
    /// scope from the source block to the root, innermost first.
    Return {
        exited_scopes: Vec<ScopeId>,
    },
    Abort,
    Unreachable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BasicBlock {
    pub(crate) id: BasicBlockId,
    pub(crate) scope: ScopeId,
    pub(crate) operations: Vec<CleanupOp>,
    pub(crate) terminator: Option<Terminator>,
}

/// Capabilities which a later drop-aware lowering must add before this plan
/// can be used to emit destruction. Keeping these in the production plan is
/// intentional: the ownership CFG is useful now, but it must not accidentally
/// advertise support for cleanup cases whose values do not yet have stable
/// places or whose liveness needs a runtime flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingCapability {
    MaybeOverwrite {
        block: BasicBlockId,
        path: MovePathId,
    },
    MovePathStateDataflow {
        block: BasicBlockId,
        path: MovePathId,
    },
    UnmaterializedResourceResult {
        block: BasicBlockId,
        destination_expected: bool,
        description: String,
    },
    LoopBreakValueTransfer {
        source: BasicBlockId,
        target: BasicBlockId,
        description: String,
    },
    TemporaryStorageLiveness {
        block: BasicBlockId,
        local: LocalId,
    },
    BorrowedPlaceMutation {
        block: BasicBlockId,
        alias: LocalId,
        description: String,
    },
    PartialApplicationCapture {
        block: BasicBlockId,
        function: String,
    },
    LocalClosureCapture {
        block: BasicBlockId,
        function: String,
        fn_once: bool,
    },
    PatternBindingTransfer {
        block: BasicBlockId,
        binding: LocalId,
        guarded: bool,
    },
    MatchDispatch {
        block: BasicBlockId,
        arm_count: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CleanupPlan {
    pub(crate) root_scope: ScopeId,
    pub(crate) entry: BasicBlockId,
    pub(crate) scopes: Vec<ScopeData>,
    pub(crate) locals: Vec<LocalDecl>,
    pub(crate) move_paths: Vec<MovePath>,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) pending_capabilities: Vec<PendingCapability>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifyError {
    message: String,
}

impl VerifyError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl CleanupPlan {
    pub(crate) fn verify(&self) -> Result<(), Vec<VerifyError>> {
        let mut errors = Vec::new();

        self.verify_scopes(&mut errors);
        self.verify_locals(&mut errors);
        self.verify_move_paths(&mut errors);
        self.verify_blocks(&mut errors);
        self.verify_pending_capabilities(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn verify_scopes(&self, errors: &mut Vec<VerifyError>) {
        for (index, scope) in self.scopes.iter().enumerate() {
            if scope.id.index() != index {
                errors.push(VerifyError::new(format!(
                    "scope id/index mismatch: slot {index} contains {:?}",
                    scope.id
                )));
            }
        }

        let Some(root) = self.scopes.get(self.root_scope.index()) else {
            errors.push(VerifyError::new(format!(
                "root scope {:?} does not exist",
                self.root_scope
            )));
            return;
        };
        if root.id != self.root_scope {
            errors.push(VerifyError::new("root scope id does not match its slot"));
        }
        if root.parent.is_some() {
            errors.push(VerifyError::new("root scope must not have a parent"));
        }
        if root.kind != ScopeKind::Root {
            errors.push(VerifyError::new("root scope must have `Root` kind"));
        }

        for scope in &self.scopes {
            if scope.id != self.root_scope {
                if scope.kind == ScopeKind::Root {
                    errors.push(VerifyError::new(format!(
                        "non-root scope {:?} has `Root` kind",
                        scope.id
                    )));
                }
                match scope.parent {
                    Some(parent) if self.scopes.get(parent.index()).is_some() => {}
                    Some(parent) => errors.push(VerifyError::new(format!(
                        "scope {:?} has invalid parent {parent:?}",
                        scope.id
                    ))),
                    None => errors.push(VerifyError::new(format!(
                        "non-root scope {:?} has no parent",
                        scope.id
                    ))),
                }
            }

            let mut seen = HashSet::new();
            let mut cursor = Some(scope.id);
            while let Some(id) = cursor {
                let Some(data) = self.scopes.get(id.index()) else {
                    break;
                };
                if !seen.insert(id) {
                    errors.push(VerifyError::new(format!(
                        "scope parent cycle reaches {id:?} from {:?}",
                        scope.id
                    )));
                    break;
                }
                cursor = data.parent;
            }
        }
    }

    fn verify_locals(&self, errors: &mut Vec<VerifyError>) {
        let mut source_locals = HashSet::new();
        for (index, local) in self.locals.iter().enumerate() {
            if local.id.index() != index {
                errors.push(VerifyError::new(format!(
                    "local id/index mismatch: slot {index} contains {:?}",
                    local.id
                )));
            }
            if self.scopes.get(local.scope.index()).is_none() {
                errors.push(VerifyError::new(format!(
                    "local {:?} belongs to invalid scope {:?}",
                    local.id, local.scope
                )));
            }
            if let Some(source_local) = local.source_local {
                if !source_locals.insert(source_local) {
                    errors.push(VerifyError::new(format!(
                        "HIR local {source_local} is mapped to more than one cleanup local"
                    )));
                }
                if local.debug_name.is_none() {
                    errors.push(VerifyError::new(format!(
                        "HIR local {source_local} has no cleanup debug name"
                    )));
                }
            }
        }

        let mut memberships = vec![0_usize; self.locals.len()];
        for scope in &self.scopes {
            let mut in_scope = HashSet::new();
            for (order, local_id) in scope.locals.iter().copied().enumerate() {
                if !in_scope.insert(local_id) {
                    errors.push(VerifyError::new(format!(
                        "scope {:?} lists local {local_id:?} more than once",
                        scope.id
                    )));
                    continue;
                }
                let Some(local) = self.locals.get(local_id.index()) else {
                    errors.push(VerifyError::new(format!(
                        "scope {:?} lists invalid local {local_id:?}",
                        scope.id
                    )));
                    continue;
                };
                memberships[local_id.index()] += 1;
                if local.id != local_id || local.scope != scope.id {
                    errors.push(VerifyError::new(format!(
                        "scope/local ownership mismatch for {local_id:?}"
                    )));
                }
                if local.declaration_order != order {
                    errors.push(VerifyError::new(format!(
                        "local {local_id:?} has declaration order {}, expected {order}",
                        local.declaration_order
                    )));
                }
            }
        }

        for (index, count) in memberships.into_iter().enumerate() {
            if count != 1 {
                errors.push(VerifyError::new(format!(
                    "local {:?} appears in {count} scope local lists, expected exactly one",
                    LocalId(index)
                )));
            }
        }
    }

    fn verify_move_paths(&self, errors: &mut Vec<VerifyError>) {
        for (index, path) in self.move_paths.iter().enumerate() {
            if path.id.index() != index {
                errors.push(VerifyError::new(format!(
                    "move path id/index mismatch: slot {index} contains {:?}",
                    path.id
                )));
            }
            self.verify_place(&path.place, path.id, errors);
            if self
                .locals
                .get(path.place.local.index())
                .is_some_and(|local| local.ownership != LocalOwnership::Owned)
            {
                errors.push(VerifyError::new(format!(
                    "move path {:?} is rooted in borrow alias {:?}",
                    path.id, path.place.local
                )));
            }

            match path.parent {
                None if !path.place.projections.is_empty() => errors.push(VerifyError::new(
                    format!("projected move path {:?} has no parent", path.id),
                )),
                Some(parent_id) => {
                    let Some(parent) = self.move_paths.get(parent_id.index()) else {
                        errors.push(VerifyError::new(format!(
                            "move path {:?} has invalid parent {parent_id:?}",
                            path.id
                        )));
                        continue;
                    };
                    let Some((_, prefix)) = path.place.projections.split_last() else {
                        errors.push(VerifyError::new(format!(
                            "root move path {:?} must not have a parent",
                            path.id
                        )));
                        continue;
                    };
                    if parent.place.local != path.place.local
                        || parent.place.projections.as_slice() != prefix
                    {
                        errors.push(VerifyError::new(format!(
                            "move path parent {parent_id:?} is not the immediate place prefix of {:?}",
                            path.id
                        )));
                    }
                }
                None => {}
            }
        }
    }

    fn verify_place(&self, place: &Place, path_id: MovePathId, errors: &mut Vec<VerifyError>) {
        if self.locals.get(place.local.index()).is_none() {
            errors.push(VerifyError::new(format!(
                "move path {path_id:?} refers to invalid place local {:?}",
                place.local
            )));
        }
        for projection in &place.projections {
            if let Projection::Index(index_local) = projection {
                if self.locals.get(index_local.index()).is_none() {
                    errors.push(VerifyError::new(format!(
                        "move path {path_id:?} uses invalid index local {index_local:?}"
                    )));
                }
            }
        }
    }

    fn verify_blocks(&self, errors: &mut Vec<VerifyError>) {
        match self.blocks.get(self.entry.index()) {
            None => errors.push(VerifyError::new(format!(
                "entry block {:?} does not exist",
                self.entry
            ))),
            Some(entry) if entry.scope != self.root_scope => errors.push(VerifyError::new(
                "entry block must belong to the cleanup plan root scope",
            )),
            Some(_) => {}
        }

        for (index, block) in self.blocks.iter().enumerate() {
            if block.id.index() != index {
                errors.push(VerifyError::new(format!(
                    "basic block id/index mismatch: slot {index} contains {:?}",
                    block.id
                )));
            }
            if self.scopes.get(block.scope.index()).is_none() {
                errors.push(VerifyError::new(format!(
                    "basic block {:?} belongs to invalid scope {:?}",
                    block.id, block.scope
                )));
            }
            for operation in &block.operations {
                self.verify_operation(block.id, operation, errors);
            }
            let Some(terminator) = &block.terminator else {
                errors.push(VerifyError::new(format!(
                    "basic block {:?} has no terminator",
                    block.id
                )));
                continue;
            };
            self.verify_terminator(block, terminator, errors);
        }
    }

    fn verify_pending_capabilities(&self, errors: &mut Vec<VerifyError>) {
        for capability in &self.pending_capabilities {
            match capability {
                PendingCapability::MaybeOverwrite { block, path } => {
                    self.verify_pending_path_operation(
                        *block,
                        *path,
                        CleanupOp::Overwrite(*path),
                        "MaybeOverwrite",
                        errors,
                    );
                }
                PendingCapability::MovePathStateDataflow { block, path } => {
                    self.verify_pending_path_operation(
                        *block,
                        *path,
                        CleanupOp::MoveOut(*path),
                        "MovePathStateDataflow",
                        errors,
                    );
                }
                PendingCapability::UnmaterializedResourceResult { block, .. }
                | PendingCapability::PartialApplicationCapture { block, .. }
                | PendingCapability::LocalClosureCapture { block, .. } => {
                    self.verify_pending_block(*block, "cleanup capability", errors);
                }
                PendingCapability::LoopBreakValueTransfer { source, target, .. } => {
                    let source_block = self.verify_pending_block(
                        *source,
                        "loop break value transfer source",
                        errors,
                    );
                    let target_block = self.verify_pending_block(
                        *target,
                        "loop break value transfer target",
                        errors,
                    );
                    if let (Some(source_block), Some(target_block)) = (source_block, target_block) {
                        let has_matching_edge = matches!(
                            &source_block.terminator,
                            Some(Terminator::Goto(edge)) if edge.target == *target
                        );
                        if source == target
                            || !self.is_ancestor(target_block.scope, source_block.scope)
                            || !has_matching_edge
                        {
                            errors.push(VerifyError::new(format!(
                                "pending loop break transfer from {source:?} to {target:?} does not match an exit edge to an ancestor scope"
                            )));
                        }
                    }
                }
                PendingCapability::TemporaryStorageLiveness { block, local } => {
                    let block =
                        self.verify_pending_block(*block, "temporary storage liveness", errors);
                    let local = self.locals.get(local.index());
                    if local.is_none() {
                        errors.push(VerifyError::new(format!(
                            "pending temporary storage liveness refers to invalid local {local:?}"
                        )));
                    }
                    if let (Some(block), Some(local)) = (block, local) {
                        if local.kind != LocalKind::Temporary
                            || local.scope != block.scope
                            || !block.operations.contains(&CleanupOp::StorageLive(local.id))
                        {
                            errors.push(VerifyError::new(
                                "pending temporary storage liveness must refer to a temporary declared live in that block",
                            ));
                        }
                    }
                }
                PendingCapability::BorrowedPlaceMutation { block, alias, .. } => {
                    let block =
                        self.verify_pending_block(*block, "borrowed place mutation", errors);
                    let alias = self.locals.get(alias.index());
                    if alias.is_none() {
                        errors.push(VerifyError::new(format!(
                            "pending borrowed place mutation refers to invalid alias {alias:?}"
                        )));
                    }
                    if let (Some(block), Some(alias)) = (block, alias) {
                        if alias.ownership != LocalOwnership::MutableBorrow
                            || !self.is_ancestor(alias.scope, block.scope)
                        {
                            errors.push(VerifyError::new(
                                "pending borrowed place mutation must refer to a visible mutable borrow alias",
                            ));
                        }
                    }
                }
                PendingCapability::PatternBindingTransfer { block, binding, .. } => {
                    let block =
                        self.verify_pending_block(*block, "pattern binding transfer", errors);
                    let binding = self.locals.get(binding.index());
                    if binding.is_none() {
                        errors.push(VerifyError::new(format!(
                            "pending pattern binding transfer refers to invalid local {binding:?}"
                        )));
                    }
                    if let (Some(block), Some(binding)) = (block, binding) {
                        if binding.kind != LocalKind::Pattern
                            || binding.ownership != LocalOwnership::Owned
                            || binding.scope != block.scope
                            || !block
                                .operations
                                .contains(&CleanupOp::StorageLive(binding.id))
                        {
                            errors.push(VerifyError::new(
                                "pending pattern binding transfer must refer to an owned pattern declared live in that block",
                            ));
                        }
                        let root_path = self.move_paths.iter().find(|path| {
                            path.place.local == binding.id && path.place.projections.is_empty()
                        });
                        match root_path {
                            Some(path)
                                if block.operations.contains(&CleanupOp::Init(path.id)) => {}
                            _ => errors.push(VerifyError::new(
                                "pending pattern binding transfer must initialize the binding root path in that block",
                            )),
                        }
                    }
                }
                PendingCapability::MatchDispatch { block, .. } => {
                    let block_data = self.verify_pending_block(*block, "match dispatch", errors);
                    if let (Some(block_data), PendingCapability::MatchDispatch { arm_count, .. }) =
                        (block_data, capability)
                    {
                        let shape_matches = if *arm_count == 0 {
                            matches!(block_data.terminator, Some(Terminator::Unreachable))
                        } else {
                            matches!(block_data.terminator, Some(Terminator::Branch { .. }))
                        };
                        if !shape_matches {
                            errors.push(VerifyError::new(
                                "pending match dispatch does not match its arm count and terminator",
                            ));
                        }
                    }
                }
            }
        }
    }

    fn verify_pending_block(
        &self,
        block: BasicBlockId,
        capability: &str,
        errors: &mut Vec<VerifyError>,
    ) -> Option<&BasicBlock> {
        let result = self.blocks.get(block.index());
        if result.is_none() {
            errors.push(VerifyError::new(format!(
                "pending {capability} refers to invalid block {block:?}"
            )));
        }
        result
    }

    fn verify_pending_path_operation(
        &self,
        block: BasicBlockId,
        path: MovePathId,
        expected_operation: CleanupOp,
        capability: &str,
        errors: &mut Vec<VerifyError>,
    ) {
        let block_data = self.verify_pending_block(block, capability, errors);
        let path_data = self.move_paths.get(path.index());
        if path_data.is_none() {
            errors.push(VerifyError::new(format!(
                "pending {capability} refers to invalid move path {path:?}"
            )));
        }
        if let (Some(block_data), Some(path_data)) = (block_data, path_data) {
            if !block_data.operations.contains(&expected_operation) {
                errors.push(VerifyError::new(format!(
                    "pending {capability} has no matching operation in block {block:?}"
                )));
            }
            if let Some(local) = self.locals.get(path_data.place.local.index()) {
                if !self.is_ancestor(local.scope, block_data.scope) {
                    errors.push(VerifyError::new(format!(
                        "pending {capability} refers to a move path that is not visible in block {block:?}"
                    )));
                }
            }
        }
    }

    fn verify_operation(
        &self,
        block: BasicBlockId,
        operation: &CleanupOp,
        errors: &mut Vec<VerifyError>,
    ) {
        match *operation {
            CleanupOp::StorageLive(local) => {
                self.verify_operation_local(block, local, "StorageLive", false, errors);
            }
            CleanupOp::StorageDead(local) => {
                self.verify_operation_local(block, local, "StorageDead", true, errors);
            }
            CleanupOp::Init(path) => {
                self.verify_operation_path(block, path, "Init", true, errors);
            }
            CleanupOp::MoveOut(path) => {
                self.verify_operation_path(block, path, "MoveOut", true, errors);
            }
            CleanupOp::Overwrite(path) => {
                self.verify_operation_path(block, path, "Overwrite", true, errors);
            }
        }
    }

    fn verify_operation_local(
        &self,
        block: BasicBlockId,
        local_id: LocalId,
        operation: &str,
        requires_owned: bool,
        errors: &mut Vec<VerifyError>,
    ) {
        let Some(local) = self.locals.get(local_id.index()) else {
            errors.push(VerifyError::new(format!(
                "{operation} in {block:?} refers to invalid local {local_id:?}"
            )));
            return;
        };
        if requires_owned && local.ownership != LocalOwnership::Owned {
            errors.push(VerifyError::new(format!(
                "{operation} in {block:?} is an owned cleanup operation on borrow alias {local_id:?}"
            )));
        }
        if let Some(block_data) = self.blocks.get(block.index()) {
            let visible = if operation == "StorageLive" {
                local.scope == block_data.scope
            } else {
                self.is_ancestor(local.scope, block_data.scope)
            };
            if !visible {
                errors.push(VerifyError::new(format!(
                    "{operation} in {block:?} refers to local {local_id:?} outside its visible scope"
                )));
            }
        }
    }

    fn verify_operation_path(
        &self,
        block: BasicBlockId,
        path_id: MovePathId,
        operation: &str,
        requires_owned: bool,
        errors: &mut Vec<VerifyError>,
    ) {
        let Some(path) = self.move_paths.get(path_id.index()) else {
            errors.push(VerifyError::new(format!(
                "{operation} in {block:?} refers to invalid move path {path_id:?}"
            )));
            return;
        };
        let Some(local) = self.locals.get(path.place.local.index()) else {
            errors.push(VerifyError::new(format!(
                "{operation} in {block:?} has a place with invalid local {:?}",
                path.place.local
            )));
            return;
        };
        if requires_owned && local.ownership != LocalOwnership::Owned {
            errors.push(VerifyError::new(format!(
                "{operation} in {block:?} is an owned cleanup operation on borrow alias {:?}",
                local.id
            )));
        }
        if let Some(block_data) = self.blocks.get(block.index()) {
            if !self.is_ancestor(local.scope, block_data.scope) {
                errors.push(VerifyError::new(format!(
                    "{operation} in {block:?} refers to move path {path_id:?} outside its visible scope"
                )));
            }
        }
    }

    fn verify_terminator(
        &self,
        block: &BasicBlock,
        terminator: &Terminator,
        errors: &mut Vec<VerifyError>,
    ) {
        match terminator {
            Terminator::Goto(edge) => self.verify_edge(block, "goto", edge, errors),
            Terminator::Branch {
                condition,
                then_edge,
                else_edge,
            } => {
                match self.locals.get(condition.index()) {
                    None => errors.push(VerifyError::new(format!(
                        "branch in {:?} uses invalid condition local {condition:?}",
                        block.id
                    ))),
                    Some(local) if !self.is_ancestor(local.scope, block.scope) => {
                        errors.push(VerifyError::new(format!(
                            "branch in {:?} uses condition local {condition:?} outside its visible scope",
                            block.id
                        )));
                    }
                    Some(_) => {}
                }
                self.verify_edge(block, "branch then", then_edge, errors);
                self.verify_edge(block, "branch else", else_edge, errors);
            }
            Terminator::Return { exited_scopes } => {
                let expected = self.return_exit_chain(block.scope);
                if expected.as_deref() != Some(exited_scopes.as_slice()) {
                    errors.push(VerifyError::new(format!(
                        "return in {:?} has exit chain {exited_scopes:?}, expected {expected:?}",
                        block.id
                    )));
                }
            }
            Terminator::Abort | Terminator::Unreachable => {}
        }
    }

    fn verify_edge(
        &self,
        source: &BasicBlock,
        edge_kind: &str,
        edge: &CleanupEdge,
        errors: &mut Vec<VerifyError>,
    ) {
        let Some(target) = self.blocks.get(edge.target.index()) else {
            errors.push(VerifyError::new(format!(
                "{edge_kind} edge from {:?} has invalid target {:?}",
                source.id, edge.target
            )));
            return;
        };
        if self.scopes.get(source.scope.index()).is_none()
            || self.scopes.get(target.scope.index()).is_none()
        {
            return;
        }

        let mut cursor = source.scope;
        let mut chain_is_valid = true;
        for exited in &edge.exited_scopes {
            if *exited != cursor || cursor == self.root_scope {
                chain_is_valid = false;
                break;
            }
            let Some(parent) = self
                .scopes
                .get(cursor.index())
                .and_then(|scope| scope.parent)
            else {
                chain_is_valid = false;
                break;
            };
            cursor = parent;
        }

        let reaches_target = if edge.exited_scopes.is_empty() {
            self.is_ancestor(source.scope, target.scope)
        } else {
            cursor == target.scope
        };
        if !chain_is_valid || !reaches_target {
            errors.push(VerifyError::new(format!(
                "{edge_kind} edge from {:?} to {:?} has a non-contiguous scope exit chain {:?}",
                source.id, target.id, edge.exited_scopes
            )));
        }
    }

    fn return_exit_chain(&self, source: ScopeId) -> Option<Vec<ScopeId>> {
        let mut result = Vec::new();
        let mut cursor = source;
        let mut visited = HashSet::new();
        while cursor != self.root_scope {
            if !visited.insert(cursor) {
                return None;
            }
            result.push(cursor);
            cursor = self.scopes.get(cursor.index())?.parent?;
        }
        Some(result)
    }

    fn is_ancestor(&self, ancestor: ScopeId, descendant: ScopeId) -> bool {
        let mut cursor = Some(descendant);
        let mut visited = HashSet::new();
        while let Some(scope) = cursor {
            if scope == ancestor {
                return true;
            }
            if !visited.insert(scope) {
                return false;
            }
            cursor = self.scopes.get(scope.index()).and_then(|data| data.parent);
        }
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildError {
    message: String,
}

impl BuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

/// Incrementally constructs a cleanup plan while assigning stable typed IDs.
pub(crate) struct CleanupPlanBuilder {
    plan: CleanupPlan,
}

impl CleanupPlanBuilder {
    pub(crate) fn new() -> Self {
        let root_scope = ScopeId(0);
        let entry = BasicBlockId(0);
        Self {
            plan: CleanupPlan {
                root_scope,
                entry,
                scopes: vec![ScopeData {
                    id: root_scope,
                    parent: None,
                    kind: ScopeKind::Root,
                    locals: Vec::new(),
                }],
                locals: Vec::new(),
                move_paths: Vec::new(),
                blocks: vec![BasicBlock {
                    id: entry,
                    scope: root_scope,
                    operations: Vec::new(),
                    terminator: None,
                }],
                pending_capabilities: Vec::new(),
            },
        }
    }

    pub(crate) fn root_scope(&self) -> ScopeId {
        self.plan.root_scope
    }

    pub(crate) fn entry_block(&self) -> BasicBlockId {
        self.plan.entry
    }

    pub(crate) fn new_scope(
        &mut self,
        parent: ScopeId,
        kind: ScopeKind,
    ) -> Result<ScopeId, BuildError> {
        if self.plan.scopes.get(parent.index()).is_none() {
            return Err(BuildError::new(format!(
                "cannot create scope with invalid parent {parent:?}"
            )));
        }
        if kind == ScopeKind::Root {
            return Err(BuildError::new("only the plan root may have `Root` kind"));
        }
        let id = ScopeId(self.plan.scopes.len());
        self.plan.scopes.push(ScopeData {
            id,
            parent: Some(parent),
            kind,
            locals: Vec::new(),
        });
        Ok(id)
    }

    pub(crate) fn new_local(
        &mut self,
        scope: ScopeId,
        kind: LocalKind,
        ownership: LocalOwnership,
        mutable: bool,
    ) -> Result<LocalId, BuildError> {
        self.new_local_with_source(scope, kind, ownership, mutable, None, None)
    }

    pub(crate) fn new_source_local(
        &mut self,
        scope: ScopeId,
        kind: LocalKind,
        ownership: LocalOwnership,
        mutable: bool,
        source_local: usize,
        debug_name: impl Into<String>,
    ) -> Result<LocalId, BuildError> {
        self.new_local_with_source(
            scope,
            kind,
            ownership,
            mutable,
            Some(source_local),
            Some(debug_name.into()),
        )
    }

    fn new_local_with_source(
        &mut self,
        scope: ScopeId,
        kind: LocalKind,
        ownership: LocalOwnership,
        mutable: bool,
        source_local: Option<usize>,
        debug_name: Option<String>,
    ) -> Result<LocalId, BuildError> {
        let Some(scope_data) = self.plan.scopes.get_mut(scope.index()) else {
            return Err(BuildError::new(format!(
                "cannot create local in invalid scope {scope:?}"
            )));
        };
        let id = LocalId(self.plan.locals.len());
        let declaration_order = scope_data.locals.len();
        scope_data.locals.push(id);
        self.plan.locals.push(LocalDecl {
            id,
            source_local,
            debug_name,
            scope,
            kind,
            ownership,
            mutable,
            declaration_order,
        });
        Ok(id)
    }

    pub(crate) fn record_pending(&mut self, capability: PendingCapability) {
        self.plan.pending_capabilities.push(capability);
    }

    pub(crate) fn new_move_path(
        &mut self,
        place: Place,
        parent: Option<MovePathId>,
    ) -> Result<MovePathId, BuildError> {
        if self.plan.locals.get(place.local.index()).is_none() {
            return Err(BuildError::new(format!(
                "cannot create move path for invalid local {:?}",
                place.local
            )));
        }
        if let Some(parent_id) = parent {
            if self.plan.move_paths.get(parent_id.index()).is_none() {
                return Err(BuildError::new(format!(
                    "cannot create move path with invalid parent {parent_id:?}"
                )));
            }
        }
        let id = MovePathId(self.plan.move_paths.len());
        self.plan.move_paths.push(MovePath { id, place, parent });
        Ok(id)
    }

    pub(crate) fn new_block(&mut self, scope: ScopeId) -> Result<BasicBlockId, BuildError> {
        if self.plan.scopes.get(scope.index()).is_none() {
            return Err(BuildError::new(format!(
                "cannot create basic block in invalid scope {scope:?}"
            )));
        }
        let id = BasicBlockId(self.plan.blocks.len());
        self.plan.blocks.push(BasicBlock {
            id,
            scope,
            operations: Vec::new(),
            terminator: None,
        });
        Ok(id)
    }

    pub(crate) fn push_operation(
        &mut self,
        block: BasicBlockId,
        operation: CleanupOp,
    ) -> Result<(), BuildError> {
        let Some(block) = self.plan.blocks.get_mut(block.index()) else {
            return Err(BuildError::new(format!(
                "cannot append operation to invalid basic block {block:?}"
            )));
        };
        block.operations.push(operation);
        Ok(())
    }

    pub(crate) fn set_terminator(
        &mut self,
        block: BasicBlockId,
        terminator: Terminator,
    ) -> Result<(), BuildError> {
        let Some(block) = self.plan.blocks.get_mut(block.index()) else {
            return Err(BuildError::new(format!(
                "cannot terminate invalid basic block {block:?}"
            )));
        };
        if block.terminator.is_some() {
            return Err(BuildError::new(format!(
                "basic block {:?} already has a terminator",
                block.id
            )));
        }
        block.terminator = Some(terminator);
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<CleanupPlan, Vec<VerifyError>> {
        self.plan.verify()?;
        Ok(self.plan)
    }

    #[cfg(test)]
    fn into_unverified(self) -> CleanupPlan {
        self.plan
    }
}

#[cfg(test)]
mod tests {
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
    fn rejects_malformed_pending_capability_references() {
        let mut builder = CleanupPlanBuilder::new();
        let entry = builder.entry_block();
        builder.record_pending(PendingCapability::MaybeOverwrite {
            block: BasicBlockId(99),
            path: MovePathId(99),
        });
        builder
            .set_terminator(
                entry,
                Terminator::Return {
                    exited_scopes: vec![],
                },
            )
            .unwrap();

        let errors = messages(&builder.into_unverified());
        assert!(errors
            .iter()
            .any(|message| message.contains("invalid move path")));
        assert!(errors
            .iter()
            .any(|message| message.contains("invalid block")));
    }

    #[test]
    fn rejects_malformed_loop_break_transfer_blocks() {
        let mut builder = CleanupPlanBuilder::new();
        let entry = builder.entry_block();
        builder.record_pending(PendingCapability::LoopBreakValueTransfer {
            source: entry,
            target: BasicBlockId(99),
            description: "test transfer".into(),
        });
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
            .any(|message| message.contains("loop break value transfer")
                && message.contains("invalid block")));
    }

    #[test]
    fn rejects_pending_move_state_without_its_move_operation() {
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
        builder.record_pending(PendingCapability::MovePathStateDataflow { block: entry, path });
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
            .any(|message| message.contains("MovePathStateDataflow")
                && message.contains("no matching operation")));
    }

    #[test]
    fn rejects_pending_local_capabilities_with_the_wrong_provenance() {
        let mut builder = CleanupPlanBuilder::new();
        let entry = builder.entry_block();
        let ordinary = builder
            .new_local(
                builder.root_scope(),
                LocalKind::User,
                LocalOwnership::Owned,
                false,
            )
            .unwrap();
        let shared = builder
            .new_local(
                builder.root_scope(),
                LocalKind::Argument,
                LocalOwnership::SharedBorrow,
                false,
            )
            .unwrap();
        builder
            .push_operation(entry, CleanupOp::StorageLive(ordinary))
            .unwrap();
        builder.record_pending(PendingCapability::TemporaryStorageLiveness {
            block: entry,
            local: ordinary,
        });
        builder.record_pending(PendingCapability::BorrowedPlaceMutation {
            block: entry,
            alias: shared,
            description: "test mutation".into(),
        });
        builder
            .set_terminator(
                entry,
                Terminator::Return {
                    exited_scopes: vec![],
                },
            )
            .unwrap();

        let errors = messages(&builder.into_unverified());
        assert!(errors
            .iter()
            .any(|message| message.contains("temporary declared live")));
        assert!(errors
            .iter()
            .any(|message| message.contains("mutable borrow alias")));
    }

    #[test]
    fn rejects_pattern_transfer_without_declaration_and_initialization_operations() {
        let mut builder = CleanupPlanBuilder::new();
        let entry = builder.entry_block();
        let binding = builder
            .new_local(
                builder.root_scope(),
                LocalKind::Pattern,
                LocalOwnership::Owned,
                false,
            )
            .unwrap();
        builder.new_move_path(Place::local(binding), None).unwrap();
        builder.record_pending(PendingCapability::PatternBindingTransfer {
            block: entry,
            binding,
            guarded: false,
        });
        builder
            .set_terminator(
                entry,
                Terminator::Return {
                    exited_scopes: vec![],
                },
            )
            .unwrap();

        let errors = messages(&builder.into_unverified());
        assert!(errors
            .iter()
            .any(|message| message.contains("declared live")));
        assert!(errors
            .iter()
            .any(|message| message.contains("initialize the binding root path")));
    }

    #[test]
    fn rejects_non_root_entry_and_mismatched_match_dispatch_shape() {
        let mut builder = CleanupPlanBuilder::new();
        let child = builder
            .new_scope(builder.root_scope(), ScopeKind::FunctionBody)
            .unwrap();
        let entry = builder.entry_block();
        builder.record_pending(PendingCapability::MatchDispatch {
            block: entry,
            arm_count: 1,
        });
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
        assert!(errors
            .iter()
            .any(|message| message.contains("match dispatch") && message.contains("terminator")));
    }
}
