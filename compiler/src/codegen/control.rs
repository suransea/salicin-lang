use std::collections::HashSet;

use crate::ast::Expr;

use super::flow::{
    alternative_sets_equal, projected_uninitialized_alternatives, FlowState, LoopFrame, LowerCtx,
};
use super::hir::{HirExpr, HirExprKind, LocalId, Ty};
use super::lower::error_expr;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_while(
        &mut self,
        condition: &Expr,
        body: &Expr,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let entry_flow = context.flow.clone();
        let outer_locals = context.outer_local_ids();
        context.loops.push(LoopFrame {
            result_ty: Some(Ty::Unit),
            unit_only: true,
            scope_depth: context.scopes.len(),
            break_flows: Vec::new(),
            continue_flows: Vec::new(),
        });

        let condition = self.lower_expr(condition, Some(&Ty::Bool), context);
        let condition_flow = context.flow.clone();
        let body = self.lower_expr(body, Some(&Ty::Unit), context);
        let backedge_flow = context.flow.clone();
        let frame = context.loops.pop().expect("while frame");

        let mut backedge_flows = frame.continue_flows;
        if backedge_flow.reachable {
            backedge_flows.push(backedge_flow);
        }
        for backedge in &backedge_flows {
            self.reject_loop_carried_moves(&entry_flow, backedge, &outer_locals);
        }
        let mut exit_flows = frame.break_flows;
        exit_flows.push(condition_flow);
        exit_flows.extend(backedge_flows);
        context.flow = FlowState::join(&exit_flows);
        HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::While {
                condition: Box::new(condition),
                body: Box::new(body),
            },
        }
    }

    pub(super) fn lower_loop(
        &mut self,
        body: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let entry_flow = context.flow.clone();
        let outer_locals = context.outer_local_ids();
        context.loops.push(LoopFrame {
            result_ty: expected.cloned(),
            unit_only: false,
            scope_depth: context.scopes.len(),
            break_flows: Vec::new(),
            continue_flows: Vec::new(),
        });
        let body = self.lower_expr(body, Some(&Ty::Unit), context);
        let backedge_flow = context.flow.clone();
        let frame = context.loops.pop().expect("loop frame");

        let mut backedge_flows = frame.continue_flows;
        if backedge_flow.reachable {
            backedge_flows.push(backedge_flow);
        }
        for backedge in &backedge_flows {
            self.reject_loop_carried_moves(&entry_flow, backedge, &outer_locals);
        }
        let has_reachable_break = !frame.break_flows.is_empty();
        context.flow = FlowState::join(&frame.break_flows);
        let ty = if has_reachable_break {
            frame.result_ty.unwrap_or(Ty::Unit)
        } else {
            Ty::Never
        };
        HirExpr {
            ty,
            kind: HirExprKind::Loop {
                body: Box::new(body),
            },
        }
    }

    pub(super) fn lower_break(&mut self, value: Option<&Expr>, context: &mut LowerCtx) -> HirExpr {
        let Some(frame) = context.loops.last() else {
            self.error("`break` cannot be used outside a `while` or `loop`");
            if let Some(value) = value {
                let _ = self.lower_expr(value, None, context);
            }
            return error_expr();
        };
        let unit_only = frame.unit_only;
        let expected = frame.result_ty.clone();
        let scope_depth = frame.scope_depth;

        if unit_only && value.is_some() {
            self.error("`break` in a `while` loop cannot carry a value");
        }
        let value = value.map(|value| {
            Box::new(self.lower_expr(
                value,
                (!unit_only).then_some(expected.as_ref()).flatten(),
                context,
            ))
        });
        let break_ty = value.as_ref().map_or(Ty::Unit, |value| value.ty.clone());
        if !unit_only {
            let result_ty = match expected {
                Some(expected) => self.unify_types(&expected, &break_ty, "break values"),
                None => break_ty,
            };
            context.loops.last_mut().expect("break frame").result_ty = Some(result_ty);
        }

        if context.flow.reachable {
            let break_flow = context.flow_without_scopes_from(scope_depth, context.flow.clone());
            context
                .loops
                .last_mut()
                .expect("break frame")
                .break_flows
                .push(break_flow);
        }
        context.flow.reachable = false;
        HirExpr {
            ty: Ty::Never,
            kind: HirExprKind::Break(value),
        }
    }

    pub(super) fn lower_continue(&mut self, context: &mut LowerCtx) -> HirExpr {
        let Some(frame) = context.loops.last() else {
            self.error("`continue` cannot be used outside a `while` or `loop`");
            return error_expr();
        };
        let scope_depth = frame.scope_depth;
        if context.flow.reachable {
            let continue_flow = context.flow_without_scopes_from(scope_depth, context.flow.clone());
            context
                .loops
                .last_mut()
                .expect("continue frame")
                .continue_flows
                .push(continue_flow);
        }
        context.flow.reachable = false;
        HirExpr {
            ty: Ty::Never,
            kind: HirExprKind::Continue,
        }
    }

    fn reject_loop_carried_moves(
        &mut self,
        entry: &FlowState,
        backedge: &FlowState,
        outer_locals: &HashSet<LocalId>,
    ) {
        for local in outer_locals {
            let entry_alternatives = projected_uninitialized_alternatives(entry, *local);
            let backedge_alternatives = projected_uninitialized_alternatives(backedge, *local);
            if alternative_sets_equal(&entry_alternatives, &backedge_alternatives)
                || backedge_alternatives.iter().all(HashSet::is_empty)
            {
                continue;
            }
            self.error(
                "move of an outer value may cross a loop backedge; reinitialize it before the next iteration or move it only on a break/return path",
            );
        }
    }
}
