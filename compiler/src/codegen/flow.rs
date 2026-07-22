use std::collections::{HashMap, HashSet};

use crate::ast::{Binding, ItemOrigin, Type};

use super::fallible::ReturnBoundary;
use super::hir::{
    ClosureInfo, HirPlace, LoanId, LocalCapability, LocalId, ParamSig, PartialInfo, Ty,
};

#[derive(Debug, Clone)]
pub(super) struct LocalInfo {
    pub(super) id: LocalId,
    pub(super) ty: Ty,
    pub(super) mutable: bool,
    pub(super) capability: LocalCapability,
    pub(super) alias: Option<HirPlace>,
    pub(super) partial: Option<PartialInfo>,
    pub(super) closure: Option<ClosureInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct PlaceKey {
    pub(super) local: LocalId,
    pub(super) projections: Vec<usize>,
}

impl From<&HirPlace> for PlaceKey {
    fn from(place: &HirPlace) -> Self {
        Self {
            local: place.local,
            projections: place.projections.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InitializationStatus {
    Initialized,
    Uninitialized,
    MaybeUninitialized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoanKind {
    Shared,
    Mutable,
}

#[derive(Debug, Clone)]
pub(super) struct Loan {
    pub(super) place: PlaceKey,
    pub(super) kind: LoanKind,
}

#[derive(Debug, Clone)]
pub(super) struct FlowState {
    pub(super) reachable: bool,
    /// Exact alternatives of normalized, uninitialized leaf move paths.
    ///
    /// A reachable flow always has at least one alternative. Keeping the
    /// alternatives disjoint until a use lets joins preserve correlations such
    /// as "the left field is moved on one branch and the right field on the
    /// other": the root is then definitely unavailable while either field is
    /// only possibly unavailable.
    pub(super) uninitialized: Vec<HashSet<PlaceKey>>,
    pub(super) loans: HashMap<LoanId, Loan>,
}

/// Preserve exact branch correlations while they remain small, then widen to
/// a conservative maybe-uninitialized summary. This prevents independent
/// branches from making ownership analysis exponential in source size.
pub(super) const MAX_INITIALIZATION_ALTERNATIVES: usize = 64;

impl Default for FlowState {
    fn default() -> Self {
        Self {
            reachable: true,
            uninitialized: vec![HashSet::new()],
            loans: HashMap::new(),
        }
    }
}

impl FlowState {
    pub(super) fn join(flows: &[Self]) -> Self {
        let reachable: Vec<_> = flows.iter().filter(|flow| flow.reachable).collect();
        match reachable.as_slice() {
            [] => Self {
                reachable: false,
                uninitialized: Vec::new(),
                loans: HashMap::new(),
            },
            [only] => (*only).clone(),
            _ => {
                let uninitialized = normalize_uninitialized_alternatives(
                    reachable
                        .iter()
                        .flat_map(|flow| flow.uninitialized.iter().cloned()),
                );
                let loans = reachable
                    .iter()
                    .flat_map(|flow| flow.loans.iter().map(|(id, loan)| (*id, loan.clone())))
                    .collect();
                Self {
                    reachable: true,
                    uninitialized,
                    loans,
                }
            }
        }
    }

    pub(super) fn initialization_status(&self, leaves: &[PlaceKey]) -> InitializationStatus {
        if !self.reachable {
            return InitializationStatus::Initialized;
        }
        let unavailable = self
            .uninitialized
            .iter()
            .filter(|alternative| leaves.iter().any(|leaf| alternative.contains(leaf)))
            .count();
        match unavailable {
            0 => InitializationStatus::Initialized,
            count if count == self.uninitialized.len() => InitializationStatus::Uninitialized,
            _ => InitializationStatus::MaybeUninitialized,
        }
    }

    pub(super) fn normalize_uninitialized(&mut self) {
        if !self.reachable {
            self.uninitialized.clear();
            return;
        }
        self.uninitialized =
            normalize_uninitialized_alternatives(std::mem::take(&mut self.uninitialized));
    }
}

#[derive(Clone)]
pub(super) struct ScopeFrame {
    pub(super) names: HashMap<String, LocalInfo>,
    pub(super) locals: Vec<LocalId>,
    pub(super) lexical_loans: Vec<LoanId>,
}

impl ScopeFrame {
    fn new() -> Self {
        Self {
            names: HashMap::new(),
            locals: Vec::new(),
            lexical_loans: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub(super) struct RecursiveFrameCall {
    pub(super) function: String,
    pub(super) captures: Vec<ParamSig>,
    pub(super) parameters: Vec<ParamSig>,
    pub(super) result: Ty,
}

#[derive(Clone)]
pub(super) struct LoopFrame {
    pub(super) result_ty: Option<Ty>,
    pub(super) unit_only: bool,
    pub(super) scope_depth: usize,
    pub(super) break_flows: Vec<FlowState>,
    pub(super) continue_flows: Vec<FlowState>,
}

#[derive(Clone)]
pub(super) struct InspectionBinding {
    pub(super) root: LocalId,
    pub(super) path: Vec<usize>,
    pub(super) ty: Ty,
}

#[derive(Clone)]
pub(super) struct LowerCtx {
    pub(super) scopes: Vec<ScopeFrame>,
    pub(super) flow: FlowState,
    pub(super) next_local: LocalId,
    pub(super) next_loan: LoanId,
    pub(super) declared_result: Option<Ty>,
    pub(super) return_boundary: Option<ReturnBoundary>,
    pub(super) returned_types: Vec<Ty>,
    pub(super) function_name: Option<String>,
    pub(super) origin: ItemOrigin,
    pub(super) type_substitutions: HashMap<String, Type>,
    pub(super) loops: Vec<LoopFrame>,
    pub(super) guard_move_restricted: HashSet<LocalId>,
    pub(super) inspection_bindings: HashMap<LocalId, InspectionBinding>,
    pub(super) borrowed_parameter_regions: HashMap<LocalId, (Option<String>, bool)>,
    pub(super) reference_loans: HashMap<LocalId, Vec<LoanId>>,
    pub(super) reference_value_depth: usize,
    pub(super) unsafe_depth: usize,
    pub(super) active_throws_error: Option<Ty>,
    pub(super) active_custom_effects: HashSet<String>,
    pub(super) active_custom_effect_sources: HashMap<String, Type>,
    pub(super) lexical_handler_effects: HashSet<String>,
    pub(super) lexical_handler_effect_sources: HashMap<String, Type>,
    pub(super) recursive_frame_calls: HashMap<String, RecursiveFrameCall>,
    pub(super) source_closures: HashMap<LocalId, Binding>,
}

impl LowerCtx {
    pub(super) fn for_function(name: &str, result: Option<Ty>, origin: ItemOrigin) -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            flow: FlowState::default(),
            next_local: 0,
            next_loan: 0,
            declared_result: result,
            return_boundary: None,
            returned_types: Vec::new(),
            function_name: Some(name.to_owned()),
            origin,
            type_substitutions: HashMap::new(),
            loops: Vec::new(),
            guard_move_restricted: HashSet::new(),
            inspection_bindings: HashMap::new(),
            borrowed_parameter_regions: HashMap::new(),
            reference_loans: HashMap::new(),
            reference_value_depth: 0,
            unsafe_depth: 0,
            active_throws_error: None,
            active_custom_effects: HashSet::new(),
            active_custom_effect_sources: HashMap::new(),
            lexical_handler_effects: HashSet::new(),
            lexical_handler_effect_sources: HashMap::new(),
            recursive_frame_calls: HashMap::new(),
            source_closures: HashMap::new(),
        }
    }

    pub(super) fn for_global(origin: ItemOrigin) -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            flow: FlowState::default(),
            next_local: 0,
            next_loan: 0,
            declared_result: None,
            return_boundary: None,
            returned_types: Vec::new(),
            function_name: None,
            origin,
            type_substitutions: HashMap::new(),
            loops: Vec::new(),
            guard_move_restricted: HashSet::new(),
            inspection_bindings: HashMap::new(),
            borrowed_parameter_regions: HashMap::new(),
            reference_loans: HashMap::new(),
            reference_value_depth: 0,
            unsafe_depth: 0,
            active_throws_error: None,
            active_custom_effects: HashSet::new(),
            active_custom_effect_sources: HashMap::new(),
            lexical_handler_effects: HashSet::new(),
            lexical_handler_effect_sources: HashMap::new(),
            recursive_frame_calls: HashMap::new(),
            source_closures: HashMap::new(),
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<&LocalInfo> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.names.get(name))
    }

    pub(super) fn has_type_parameter(&self, name: &str) -> bool {
        self.type_substitutions.contains_key(name)
    }

    pub(super) fn shadows_top_level_name(&self, name: &str) -> bool {
        self.lookup(name).is_some() || self.has_type_parameter(name)
    }

    pub(super) fn fresh_local(&mut self) -> LocalId {
        let id = self.next_local;
        self.next_local += 1;
        id
    }

    pub(super) fn push_scope(&mut self) {
        self.scopes.push(ScopeFrame::new());
    }

    pub(super) fn pop_scope(&mut self) {
        let scope = self.scopes.pop().expect("cannot pop all scopes");
        for loan in scope.lexical_loans {
            self.flow.loans.remove(&loan);
        }
        for alternative in &mut self.flow.uninitialized {
            alternative.retain(|place| !scope.locals.contains(&place.local));
        }
        self.flow.normalize_uninitialized();
    }

    pub(super) fn pop_scope_preserving_loans(&mut self, preserved: &[LoanId]) {
        let scope = self.scopes.pop().expect("cannot pop all scopes");
        for loan in scope.lexical_loans {
            if preserved.contains(&loan) {
                let parent = self
                    .scopes
                    .last_mut()
                    .expect("an escaping block reference has a parent scope");
                if !parent.lexical_loans.contains(&loan) {
                    parent.lexical_loans.push(loan);
                }
            } else {
                self.flow.loans.remove(&loan);
            }
        }
        for alternative in &mut self.flow.uninitialized {
            alternative.retain(|place| !scope.locals.contains(&place.local));
        }
        self.flow.normalize_uninitialized();
    }

    pub(super) fn flow_without_current_scope(&self, mut flow: FlowState) -> FlowState {
        let scope = self.scopes.last().expect("at least one scope");
        for loan in &scope.lexical_loans {
            flow.loans.remove(loan);
        }
        for alternative in &mut flow.uninitialized {
            alternative.retain(|place| !scope.locals.contains(&place.local));
        }
        flow.normalize_uninitialized();
        flow
    }

    pub(super) fn flow_without_scopes_from(
        &self,
        scope_depth: usize,
        mut flow: FlowState,
    ) -> FlowState {
        for scope in &self.scopes[scope_depth..] {
            for loan in &scope.lexical_loans {
                flow.loans.remove(loan);
            }
            for alternative in &mut flow.uninitialized {
                alternative.retain(|place| !scope.locals.contains(&place.local));
            }
        }
        flow.normalize_uninitialized();
        flow
    }

    pub(super) fn outer_local_ids(&self) -> HashSet<LocalId> {
        self.scopes
            .iter()
            .flat_map(|scope| scope.locals.iter().copied())
            .collect()
    }

    pub(super) fn insert_local(&mut self, name: String, local: LocalInfo) -> bool {
        let scope = self.scopes.last_mut().expect("at least one scope");
        if scope.names.contains_key(&name) {
            return false;
        }
        scope.locals.push(local.id);
        scope.names.insert(name, local);
        true
    }
}

pub(super) fn is_place_prefix(prefix: &PlaceKey, place: &PlaceKey) -> bool {
    prefix.local == place.local
        && prefix.projections.len() <= place.projections.len()
        && place.projections.starts_with(&prefix.projections)
}

pub(super) fn places_overlap(left: &PlaceKey, right: &PlaceKey) -> bool {
    is_place_prefix(left, right) || is_place_prefix(right, left)
}

pub(super) fn projected_uninitialized_alternatives(
    flow: &FlowState,
    local: LocalId,
) -> Vec<HashSet<PlaceKey>> {
    normalize_uninitialized_alternatives(flow.uninitialized.iter().map(|alternative| {
        alternative
            .iter()
            .filter(|place| place.local == local)
            .cloned()
            .collect()
    }))
}

pub(super) fn normalize_uninitialized_alternatives(
    alternatives: impl IntoIterator<Item = HashSet<PlaceKey>>,
) -> Vec<HashSet<PlaceKey>> {
    let mut unique = Vec::new();
    let mut union = HashSet::new();
    let mut widened = false;
    for alternative in alternatives {
        union.extend(alternative.iter().cloned());
        if widened || unique.iter().any(|existing| existing == &alternative) {
            continue;
        }
        unique.push(alternative);
        if unique.len() > MAX_INITIALIZATION_ALTERNATIVES {
            widened = true;
            unique.clear();
        }
    }

    if widened {
        if union.is_empty() {
            vec![HashSet::new()]
        } else {
            vec![HashSet::new(), union]
        }
    } else {
        unique
    }
}

pub(super) fn alternative_sets_equal(
    left: &[HashSet<PlaceKey>],
    right: &[HashSet<PlaceKey>],
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .all(|alternative| right.iter().any(|other| other == alternative))
}
