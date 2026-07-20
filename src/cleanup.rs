//! Type-independent ownership and cleanup control-flow skeleton.
//!
//! This module deliberately does not know about Salicin types. In particular,
//! it must not infer either `Copy` or `needs_drop`; those decisions belong to
//! semantic analysis and are inputs to later cleanup lowering. It also does
//! not contain drop glue or runtime drop flags.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};
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
    ConstantIndex(u64),
    Index(LocalId),
    Downcast(u32),
    Capture(u32),
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
    /// Atomically consumes a fully initialized source place and installs it
    /// into a destination. The kind describes the state of the destination
    /// before the transfer; every transfer leaves the destination initialized.
    Transfer {
        source: MovePathId,
        destination: MovePathId,
        kind: TransferKind,
    },
    /// Records that an enum destination has selected a variant before any of
    /// that variant's fields become initialized.
    SetDiscriminant {
        destination: MovePathId,
        variant: u32,
    },
    StorageDead(LocalId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferKind {
    Initialize,
    Overwrite,
    MaybeOverwrite,
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
    TemporaryStorageLiveness {
        block: BasicBlockId,
        local: LocalId,
    },
    BorrowedPlaceMutation {
        block: BasicBlockId,
        alias: LocalId,
        source: MovePathId,
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
struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    fn new(bit_count: usize) -> Self {
        Self {
            words: vec![0; bit_count.div_ceil(u64::BITS as usize)],
        }
    }

    fn contains(&self, bit: usize) -> bool {
        self.words
            .get(bit / u64::BITS as usize)
            .is_some_and(|word| word & (1_u64 << (bit % u64::BITS as usize)) != 0)
    }

    fn insert(&mut self, bit: usize) {
        self.words[bit / u64::BITS as usize] |= 1_u64 << (bit % u64::BITS as usize);
    }

    fn remove(&mut self, bit: usize) {
        self.words[bit / u64::BITS as usize] &= !(1_u64 << (bit % u64::BITS as usize));
    }

    fn union_with(&mut self, other: &Self) {
        for (word, other) in self.words.iter_mut().zip(&other.words) {
            *word |= other;
        }
    }

    fn intersect_with(&mut self, other: &Self) {
        for (word, other) in self.words.iter_mut().zip(&other.words) {
            *word &= other;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiscriminantState {
    may_be_unset: bool,
    possible_variants: Vec<u32>,
}

impl DiscriminantState {
    fn unset() -> Self {
        Self {
            may_be_unset: true,
            possible_variants: Vec::new(),
        }
    }

    fn set_known(&mut self, variant: u32) {
        self.may_be_unset = false;
        self.possible_variants.clear();
        self.possible_variants.push(variant);
    }

    fn include_variant(&mut self, variant: u32) {
        match self.possible_variants.binary_search(&variant) {
            Ok(_) => {}
            Err(index) => self.possible_variants.insert(index, variant),
        }
    }

    fn join_with(&mut self, other: &Self) {
        self.may_be_unset |= other.may_be_unset;
        for variant in &other.possible_variants {
            self.include_variant(*variant);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveState {
    reachable: bool,
    may_init: BitSet,
    must_init: BitSet,
    discriminants: Vec<DiscriminantState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathInitialization {
    Uninitialized,
    MaybeOrPartial,
    Initialized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CompletionRule {
    Explicit,
    AllChildren,
    EnumRoot(Vec<(u32, MovePathId)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MovePathTopology {
    children: Vec<Vec<MovePathId>>,
    completion: Vec<CompletionRule>,
    discriminant_index: Vec<Option<usize>>,
    discriminant_count: usize,
    roots_by_local: Vec<Option<MovePathId>>,
    preorder_paths: Vec<MovePathId>,
    preorder_index: Vec<usize>,
    subtree_end: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveStateAnalysis {
    computed: bool,
    topology: MovePathTopology,
    block_entry: Vec<MoveState>,
    block_exit: Vec<MoveState>,
}

impl MoveStateAnalysis {
    fn uncomputed() -> Self {
        Self {
            computed: false,
            topology: MovePathTopology {
                children: Vec::new(),
                completion: Vec::new(),
                discriminant_index: Vec::new(),
                discriminant_count: 0,
                roots_by_local: Vec::new(),
                preorder_paths: Vec::new(),
                preorder_index: Vec::new(),
                subtree_end: Vec::new(),
            },
            block_entry: Vec::new(),
            block_exit: Vec::new(),
        }
    }

    pub(crate) fn block_entry(&self, block: BasicBlockId) -> Option<&MoveState> {
        self.computed
            .then(|| self.block_entry.get(block.index()))
            .flatten()
    }

    pub(crate) fn block_exit(&self, block: BasicBlockId) -> Option<&MoveState> {
        self.computed
            .then(|| self.block_exit.get(block.index()))
            .flatten()
    }
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
    pub(crate) move_state: MoveStateAnalysis,
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

impl MovePathTopology {
    fn new(plan: &CleanupPlan) -> Self {
        let mut children = vec![Vec::new(); plan.move_paths.len()];
        let mut roots_by_local = vec![None; plan.locals.len()];
        for path in &plan.move_paths {
            if let Some(parent) = path.parent {
                children[parent.index()].push(path.id);
            } else {
                roots_by_local[path.place.local.index()] = Some(path.id);
            }
        }
        for child_ids in &mut children {
            child_ids.sort_unstable();
        }

        let completion: Vec<CompletionRule> = children
            .iter()
            .enumerate()
            .map(|(index, child_ids)| {
                if child_ids.is_empty() {
                    return CompletionRule::Explicit;
                }
                let mut variants = Vec::new();
                for child in child_ids {
                    if let Some(Projection::Downcast(variant)) =
                        plan.move_paths[child.index()].place.projections.last()
                    {
                        variants.push((*variant, *child));
                    }
                }
                if variants.len() == child_ids.len() {
                    variants.sort_unstable_by_key(|(variant, _)| *variant);
                    CompletionRule::EnumRoot(variants)
                } else {
                    debug_assert!(variants.is_empty(), "mixed enum and aggregate children");
                    let _ = index;
                    CompletionRule::AllChildren
                }
            })
            .collect();
        let mut discriminant_count = 0_usize;
        let discriminant_index = completion
            .iter()
            .map(|rule| {
                if matches!(rule, CompletionRule::EnumRoot(_)) {
                    let index = discriminant_count;
                    discriminant_count += 1;
                    Some(index)
                } else {
                    None
                }
            })
            .collect();

        let mut preorder_paths = Vec::with_capacity(plan.move_paths.len());
        let mut preorder_index = vec![0; plan.move_paths.len()];
        let mut subtree_end = vec![0; plan.move_paths.len()];
        for root in roots_by_local.iter().flatten().copied() {
            let mut stack = vec![(root, false)];
            while let Some((path, exiting)) = stack.pop() {
                if exiting {
                    subtree_end[path.index()] = preorder_paths.len();
                    continue;
                }
                preorder_index[path.index()] = preorder_paths.len();
                preorder_paths.push(path);
                stack.push((path, true));
                for child in children[path.index()].iter().rev() {
                    stack.push((*child, false));
                }
            }
        }
        debug_assert_eq!(preorder_paths.len(), plan.move_paths.len());

        Self {
            children,
            completion,
            discriminant_index,
            discriminant_count,
            roots_by_local,
            preorder_paths,
            preorder_index,
            subtree_end,
        }
    }

    fn subtree(&self, path: MovePathId) -> impl Iterator<Item = MovePathId> + '_ {
        let start = self.preorder_index[path.index()];
        self.preorder_paths[start..self.subtree_end[path.index()]]
            .iter()
            .copied()
    }

    fn enum_variants(&self, path: MovePathId) -> Option<&[(u32, MovePathId)]> {
        match &self.completion[path.index()] {
            CompletionRule::EnumRoot(variants) => Some(variants),
            CompletionRule::Explicit | CompletionRule::AllChildren => None,
        }
    }

    fn transfer_shapes_match(
        &self,
        plan: &CleanupPlan,
        source: MovePathId,
        destination: MovePathId,
    ) -> bool {
        let mut stack = vec![(source, destination)];
        while let Some((source, destination)) = stack.pop() {
            let source_children = &self.children[source.index()];
            let destination_children = &self.children[destination.index()];
            let is_callable_environment = |children: &[MovePathId]| {
                !children.is_empty()
                    && children.iter().all(|child| {
                        matches!(
                            plan.move_paths[child.index()].place.projections.last(),
                            Some(Projection::Capture(_))
                        )
                    })
            };
            if is_callable_environment(source_children)
                || is_callable_environment(destination_children)
            {
                // Function types do not yet encode their environment layout.
                // Concrete capture paths remain explicitly pending and may
                // differ across otherwise compatible callable storage.
                continue;
            }
            if source_children.len() != destination_children.len() {
                return false;
            }
            let destination_by_projection: HashMap<&Projection, MovePathId> = destination_children
                .iter()
                .map(|child| {
                    (
                        plan.move_paths[child.index()]
                            .place
                            .projections
                            .last()
                            .expect("a child path has a final projection"),
                        *child,
                    )
                })
                .collect();
            for source_child in source_children {
                let projection = plan.move_paths[source_child.index()]
                    .place
                    .projections
                    .last()
                    .expect("a child path has a final projection");
                let Some(destination_child) = destination_by_projection.get(projection) else {
                    return false;
                };
                stack.push((*source_child, *destination_child));
            }
        }
        true
    }
}

#[derive(Clone, Copy)]
enum InitializationCertainty {
    May,
    Must,
}

impl MoveState {
    fn new(reachable: bool, topology: &MovePathTopology) -> Self {
        let discriminants = vec![DiscriminantState::unset(); topology.discriminant_count];
        Self {
            reachable,
            may_init: BitSet::new(topology.children.len()),
            must_init: BitSet::new(topology.children.len()),
            discriminants,
        }
    }

    fn join_from(&mut self, incoming: &Self) -> bool {
        if !incoming.reachable {
            return false;
        }
        if !self.reachable {
            *self = incoming.clone();
            return true;
        }
        let previous = self.clone();
        self.may_init.union_with(&incoming.may_init);
        self.must_init.intersect_with(&incoming.must_init);
        for (state, other) in self.discriminants.iter_mut().zip(&incoming.discriminants) {
            state.join_with(other);
        }
        *self != previous
    }

    fn clear_subtree_only(&mut self, path: MovePathId, topology: &MovePathTopology) {
        for descendant in topology.subtree(path) {
            self.may_init.remove(descendant.index());
            self.must_init.remove(descendant.index());
            if let Some(index) = topology.discriminant_index[descendant.index()] {
                self.discriminants[index] = DiscriminantState::unset();
            }
        }
    }

    fn clear_path(&mut self, path: MovePathId, plan: &CleanupPlan, topology: &MovePathTopology) {
        self.clear_subtree_only(path, topology);
        let mut parent = plan.move_paths[path.index()].parent;
        while let Some(path) = parent {
            self.may_init.remove(path.index());
            self.must_init.remove(path.index());
            parent = plan.move_paths[path.index()].parent;
        }
    }

    fn clear_local(&mut self, local: LocalId, topology: &MovePathTopology) {
        if let Some(root) = topology
            .roots_by_local
            .get(local.index())
            .copied()
            .flatten()
        {
            self.clear_subtree_only(root, topology);
        }
    }

    fn mark_whole(
        &mut self,
        root: MovePathId,
        certainty: InitializationCertainty,
        topology: &MovePathTopology,
    ) {
        let mut stack = vec![(root, certainty)];
        while let Some((path, certainty)) = stack.pop() {
            self.may_init.insert(path.index());
            match certainty {
                InitializationCertainty::Must => self.must_init.insert(path.index()),
                InitializationCertainty::May => self.must_init.remove(path.index()),
            }
            match &topology.completion[path.index()] {
                CompletionRule::Explicit => {}
                CompletionRule::AllChildren => {
                    for child in &topology.children[path.index()] {
                        stack.push((*child, certainty));
                    }
                }
                CompletionRule::EnumRoot(variants) => {
                    let all_variants: Vec<_> =
                        variants.iter().map(|(variant, _)| *variant).collect();
                    let possible = {
                        let index = topology.discriminant_index[path.index()]
                            .expect("enum root has discriminant state");
                        let discriminant = &mut self.discriminants[index];
                        match certainty {
                            InitializationCertainty::Must => {
                                if discriminant.may_be_unset {
                                    for variant in &all_variants {
                                        discriminant.include_variant(*variant);
                                    }
                                }
                                discriminant.may_be_unset = false;
                            }
                            InitializationCertainty::May => {
                                discriminant.may_be_unset = true;
                                for variant in &all_variants {
                                    discriminant.include_variant(*variant);
                                }
                            }
                        }
                        discriminant.possible_variants.clone()
                    };
                    let singleton =
                        matches!(certainty, InitializationCertainty::Must) && possible.len() == 1;
                    for (variant, child) in variants {
                        if possible.binary_search(variant).is_ok() {
                            stack.push((
                                *child,
                                if singleton {
                                    InitializationCertainty::Must
                                } else {
                                    InitializationCertainty::May
                                },
                            ));
                        } else {
                            self.clear_subtree_only(*child, topology);
                        }
                    }
                }
            }
        }
    }

    fn recompose_ancestors(
        &mut self,
        path: MovePathId,
        plan: &CleanupPlan,
        topology: &MovePathTopology,
    ) {
        let mut parent = plan.move_paths[path.index()].parent;
        while let Some(path) = parent {
            match &topology.completion[path.index()] {
                CompletionRule::Explicit => {}
                CompletionRule::AllChildren => {
                    let all_must = topology.children[path.index()]
                        .iter()
                        .all(|child| self.must_init.contains(child.index()));
                    let all_may = topology.children[path.index()]
                        .iter()
                        .all(|child| self.may_init.contains(child.index()));
                    if all_must {
                        self.may_init.insert(path.index());
                        self.must_init.insert(path.index());
                    } else {
                        self.must_init.remove(path.index());
                        if all_may {
                            self.may_init.insert(path.index());
                        } else {
                            self.may_init.remove(path.index());
                        }
                    }
                }
                CompletionRule::EnumRoot(variants) => {
                    let active_child = topology.discriminant_index[path.index()]
                        .map(|index| &self.discriminants[index])
                        .filter(|state| !state.may_be_unset && state.possible_variants.len() == 1)
                        .and_then(|state| {
                            let active = state.possible_variants[0];
                            variants
                                .binary_search_by_key(&active, |(variant, _)| *variant)
                                .ok()
                                .map(|index| variants[index].1)
                        });
                    if let Some(active_child) = active_child {
                        if self.must_init.contains(active_child.index()) {
                            self.may_init.insert(path.index());
                            self.must_init.insert(path.index());
                        } else {
                            self.must_init.remove(path.index());
                            if self.may_init.contains(active_child.index()) {
                                self.may_init.insert(path.index());
                            } else {
                                self.may_init.remove(path.index());
                            }
                        }
                    }
                }
            }
            parent = plan.move_paths[path.index()].parent;
        }
    }

    fn initialize_path(
        &mut self,
        path: MovePathId,
        plan: &CleanupPlan,
        topology: &MovePathTopology,
    ) {
        self.mark_whole(path, InitializationCertainty::Must, topology);
        self.recompose_ancestors(path, plan, topology);
    }

    fn set_discriminant(
        &mut self,
        destination: MovePathId,
        variant: u32,
        plan: &CleanupPlan,
        topology: &MovePathTopology,
    ) {
        self.clear_path(destination, plan, topology);
        if let Some(index) = topology.discriminant_index[destination.index()] {
            self.discriminants[index].set_known(variant);
        }
    }

    fn apply_operation(
        &mut self,
        operation: &CleanupOp,
        plan: &CleanupPlan,
        topology: &MovePathTopology,
    ) {
        match *operation {
            CleanupOp::StorageLive(local) | CleanupOp::StorageDead(local) => {
                self.clear_local(local, topology);
            }
            CleanupOp::Init(path) => {
                self.initialize_path(path, plan, topology);
            }
            CleanupOp::Overwrite(path) => {
                self.clear_path(path, plan, topology);
                self.initialize_path(path, plan, topology);
            }
            CleanupOp::MoveOut(path) => self.clear_path(path, plan, topology),
            CleanupOp::Transfer {
                source,
                destination,
                ..
            } => {
                let source_discriminant = topology.discriminant_index[source.index()]
                    .map(|index| self.discriminants[index].clone());
                self.clear_path(source, plan, topology);
                self.clear_path(destination, plan, topology);
                if let (Some(source), Some(destination_index)) = (
                    source_discriminant,
                    topology.discriminant_index[destination.index()],
                ) {
                    self.discriminants[destination_index] = source;
                }
                self.initialize_path(destination, plan, topology);
            }
            CleanupOp::SetDiscriminant {
                destination,
                variant,
            } => self.set_discriminant(destination, variant, plan, topology),
        }
    }

    fn path_initialization(
        &self,
        path: MovePathId,
        topology: &MovePathTopology,
    ) -> PathInitialization {
        if self.must_init.contains(path.index()) {
            return PathInitialization::Initialized;
        }
        let has_initialized_part = topology.subtree(path).any(|descendant| {
            self.may_init.contains(descendant.index())
                || topology.discriminant_index[descendant.index()].is_some_and(|index| {
                    let discriminant = &self.discriminants[index];
                    !discriminant.may_be_unset || !discriminant.possible_variants.is_empty()
                })
        });
        if has_initialized_part {
            PathInitialization::MaybeOrPartial
        } else {
            PathInitialization::Uninitialized
        }
    }

    fn inactive_downcast(
        &self,
        path: MovePathId,
        plan: &CleanupPlan,
        topology: &MovePathTopology,
    ) -> Option<(MovePathId, u32, bool, Vec<u32>)> {
        let mut cursor = Some(path);
        while let Some(current) = cursor {
            let parent = plan.move_paths[current.index()].parent;
            if let (Some(Projection::Downcast(required)), Some(enum_root)) = (
                plan.move_paths[current.index()].place.projections.last(),
                parent,
            ) {
                let index = topology.discriminant_index[enum_root.index()]
                    .expect("a downcast parent is an enum root");
                let state = &self.discriminants[index];
                if state.may_be_unset || state.possible_variants.as_slice() != [*required] {
                    return Some((
                        enum_root,
                        *required,
                        state.may_be_unset,
                        state.possible_variants.clone(),
                    ));
                }
            }
            cursor = parent;
        }
        None
    }
}

impl MoveStateAnalysis {
    fn compute(plan: &CleanupPlan, topology: MovePathTopology) -> Self {
        let mut block_entry = vec![MoveState::new(false, &topology); plan.blocks.len()];
        let mut block_exit = vec![MoveState::new(false, &topology); plan.blocks.len()];
        block_entry[plan.entry.index()] = MoveState::new(true, &topology);
        let mut queue = VecDeque::from([plan.entry]);
        let mut queued = vec![false; plan.blocks.len()];
        queued[plan.entry.index()] = true;

        while let Some(block_id) = queue.pop_front() {
            queued[block_id.index()] = false;
            let block = &plan.blocks[block_id.index()];
            let mut state = block_entry[block_id.index()].clone();
            for operation in &block.operations {
                state.apply_operation(operation, plan, &topology);
            }
            if state == block_exit[block_id.index()] {
                continue;
            }
            block_exit[block_id.index()] = state.clone();

            let edges: Vec<&CleanupEdge> = match block
                .terminator
                .as_ref()
                .expect("structurally verified block has a terminator")
            {
                Terminator::Goto(edge) => vec![edge],
                Terminator::Branch {
                    then_edge,
                    else_edge,
                    ..
                } => vec![then_edge, else_edge],
                Terminator::Return { .. } | Terminator::Abort | Terminator::Unreachable => {
                    Vec::new()
                }
            };
            for edge in edges {
                let mut edge_state = state.clone();
                for scope in &edge.exited_scopes {
                    for local in &plan.scopes[scope.index()].locals {
                        edge_state.clear_local(*local, &topology);
                    }
                }
                if block_entry[edge.target.index()].join_from(&edge_state)
                    && !queued[edge.target.index()]
                {
                    queue.push_back(edge.target);
                    queued[edge.target.index()] = true;
                }
            }
        }

        Self {
            computed: true,
            topology,
            block_entry,
            block_exit,
        }
    }

    pub(crate) fn state_before(
        &self,
        plan: &CleanupPlan,
        block: BasicBlockId,
        operation_index: usize,
    ) -> Option<MoveState> {
        if !self.computed {
            return None;
        }
        let block_data = plan.blocks.get(block.index())?;
        if operation_index > block_data.operations.len() {
            return None;
        }
        let mut state = self.block_entry.get(block.index())?.clone();
        for operation in &block_data.operations[..operation_index] {
            state.apply_operation(operation, plan, &self.topology);
        }
        Some(state)
    }

    pub(crate) fn path_initialization_before(
        &self,
        plan: &CleanupPlan,
        block: BasicBlockId,
        operation_index: usize,
        path: MovePathId,
    ) -> Option<PathInitialization> {
        if path.index() >= self.topology.children.len() {
            return None;
        }
        self.state_before(plan, block, operation_index)
            .map(|state| state.path_initialization(path, &self.topology))
    }

    pub(crate) fn possible_variants_before(
        &self,
        plan: &CleanupPlan,
        block: BasicBlockId,
        operation_index: usize,
        path: MovePathId,
    ) -> Option<(bool, Vec<u32>)> {
        let state = self.state_before(plan, block, operation_index)?;
        let index = self
            .topology
            .discriminant_index
            .get(path.index())?
            .as_ref()?;
        let discriminant = state.discriminants.get(*index)?;
        Some((
            discriminant.may_be_unset,
            discriminant.possible_variants.clone(),
        ))
    }
}

impl CleanupPlan {
    pub(crate) fn verify(&self) -> Result<(), Vec<VerifyError>> {
        let analysis = self.analyze_and_verify()?;
        if self.move_state.computed && self.move_state != analysis {
            return Err(vec![VerifyError::new(
                "cached move-state analysis does not match the cleanup plan",
            )]);
        }
        Ok(())
    }

    fn analyze_and_verify(&self) -> Result<MoveStateAnalysis, Vec<VerifyError>> {
        let mut errors = Vec::new();

        self.verify_scopes(&mut errors);
        self.verify_locals(&mut errors);
        self.verify_move_paths(&mut errors);
        self.verify_blocks(&mut errors);
        self.verify_pending_capabilities(&mut errors);

        if !errors.is_empty() {
            return Err(errors);
        }

        let topology = MovePathTopology::new(self);
        self.verify_topology_operations(&topology, &mut errors);
        if !errors.is_empty() {
            return Err(errors);
        }
        let analysis = MoveStateAnalysis::compute(self, topology);
        self.verify_stable_move_state(&analysis, &mut errors);
        if errors.is_empty() {
            Ok(analysis)
        } else {
            Err(errors)
        }
    }

    fn verify_topology_operations(
        &self,
        topology: &MovePathTopology,
        errors: &mut Vec<VerifyError>,
    ) {
        for block in &self.blocks {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                match *operation {
                    CleanupOp::SetDiscriminant {
                        destination,
                        variant,
                    } => {
                        let valid_variant =
                            topology.enum_variants(destination).is_some_and(|variants| {
                                variants
                                    .binary_search_by_key(&variant, |(candidate, _)| *candidate)
                                    .is_ok()
                            });
                        if !valid_variant {
                            errors.push(VerifyError::new(format!(
                                "SetDiscriminant in {:?} at operation {operation_index} uses variant {variant} on non-enum or incomplete enum path {destination:?}",
                                block.id
                            )));
                        }
                    }
                    CleanupOp::Transfer {
                        source,
                        destination,
                        ..
                    } => {
                        if !topology.transfer_shapes_match(self, source, destination) {
                            errors.push(VerifyError::new(format!(
                                "Transfer in {:?} at operation {operation_index} uses incompatible move-path subtrees between {source:?} and {destination:?}",
                                block.id
                            )));
                        }
                    }
                    CleanupOp::StorageLive(_)
                    | CleanupOp::Init(_)
                    | CleanupOp::MoveOut(_)
                    | CleanupOp::Overwrite(_)
                    | CleanupOp::StorageDead(_) => {}
                }
            }
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
        let mut return_places = 0_usize;
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
            if local.kind == LocalKind::ReturnPlace {
                return_places += 1;
                if local.ownership != LocalOwnership::Owned {
                    errors.push(VerifyError::new(format!(
                        "return place {:?} must use owned storage",
                        local.id
                    )));
                }
                if local.source_local.is_some() || local.debug_name.is_some() {
                    errors.push(VerifyError::new(format!(
                        "return place {:?} must be planner-generated storage",
                        local.id
                    )));
                }
            }
        }
        if return_places > 1 {
            errors.push(VerifyError::new(format!(
                "cleanup plan has {return_places} return places, expected at most one"
            )));
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
        let mut places = HashSet::new();
        let mut roots = vec![0_usize; self.locals.len()];
        let mut children = vec![Vec::new(); self.move_paths.len()];
        for (index, path) in self.move_paths.iter().enumerate() {
            if path.id.index() != index {
                errors.push(VerifyError::new(format!(
                    "move path id/index mismatch: slot {index} contains {:?}",
                    path.id
                )));
            }
            if !places.insert(path.place.clone()) {
                errors.push(VerifyError::new(format!(
                    "move path {:?} duplicates place {:?}",
                    path.id, path.place
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
                    children[parent_id.index()].push(path.id);
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
                None => {
                    if let Some(count) = roots.get_mut(path.place.local.index()) {
                        *count += 1;
                    }
                }
            }
        }

        for (index, local) in self.locals.iter().enumerate() {
            let expected = usize::from(local.ownership == LocalOwnership::Owned);
            if roots[index] != expected {
                errors.push(VerifyError::new(format!(
                    "local {:?} has {} root move paths, expected {expected} for {:?} storage",
                    local.id, roots[index], local.ownership
                )));
            }
        }

        for (index, child_ids) in children.iter().enumerate() {
            let downcasts = child_ids
                .iter()
                .filter(|child| {
                    matches!(
                        self.move_paths[child.index()].place.projections.last(),
                        Some(Projection::Downcast(_))
                    )
                })
                .count();
            if downcasts != 0 && downcasts != child_ids.len() {
                errors.push(VerifyError::new(format!(
                    "move path {:?} mixes enum downcast and aggregate children",
                    MovePathId(index)
                )));
            }
            let captures = child_ids
                .iter()
                .filter(|child| {
                    matches!(
                        self.move_paths[child.index()].place.projections.last(),
                        Some(Projection::Capture(_))
                    )
                })
                .count();
            if captures != 0 && captures != child_ids.len() {
                errors.push(VerifyError::new(format!(
                    "move path {:?} mixes callable-capture and aggregate children",
                    MovePathId(index)
                )));
            }
            let mut variants = HashSet::new();
            for child in child_ids {
                if let Some(Projection::Downcast(variant)) =
                    self.move_paths[child.index()].place.projections.last()
                {
                    if !variants.insert(*variant) {
                        errors.push(VerifyError::new(format!(
                            "enum move path {:?} has duplicate downcast variant {variant}",
                            MovePathId(index)
                        )));
                    }
                }
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
                errors.push(VerifyError::new(format!(
                    "move path {path_id:?} uses unsupported dynamic Index projection"
                )));
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
                    let block_data = self.verify_pending_block(*block, "MaybeOverwrite", errors);
                    if self.move_paths.get(path.index()).is_none() {
                        errors.push(VerifyError::new(format!(
                            "pending MaybeOverwrite refers to invalid move path {path:?}"
                        )));
                    }
                    if let Some(block_data) = block_data {
                        let has_matching_operation = block_data.operations.iter().any(|operation| {
                            matches!(operation, CleanupOp::Overwrite(candidate) if candidate == path)
                                || matches!(
                                    operation,
                                    CleanupOp::Transfer {
                                        destination,
                                        kind: TransferKind::MaybeOverwrite,
                                        ..
                                    } if destination == path
                                )
                        });
                        if !has_matching_operation {
                            errors.push(VerifyError::new(format!(
                                "pending MaybeOverwrite has no matching operation in block {block:?}"
                            )));
                        }
                    }
                }
                PendingCapability::PartialApplicationCapture { block, .. }
                | PendingCapability::LocalClosureCapture { block, .. } => {
                    self.verify_pending_block(*block, "cleanup capability", errors);
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
                PendingCapability::BorrowedPlaceMutation {
                    block,
                    alias,
                    source,
                    ..
                } => {
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
                        if self.move_paths.get(source.index()).is_none()
                            || !block.operations.contains(&CleanupOp::MoveOut(*source))
                        {
                            errors.push(VerifyError::new(
                                "pending borrowed place mutation must consume its staged source in that block",
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
                if self
                    .locals
                    .get(local.index())
                    .is_some_and(|decl| decl.kind == LocalKind::Temporary)
                    && !self.pending_capabilities.iter().any(|pending| {
                        matches!(
                            pending,
                            PendingCapability::TemporaryStorageLiveness {
                                block: pending_block,
                                local: pending_local,
                            } if *pending_block == block && *pending_local == local
                        )
                    })
                {
                    errors.push(VerifyError::new(format!(
                        "temporary StorageLive in {block:?} has no matching TemporaryStorageLiveness pending capability"
                    )));
                }
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
            CleanupOp::Transfer {
                source,
                destination,
                kind,
            } => {
                self.verify_operation_path(block, source, "Transfer source", true, errors);
                self.verify_operation_path(
                    block,
                    destination,
                    "Transfer destination",
                    true,
                    errors,
                );
                if source == destination {
                    errors.push(VerifyError::new(format!(
                        "Transfer in {block:?} must use distinct source and destination paths"
                    )));
                } else if let (Some(source_path), Some(destination_path)) = (
                    self.move_paths.get(source.index()),
                    self.move_paths.get(destination.index()),
                ) {
                    let overlaps = source_path.place.local == destination_path.place.local
                        && (source_path
                            .place
                            .projections
                            .starts_with(&destination_path.place.projections)
                            || destination_path
                                .place
                                .projections
                                .starts_with(&source_path.place.projections));
                    if overlaps {
                        errors.push(VerifyError::new(format!(
                            "Transfer in {block:?} has overlapping source and destination paths"
                        )));
                    }
                }
                if kind == TransferKind::MaybeOverwrite
                    && !self.pending_capabilities.iter().any(|pending| {
                        matches!(
                            pending,
                            PendingCapability::MaybeOverwrite {
                                block: pending_block,
                                path
                            } if *pending_block == block && *path == destination
                        )
                    })
                {
                    errors.push(VerifyError::new(format!(
                        "MaybeOverwrite transfer in {block:?} has no matching pending capability"
                    )));
                }
            }
            CleanupOp::SetDiscriminant { destination, .. } => {
                self.verify_operation_path(block, destination, "SetDiscriminant", true, errors);
            }
        }
    }

    fn verify_stable_move_state(
        &self,
        analysis: &MoveStateAnalysis,
        errors: &mut Vec<VerifyError>,
    ) {
        for block in &self.blocks {
            let mut state = analysis.block_entry[block.id.index()].clone();
            if !state.reachable {
                continue;
            }
            for (operation_index, operation) in block.operations.iter().enumerate() {
                let status = |path| state.path_initialization(path, &analysis.topology);
                let mut require_active_downcasts = |path, role: &str| {
                    if let Some((enum_root, required, may_be_unset, possible)) =
                        state.inactive_downcast(path, self, &analysis.topology)
                    {
                        errors.push(VerifyError::new(format!(
                            "{role} in {:?} at operation {operation_index} projects variant {required} through enum path {enum_root:?}, whose discriminant is unset={may_be_unset} with possible variants {possible:?}",
                            block.id
                        )));
                    }
                };
                match *operation {
                    CleanupOp::Init(path) => {
                        require_active_downcasts(path, "Init");
                    }
                    CleanupOp::MoveOut(path) => {
                        require_active_downcasts(path, "MoveOut");
                        if status(path) != PathInitialization::Initialized {
                            errors.push(VerifyError::new(format!(
                                "MoveOut in {:?} at operation {operation_index} requires an initialized source {path:?}, found {:?}",
                                block.id,
                                status(path)
                            )));
                        }
                    }
                    CleanupOp::Overwrite(path) => {
                        require_active_downcasts(path, "Overwrite");
                        if status(path) != PathInitialization::Initialized {
                            errors.push(VerifyError::new(format!(
                                "Overwrite in {:?} at operation {operation_index} requires an initialized destination {path:?}, found {:?}",
                                block.id,
                                status(path)
                            )));
                        }
                    }
                    CleanupOp::Transfer {
                        source,
                        destination,
                        kind,
                    } => {
                        require_active_downcasts(source, "Transfer source");
                        require_active_downcasts(destination, "Transfer destination");
                        if status(source) != PathInitialization::Initialized {
                            errors.push(VerifyError::new(format!(
                                "Transfer in {:?} at operation {operation_index} requires an initialized source {source:?}, found {:?}",
                                block.id,
                                status(source)
                            )));
                        }
                        let destination_state = status(destination);
                        let valid_destination = match kind {
                            TransferKind::Initialize => {
                                destination_state == PathInitialization::Uninitialized
                            }
                            TransferKind::Overwrite => {
                                destination_state == PathInitialization::Initialized
                            }
                            TransferKind::MaybeOverwrite => {
                                destination_state == PathInitialization::MaybeOrPartial
                            }
                        };
                        if !valid_destination {
                            errors.push(VerifyError::new(format!(
                                "{kind:?} Transfer in {:?} at operation {operation_index} has destination {destination:?} in incompatible state {destination_state:?}",
                                block.id
                            )));
                        }
                    }
                    CleanupOp::SetDiscriminant {
                        destination,
                        variant: _,
                    } => {
                        require_active_downcasts(destination, "SetDiscriminant");
                        let destination_state = status(destination);
                        if destination_state != PathInitialization::Uninitialized {
                            errors.push(VerifyError::new(format!(
                                "SetDiscriminant in {:?} at operation {operation_index} requires an uninitialized destination {destination:?}, found {destination_state:?}",
                                block.id
                            )));
                        }
                    }
                    CleanupOp::StorageLive(_) | CleanupOp::StorageDead(_) => {}
                }
                state.apply_operation(operation, self, &analysis.topology);
                for path in 0..self.move_paths.len() {
                    if state.must_init.contains(path) && !state.may_init.contains(path) {
                        errors.push(VerifyError::new(format!(
                            "move-state invariant failed after operation {operation_index} in {:?}: {path:?} is must-init but not may-init",
                            block.id
                        )));
                    }
                }
            }

            match block
                .terminator
                .as_ref()
                .expect("structurally verified block has a terminator")
            {
                Terminator::Branch { condition, .. } => {
                    if let Some(root) = analysis
                        .topology
                        .roots_by_local
                        .get(condition.index())
                        .copied()
                        .flatten()
                    {
                        let condition_state = state.path_initialization(root, &analysis.topology);
                        if condition_state != PathInitialization::Initialized {
                            errors.push(VerifyError::new(format!(
                                "branch in {:?} requires initialized condition storage {condition:?}, found {condition_state:?}",
                                block.id
                            )));
                        }
                    } else {
                        errors.push(VerifyError::new(format!(
                            "branch in {:?} has no owned root move path for condition storage {condition:?}",
                            block.id
                        )));
                    }
                }
                Terminator::Return { .. } => {
                    for local in self
                        .locals
                        .iter()
                        .filter(|local| local.kind == LocalKind::ReturnPlace)
                    {
                        let Some(root) = analysis.topology.roots_by_local[local.id.index()] else {
                            errors.push(VerifyError::new(format!(
                                "return place {:?} has no root move path",
                                local.id
                            )));
                            continue;
                        };
                        let return_state = state.path_initialization(root, &analysis.topology);
                        if return_state != PathInitialization::Initialized {
                            errors.push(VerifyError::new(format!(
                                "return in {:?} requires initialized return place {:?}, found {return_state:?}",
                                block.id, local.id
                            )));
                        }
                    }
                }
                Terminator::Goto(_) | Terminator::Abort | Terminator::Unreachable => {}
            }
        }
    }

    pub(crate) fn move_state_before(
        &self,
        block: BasicBlockId,
        operation_index: usize,
    ) -> Option<MoveState> {
        self.move_state.state_before(self, block, operation_index)
    }

    pub(crate) fn path_initialization_before(
        &self,
        block: BasicBlockId,
        operation_index: usize,
        path: MovePathId,
    ) -> Option<PathInitialization> {
        self.move_state
            .path_initialization_before(self, block, operation_index, path)
    }

    pub(crate) fn possible_variants_before(
        &self,
        block: BasicBlockId,
        operation_index: usize,
        path: MovePathId,
    ) -> Option<(bool, Vec<u32>)> {
        self.move_state
            .possible_variants_before(self, block, operation_index, path)
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
                    Some(local) if local.ownership != LocalOwnership::Owned => {
                        errors.push(VerifyError::new(format!(
                            "branch in {:?} requires owned condition storage, found {:?}",
                            block.id, local.ownership
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
                move_state: MoveStateAnalysis::uncomputed(),
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

    pub(crate) fn finish(mut self) -> Result<CleanupPlan, Vec<VerifyError>> {
        let analysis = self.plan.analyze_and_verify()?;
        self.plan.move_state = analysis;
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
        builder.record_pending(PendingCapability::TemporaryStorageLiveness {
            block: body,
            local: borrowed,
        });
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
            message.contains("requires initialized return place")
                && message.contains("MaybeOrPartial")
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
            .set_terminator(orphan, Terminator::Unreachable)
            .unwrap();

        assert!(messages(&builder.into_unverified()).iter().any(|message| {
            message.contains("SetDiscriminant") && message.contains("non-enum")
        }));
    }

    #[test]
    fn rejects_temporary_storage_without_liveness_pending() {
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
        assert!(messages(&builder.into_unverified()).iter().any(|message| {
            message.contains("TemporaryStorageLiveness") && message.contains("no matching")
        }));
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
        let ordinary_path = builder.new_move_path(Place::local(ordinary), None).unwrap();
        builder.record_pending(PendingCapability::TemporaryStorageLiveness {
            block: entry,
            local: ordinary,
        });
        builder.record_pending(PendingCapability::BorrowedPlaceMutation {
            block: entry,
            alias: shared,
            source: ordinary_path,
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
            plan.possible_variants_before(entry, 4, root),
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
        builder.record_pending(PendingCapability::MaybeOverwrite {
            block: join,
            path: destination_path,
        });
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
            if kind == TransferKind::MaybeOverwrite {
                builder.record_pending(PendingCapability::MaybeOverwrite {
                    block: entry,
                    path: destination,
                });
            }
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
        for (block, variant, variant_path) in
            [(first, 0, first_variant), (second, 1, second_variant)]
        {
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
}
