use std::collections::HashSet;

use crate::ast::Expr;

use super::flow::{places_overlap, InitializationStatus, Loan, LoanKind, LowerCtx, PlaceKey};
use super::hir::{
    AccessKind, AssignmentKind, HirExpr, HirExprKind, HirPlace, HirReadKind, LoanId,
    LocalCapability, Ty,
};
use super::lower::integer_literal_value;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_place(
        &mut self,
        expression: &Expr,
        context: &mut LowerCtx,
    ) -> Option<HirPlace> {
        match expression {
            Expr::Name(name) => {
                let Some(local) = context.lookup(name).cloned() else {
                    if context.has_type_parameter(name) {
                        self.error(format!("type parameter `{name}` is not a data place"));
                    } else if self.globals.contains_key(name) {
                        self.error(format!(
                            "global constant `{name}` is not a borrowable place"
                        ));
                    } else {
                        self.error(format!("unknown local `{name}` in place expression"));
                    }
                    return None;
                };
                if let Some(alias) = local.alias {
                    return Some(alias);
                }
                if let Ty::Reference {
                    pointee, mutable, ..
                } = &local.ty
                {
                    return Some(HirPlace {
                        local: local.id,
                        root_ty: (**pointee).clone(),
                        projections: Vec::new(),
                        ty: (**pointee).clone(),
                        capability: if *mutable {
                            LocalCapability::MutParam
                        } else {
                            LocalCapability::SharedParam
                        },
                        root_mutable: *mutable,
                        loan: None,
                        indirect: true,
                    });
                }
                Some(HirPlace {
                    local: local.id,
                    root_ty: local.ty.clone(),
                    projections: Vec::new(),
                    ty: local.ty,
                    capability: local.capability,
                    root_mutable: local.mutable,
                    loan: None,
                    indirect: false,
                })
            }
            Expr::Member(base, field_name) => {
                let mut place = self.lower_place(base, context)?;
                let Ty::Struct(struct_name) = &place.ty else {
                    self.error(format!(
                        "field `{field_name}` cannot be selected on value of type `{}`",
                        place.ty
                    ));
                    return None;
                };
                let layout = self.struct_layout_or_diagnostic(struct_name)?;
                let Some((index, field)) = layout
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.name == *field_name)
                else {
                    self.error(format!(
                        "unknown field `{field_name}` on struct `{struct_name}`"
                    ));
                    return None;
                };
                if !self.require_field_access(struct_name, field, &context.origin) {
                    return None;
                }
                place.projections.push(index);
                place.ty = field.ty.clone();
                Some(place)
            }
            Expr::Index { base, index } => {
                let mut place = self.lower_place(base, context)?;
                let Ty::Array(element, length) = &place.ty else {
                    self.error(format!(
                        "array index place requires an array value, found `{}`",
                        place.ty
                    ));
                    return None;
                };
                let Some(index) = integer_literal_value(index) else {
                    self.error(
                        "array place index must be a compile-time integer literal; dynamic indexes are read-only for now",
                    );
                    return None;
                };
                let Ok(index) = u64::try_from(index) else {
                    self.error(format!(
                        "array index {index} is out of bounds for length {length}"
                    ));
                    return None;
                };
                if index >= *length {
                    self.error(format!(
                        "array index {index} is out of bounds for length {length}"
                    ));
                    return None;
                }
                let Ok(projection) = usize::try_from(index) else {
                    self.error(format!("array index {index} does not fit this target"));
                    return None;
                };
                place.projections.push(projection);
                place.ty = element.as_ref().clone();
                Some(place)
            }
            _ => {
                self.error("expression is not a local place");
                None
            }
        }
    }

    pub(super) fn access_place(
        &mut self,
        place: HirPlace,
        requested: AccessKind,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let access = if requested == AccessKind::Auto {
            if self.is_copy_type(&place.ty) {
                AccessKind::Copy
            } else {
                AccessKind::Move
            }
        } else {
            requested
        };
        self.ensure_available(&place, context);
        self.ensure_no_conflicting_loan(&place, access, context);
        match access {
            AccessKind::Copy => {
                if !self.is_copy_type(&place.ty) {
                    let ty = self.diagnostic_type_name(&place.ty);
                    self.error(format!(
                        "type `{ty}` does not implement Copy and cannot be copied"
                    ));
                }
                HirExpr {
                    ty: place.ty.clone(),
                    kind: HirExprKind::Read {
                        place,
                        kind: HirReadKind::Copy,
                    },
                }
            }
            AccessKind::Move => {
                if place.capability != LocalCapability::Owned {
                    self.error("cannot move out of a borrowed value");
                } else if self.projected_place_crosses_custom_drop(&place) {
                    self.error(
                        "moving a field out through a type with custom Drop is not allowed because its destructor requires a complete value",
                    );
                } else if context.guard_move_restricted.contains(&place.local)
                    && !self.is_copy_type(&place.ty)
                {
                    self.error("cannot move a non-Copy pattern binding in a match guard");
                } else {
                    self.mark_moved(&place, context);
                }
                HirExpr {
                    ty: place.ty.clone(),
                    kind: HirExprKind::Read {
                        place,
                        kind: HirReadKind::Move,
                    },
                }
            }
            AccessKind::Auto | AccessKind::SharedBorrow | AccessKind::MutBorrow => {
                unreachable!("borrow accesses do not produce values")
            }
        }
    }

    pub(super) fn ensure_available(&mut self, place: &HirPlace, context: &LowerCtx) {
        if !context.flow.reachable {
            return;
        }
        let leaves = self.place_leaf_keys(place);
        match context.flow.initialization_status(&leaves) {
            InitializationStatus::Initialized => {}
            InitializationStatus::Uninitialized => {
                self.error("use of moved or uninitialized value")
            }
            InitializationStatus::MaybeUninitialized => {
                self.error("use of possibly moved or uninitialized value")
            }
        }
    }

    pub(super) fn ensure_no_conflicting_loan(
        &mut self,
        place: &HirPlace,
        access: AccessKind,
        context: &LowerCtx,
    ) {
        if !context.flow.reachable {
            return;
        }
        let requested = PlaceKey::from(place);
        let conflict = context.flow.loans.iter().any(|(id, loan)| {
            Some(*id) != place.loan
                && places_overlap(&requested, &loan.place)
                && match access {
                    AccessKind::Copy | AccessKind::SharedBorrow => loan.kind == LoanKind::Mutable,
                    AccessKind::Move | AccessKind::MutBorrow => true,
                    AccessKind::Auto => unreachable!("auto access must be resolved"),
                }
        });
        if !conflict {
            return;
        }
        self.error(match access {
            AccessKind::Copy => "cannot read value while it is mutably borrowed",
            AccessKind::Move => "cannot move value because it is borrowed",
            AccessKind::SharedBorrow => "cannot borrow value while it is mutably borrowed",
            AccessKind::MutBorrow => {
                "cannot create mutable borrow because the value is already borrowed"
            }
            AccessKind::Auto => unreachable!("auto access must be resolved"),
        });
    }

    pub(super) fn ensure_writable(&mut self, place: &HirPlace) {
        match place.capability {
            LocalCapability::MutParam => {}
            LocalCapability::SharedParam => {
                self.error("cannot assign through a shared borrow");
            }
            LocalCapability::Owned if place.root_mutable => {}
            LocalCapability::Owned => {
                self.error("cannot assign to immutable binding");
            }
        }
    }

    pub(super) fn mark_moved(&mut self, place: &HirPlace, context: &mut LowerCtx) {
        if !context.flow.reachable {
            return;
        }
        let leaves = self.place_leaf_keys(place);
        for alternative in &mut context.flow.uninitialized {
            alternative.extend(leaves.iter().cloned());
        }
        context.flow.normalize_uninitialized();
    }

    pub(super) fn mark_initialized(
        &mut self,
        place: &HirPlace,
        context: &mut LowerCtx,
    ) -> AssignmentKind {
        if !context.flow.reachable {
            return AssignmentKind::Overwrite;
        }
        let leaves = self.place_leaf_keys(place);
        let mut saw_overwrite = false;
        let mut saw_initialize = false;
        let mut saw_partial = false;
        for alternative in &context.flow.uninitialized {
            let unavailable = leaves
                .iter()
                .filter(|leaf| alternative.contains(*leaf))
                .count();
            if unavailable == 0 {
                saw_overwrite = true;
            } else if unavailable == leaves.len() {
                saw_initialize = true;
            } else {
                saw_partial = true;
            }
        }
        let assignment = match (saw_overwrite, saw_initialize, saw_partial) {
            (true, false, false) => AssignmentKind::Overwrite,
            (false, true, false) => AssignmentKind::Initialize,
            _ => AssignmentKind::MaybeOverwrite,
        };
        for alternative in &mut context.flow.uninitialized {
            alternative.retain(|leaf| !leaves.contains(leaf));
        }
        context.flow.normalize_uninitialized();
        assignment
    }

    pub(super) fn place_leaf_keys(&self, place: &HirPlace) -> Vec<PlaceKey> {
        let mut leaves = Vec::new();
        self.append_leaf_keys(
            PlaceKey::from(place),
            &place.ty,
            &mut HashSet::new(),
            &mut leaves,
        );
        leaves
    }

    fn append_leaf_keys(
        &self,
        key: PlaceKey,
        ty: &Ty,
        visiting: &mut HashSet<String>,
        leaves: &mut Vec<PlaceKey>,
    ) {
        if let Ty::Array(element, length) = ty {
            if *length == 0 {
                leaves.push(key);
                return;
            }
            for index in 0..*length {
                let Ok(index) = usize::try_from(index) else {
                    leaves.push(key);
                    return;
                };
                let mut element_key = key.clone();
                element_key.projections.push(index);
                self.append_leaf_keys(element_key, element, visiting, leaves);
            }
            return;
        }
        let Ty::Struct(name) = ty else {
            leaves.push(key);
            return;
        };
        // Recursive value layouts are diagnosed separately. Keep move-path
        // construction finite while that invalid source continues lowering.
        if !visiting.insert(name.clone()) {
            leaves.push(key);
            return;
        }
        let Some(layout) = self.struct_layouts.get(name) else {
            visiting.remove(name);
            leaves.push(key);
            return;
        };
        if layout.fields.is_empty() {
            visiting.remove(name);
            leaves.push(key);
            return;
        }
        for (index, field) in layout.fields.iter().enumerate() {
            let mut field_key = key.clone();
            field_key.projections.push(index);
            self.append_leaf_keys(field_key, &field.ty, visiting, leaves);
        }
        visiting.remove(name);
    }

    pub(super) fn acquire_loan(
        &mut self,
        place: &HirPlace,
        kind: LoanKind,
        lexical: bool,
        context: &mut LowerCtx,
    ) -> Option<LoanId> {
        let diagnostics_before = self.diagnostics.len();
        self.ensure_available(place, context);
        let access = match kind {
            LoanKind::Shared => AccessKind::SharedBorrow,
            LoanKind::Mutable => AccessKind::MutBorrow,
        };
        self.ensure_no_conflicting_loan(place, access, context);
        if self.diagnostics.len() != diagnostics_before || !context.flow.reachable {
            return None;
        }
        let id = context.next_loan;
        context.next_loan += 1;
        context.flow.loans.insert(
            id,
            Loan {
                place: PlaceKey::from(place),
                kind,
            },
        );
        if lexical {
            context
                .scopes
                .last_mut()
                .expect("borrow expression has a scope")
                .lexical_loans
                .push(id);
        }
        Some(id)
    }

    pub(super) fn release_loans(&mut self, loans: &[LoanId], context: &mut LowerCtx) {
        for loan in loans {
            context.flow.loans.remove(loan);
        }
    }

    pub(super) fn lower_place_without_diagnostic(
        &mut self,
        expression: &Expr,
        context: &mut LowerCtx,
    ) -> Option<HirPlace> {
        match expression {
            Expr::Name(name) if context.lookup(name).is_some() => {
                self.lower_place(expression, context)
            }
            Expr::Member(base, _) => {
                self.lower_place_without_diagnostic(base, context)?;
                self.lower_place(expression, context)
            }
            Expr::Index { base, index } if integer_literal_value(index).is_some() => {
                self.lower_place_without_diagnostic(base, context)?;
                self.lower_place(expression, context)
            }
            _ => None,
        }
    }
}
