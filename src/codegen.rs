//! Type checking and textual LLVM IR generation for Salicin's M0 subset.
//!
//! The backend intentionally consumes the parser AST directly but first lowers
//! it to a small typed representation.  No malformed program reaches the LLVM
//! emitter, which keeps the generated IR simple enough to inspect in tests.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ast::{BinaryOp, Binding, Expr, Function, Item, PassMode, Program, Stmt, Type, UnaryOp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
}

impl Diagnostic {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Type-check `program` and emit portable textual LLVM IR using opaque
/// pointers.  The returned module deliberately omits a target triple so that
/// the caller can compile it for the selected LLVM target.
pub fn compile(program: &Program) -> Result<String, Vec<Diagnostic>> {
    let mut analyzer = Analyzer::new(program);
    let hir = analyzer.analyze();
    if !analyzer.diagnostics.is_empty() {
        return Err(analyzer.diagnostics);
    }

    let hir = hir.expect("analysis without diagnostics must produce HIR");
    let constants = evaluate_globals(&hir)?;

    match Emitter::new(&hir, constants).emit_module() {
        Ok(ir) => Ok(ir),
        Err(error) => Err(vec![error]),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Ty {
    I32,
    I64,
    U32,
    U64,
    Bool,
    Unit,
    Never,
    Function(FunctionTy),
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FunctionTy {
    groups: Vec<Vec<Ty>>,
    result: Box<Ty>,
}

impl Ty {
    fn is_integer(&self) -> bool {
        matches!(self, Self::I32 | Self::I64 | Self::U32 | Self::U64)
    }

    fn is_signed(&self) -> bool {
        matches!(self, Self::I32 | Self::I64)
    }

    fn llvm(&self) -> Option<&'static str> {
        match self {
            Self::I32 | Self::U32 => Some("i32"),
            Self::I64 | Self::U64 => Some("i64"),
            Self::Bool => Some("i1"),
            Self::Unit | Self::Never => None,
            Self::Function(_) | Self::Error => None,
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I32 => f.write_str("i32"),
            Self::I64 => f.write_str("i64"),
            Self::U32 => f.write_str("u32"),
            Self::U64 => f.write_str("u64"),
            Self::Bool => f.write_str("bool"),
            Self::Unit => f.write_str("()"),
            Self::Never => f.write_str("never"),
            Self::Error => f.write_str("<error>"),
            Self::Function(function) => {
                for group in &function.groups {
                    f.write_str("(")?;
                    for (index, ty) in group.iter().enumerate() {
                        if index != 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{ty}")?;
                    }
                    f.write_str("): ")?;
                }
                write!(f, "{}", function.result)
            }
        }
    }
}

type LocalId = usize;

#[derive(Debug, Clone)]
struct HirProgram {
    globals: Vec<HirGlobal>,
    functions: Vec<HirFunction>,
}

#[derive(Debug, Clone)]
struct HirGlobal {
    name: String,
    ty: Ty,
    value: HirExpr,
}

#[derive(Debug, Clone)]
struct HirFunction {
    name: String,
    params: Vec<HirParam>,
    result: Ty,
    body: HirExpr,
}

#[derive(Debug, Clone)]
struct HirParam {
    id: LocalId,
    name: String,
    ty: Ty,
}

#[derive(Debug, Clone)]
struct HirBinding {
    id: LocalId,
    name: String,
    ty: Ty,
    value: HirExpr,
}

#[derive(Debug, Clone)]
enum HirStmt {
    Let(HirBinding),
    Expr(HirExpr),
}

#[derive(Debug, Clone)]
struct HirExpr {
    ty: Ty,
    kind: HirExprKind,
}

#[derive(Debug, Clone)]
enum HirExprKind {
    Integer(i128),
    Bool(bool),
    Unit,
    Local(LocalId),
    Global(String),
    Function(String),
    Unary(UnaryOp, Box<HirExpr>),
    Binary(Box<HirExpr>, BinaryOp, Box<HirExpr>),
    Assign(LocalId, Box<HirExpr>),
    Call {
        function: String,
        arguments: Vec<HirExpr>,
    },
    Block(Vec<HirStmt>, Option<Box<HirExpr>>),
    If {
        condition: Box<HirExpr>,
        then_branch: Box<HirExpr>,
        else_branch: Option<Box<HirExpr>>,
    },
    Return(Option<Box<HirExpr>>),
}

#[derive(Debug, Clone)]
struct ParamSig {
    name: String,
    ty: Ty,
    mode: PassMode,
}

#[derive(Debug, Clone)]
struct FunctionSig {
    groups: Vec<Vec<ParamSig>>,
    result: Option<Ty>,
}

impl FunctionSig {
    fn function_ty(&self) -> Option<Ty> {
        Some(Ty::Function(FunctionTy {
            groups: self
                .groups
                .iter()
                .map(|group| group.iter().map(|param| param.ty.clone()).collect())
                .collect(),
            result: Box::new(self.result.clone()?),
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolutionState {
    Resolving,
    Resolved,
}

#[derive(Debug, Clone)]
struct LocalInfo {
    id: LocalId,
    ty: Ty,
    mutable: bool,
}

struct LowerCtx {
    scopes: Vec<HashMap<String, LocalInfo>>,
    next_local: LocalId,
    declared_result: Option<Ty>,
    returned_types: Vec<Ty>,
    function_name: Option<String>,
}

impl LowerCtx {
    fn for_function(name: &str, result: Option<Ty>) -> Self {
        Self {
            scopes: vec![HashMap::new()],
            next_local: 0,
            declared_result: result,
            returned_types: Vec::new(),
            function_name: Some(name.to_owned()),
        }
    }

    fn for_global() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            next_local: 0,
            declared_result: None,
            returned_types: Vec::new(),
            function_name: None,
        }
    }

    fn lookup(&self, name: &str) -> Option<&LocalInfo> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn fresh_local(&mut self) -> LocalId {
        let id = self.next_local;
        self.next_local += 1;
        id
    }
}

struct Analyzer {
    functions: HashMap<String, Function>,
    globals: HashMap<String, Binding>,
    function_order: Vec<String>,
    global_order: Vec<String>,
    signatures: HashMap<String, FunctionSig>,
    global_annotations: HashMap<String, Option<Ty>>,
    function_states: HashMap<String, ResolutionState>,
    global_states: HashMap<String, ResolutionState>,
    hir_functions: HashMap<String, HirFunction>,
    hir_globals: HashMap<String, HirGlobal>,
    diagnostics: Vec<Diagnostic>,
}

impl Analyzer {
    fn new(program: &Program) -> Self {
        let mut analyzer = Self {
            functions: HashMap::new(),
            globals: HashMap::new(),
            function_order: Vec::new(),
            global_order: Vec::new(),
            signatures: HashMap::new(),
            global_annotations: HashMap::new(),
            function_states: HashMap::new(),
            global_states: HashMap::new(),
            hir_functions: HashMap::new(),
            hir_globals: HashMap::new(),
            diagnostics: Vec::new(),
        };
        analyzer.collect_items(program);
        analyzer
    }

    fn collect_items(&mut self, program: &Program) {
        let mut names = HashSet::new();
        for item in &program.items {
            match item {
                Item::Function(function) => {
                    if !names.insert(function.name.clone()) {
                        self.error(format!("duplicate top-level name `{}`", function.name));
                        continue;
                    }

                    let groups = function
                        .groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|param| ParamSig {
                                    name: param.name.clone(),
                                    ty: self.lower_source_type(&param.ty),
                                    mode: param.mode,
                                })
                                .collect()
                        })
                        .collect();
                    let result = function
                        .return_type
                        .as_ref()
                        .map(|ty| self.lower_source_type(ty));
                    self.signatures
                        .insert(function.name.clone(), FunctionSig { groups, result });
                    self.function_order.push(function.name.clone());
                    self.functions
                        .insert(function.name.clone(), function.clone());
                }
                Item::Global(binding) => {
                    if !names.insert(binding.name.clone()) {
                        self.error(format!("duplicate top-level name `{}`", binding.name));
                        continue;
                    }
                    if binding.mutable {
                        self.error(format!(
                            "mutable global `{}` is not supported in M0",
                            binding.name
                        ));
                    }
                    let annotation = binding
                        .annotation
                        .as_ref()
                        .map(|ty| self.lower_source_type(ty));
                    self.global_annotations
                        .insert(binding.name.clone(), annotation);
                    self.global_order.push(binding.name.clone());
                    self.globals.insert(binding.name.clone(), binding.clone());
                }
            }
        }
    }

    fn analyze(&mut self) -> Option<HirProgram> {
        for name in self.global_order.clone() {
            self.lower_global(&name);
        }
        for name in self.function_order.clone() {
            self.lower_function(&name);
        }
        self.validate_entry_point();

        if !self.diagnostics.is_empty() {
            return None;
        }

        Some(HirProgram {
            globals: self
                .global_order
                .iter()
                .map(|name| self.hir_globals[name].clone())
                .collect(),
            functions: self
                .function_order
                .iter()
                .map(|name| self.hir_functions[name].clone())
                .collect(),
        })
    }

    fn lower_source_type(&mut self, source: &Type) -> Ty {
        match source {
            Type::I32 => Ty::I32,
            Type::I64 => Ty::I64,
            Type::U32 => Ty::U32,
            Type::U64 => Ty::U64,
            Type::Bool => Ty::Bool,
            Type::Void => Ty::Unit,
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Ty::Unit,
            Type::Named(name, _) => {
                self.error(format!("type `{name}` is not supported in M0"));
                Ty::Error
            }
        }
    }

    fn lower_function(&mut self, name: &str) -> Ty {
        if self.function_states.get(name) == Some(&ResolutionState::Resolved) {
            return self.signatures[name].result.clone().unwrap_or(Ty::Error);
        }
        if self.function_states.get(name) == Some(&ResolutionState::Resolving) {
            if let Some(result) = self.signatures[name].result.clone() {
                return result;
            }
            self.error(format!(
                "cannot infer recursive function `{name}`; add a return type"
            ));
            return Ty::Error;
        }

        self.function_states
            .insert(name.to_owned(), ResolutionState::Resolving);
        let function = self.functions[name].clone();
        let signature = self.signatures[name].clone();
        let mut context = LowerCtx::for_function(name, signature.result.clone());
        let mut params = Vec::new();

        for group in &signature.groups {
            for param in group {
                self.validate_parameter_mode(name, param);
                if context.scopes[0].contains_key(&param.name) {
                    self.error(format!(
                        "duplicate parameter `{}` in function `{name}`",
                        param.name
                    ));
                    continue;
                }
                let id = context.fresh_local();
                context.scopes[0].insert(
                    param.name.clone(),
                    LocalInfo {
                        id,
                        ty: param.ty.clone(),
                        mutable: false,
                    },
                );
                params.push(HirParam {
                    id,
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                });
            }
        }

        let Some(body) = function.body.as_ref() else {
            self.error(format!("function `{name}` has no body"));
            self.function_states
                .insert(name.to_owned(), ResolutionState::Resolved);
            self.set_function_result(name, Ty::Error);
            return Ty::Error;
        };

        let lowered_body = self.lower_expr(body, signature.result.as_ref(), &mut context);
        let result = if let Some(declared) = signature.result {
            for returned in &context.returned_types {
                self.require_same_type(
                    returned,
                    &declared,
                    format!("return value in function `{name}`"),
                );
            }
            declared
        } else {
            let mut inferred = if lowered_body.ty == Ty::Never {
                None
            } else {
                Some(lowered_body.ty.clone())
            };
            for returned in &context.returned_types {
                inferred = Some(match inferred {
                    Some(current) => self.unify_types(
                        &current,
                        returned,
                        format!("return values in function `{name}`"),
                    ),
                    None => returned.clone(),
                });
            }
            inferred.unwrap_or(Ty::Unit)
        };

        self.set_function_result(name, result.clone());
        self.hir_functions.insert(
            name.to_owned(),
            HirFunction {
                name: name.to_owned(),
                params,
                result: result.clone(),
                body: lowered_body,
            },
        );
        self.function_states
            .insert(name.to_owned(), ResolutionState::Resolved);
        result
    }

    fn validate_parameter_mode(&mut self, function: &str, param: &ParamSig) {
        if matches!(
            param.mode,
            PassMode::Move | PassMode::Borrow | PassMode::MutBorrow
        ) {
            self.error(format!(
                "parameter `{}` in function `{function}` uses {}, which is not supported in M0",
                param.name,
                match param.mode {
                    PassMode::Move => "`move`",
                    PassMode::Borrow => "`borrow`",
                    PassMode::MutBorrow => "`mut borrow`",
                    _ => unreachable!(),
                }
            ));
        }
    }

    fn set_function_result(&mut self, name: &str, result: Ty) {
        if let Some(signature) = self.signatures.get_mut(name) {
            signature.result = Some(result);
        }
    }

    fn function_type(&mut self, name: &str) -> Ty {
        let Some(signature) = self.signatures.get(name) else {
            self.error(format!("unknown function `{name}`"));
            return Ty::Error;
        };
        if signature.result.is_none() {
            self.lower_function(name);
        }
        self.signatures[name].function_ty().unwrap_or(Ty::Error)
    }

    fn lower_global(&mut self, name: &str) -> Ty {
        if self.global_states.get(name) == Some(&ResolutionState::Resolved) {
            return self.hir_globals[name].ty.clone();
        }
        if self.global_states.get(name) == Some(&ResolutionState::Resolving) {
            self.error(format!("cyclic global constant involving `{name}`"));
            return Ty::Error;
        }
        self.global_states
            .insert(name.to_owned(), ResolutionState::Resolving);

        let binding = self.globals[name].clone();
        let expected = self.global_annotations[name].clone();
        let mut context = LowerCtx::for_global();
        let value = self.lower_expr(&binding.value, expected.as_ref(), &mut context);
        if !context.returned_types.is_empty() {
            self.error(format!("`return` is not allowed in global `{name}`"));
        }
        let ty = expected.unwrap_or_else(|| value.ty.clone());
        if matches!(ty, Ty::Function(_)) {
            self.error(format!(
                "global function values are not supported in M0 (`{name}`)"
            ));
        }
        self.hir_globals.insert(
            name.to_owned(),
            HirGlobal {
                name: name.to_owned(),
                ty: ty.clone(),
                value,
            },
        );
        self.global_states
            .insert(name.to_owned(), ResolutionState::Resolved);
        ty
    }

    fn global_type(&mut self, name: &str) -> Ty {
        self.lower_global(name)
    }

    fn lower_expr(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let lowered = match expression {
            Expr::Integer(value) => {
                let ty = match expected {
                    Some(ty) if ty.is_integer() => ty.clone(),
                    Some(Ty::Error) => Ty::Error,
                    Some(ty) => {
                        self.error(format!(
                            "integer literal cannot be used where `{ty}` is expected"
                        ));
                        Ty::Error
                    }
                    None => Ty::I32,
                };
                if ty.is_integer() && !integer_fits(*value, &ty) {
                    self.error(format!("integer literal `{value}` does not fit in `{ty}`"));
                }
                HirExpr {
                    ty,
                    kind: HirExprKind::Integer(*value),
                }
            }
            Expr::Bool(value) => HirExpr {
                ty: Ty::Bool,
                kind: HirExprKind::Bool(*value),
            },
            Expr::Unit => HirExpr {
                ty: Ty::Unit,
                kind: HirExprKind::Unit,
            },
            Expr::Name(name) => {
                if let Some(local) = context.lookup(name) {
                    HirExpr {
                        ty: local.ty.clone(),
                        kind: HirExprKind::Local(local.id),
                    }
                } else if self.globals.contains_key(name) {
                    HirExpr {
                        ty: self.global_type(name),
                        kind: HirExprKind::Global(name.clone()),
                    }
                } else if self.functions.contains_key(name) {
                    HirExpr {
                        ty: self.function_type(name),
                        kind: HirExprKind::Function(name.clone()),
                    }
                } else {
                    self.error(format!("unknown name `{name}`"));
                    error_expr()
                }
            }
            Expr::Unary(operator, operand) => {
                if *operator == UnaryOp::Neg {
                    if let Expr::Integer(value) = operand.as_ref() {
                        let ty = match expected {
                            Some(ty) if ty.is_signed() => ty.clone(),
                            Some(Ty::Error) => Ty::Error,
                            Some(ty) => {
                                self.error(format!(
                                    "negative integer literal cannot be used where `{ty}` is expected"
                                ));
                                Ty::Error
                            }
                            None => Ty::I32,
                        };
                        if ty.is_signed()
                            && value
                                .checked_neg()
                                .is_none_or(|negative| !integer_fits(negative, &ty))
                        {
                            self.error(format!(
                                "negative integer literal `-{value}` does not fit in `{ty}`"
                            ));
                        }
                        return HirExpr {
                            ty: ty.clone(),
                            kind: HirExprKind::Unary(
                                UnaryOp::Neg,
                                Box::new(HirExpr {
                                    ty,
                                    kind: HirExprKind::Integer(*value),
                                }),
                            ),
                        };
                    }
                }
                let operand_expected = match operator {
                    UnaryOp::Not => Some(Ty::Bool),
                    UnaryOp::Neg => expected.filter(|ty| ty.is_integer()).cloned(),
                };
                let operand = self.lower_expr(operand, operand_expected.as_ref(), context);
                let ty = match operator {
                    UnaryOp::Not => {
                        self.require_same_type(&operand.ty, &Ty::Bool, "operand of `!`");
                        Ty::Bool
                    }
                    UnaryOp::Neg => {
                        if !operand.ty.is_integer() || !operand.ty.is_signed() {
                            self.error(format!(
                                "unary `-` requires a signed integer, found `{}`",
                                operand.ty
                            ));
                            Ty::Error
                        } else {
                            operand.ty.clone()
                        }
                    }
                };
                HirExpr {
                    ty,
                    kind: HirExprKind::Unary(*operator, Box::new(operand)),
                }
            }
            Expr::Binary(left, operator, right) => {
                self.lower_binary(left, *operator, right, expected, context)
            }
            Expr::Assign(name, value) => {
                let Some(local) = context.lookup(name).cloned() else {
                    if self.globals.contains_key(name) {
                        self.error(format!("global constant `{name}` cannot be assigned"));
                    } else {
                        self.error(format!("unknown local `{name}` in assignment"));
                    }
                    return error_expr();
                };
                if !local.mutable {
                    self.error(format!("cannot assign to immutable binding `{name}`"));
                }
                let value = self.lower_expr(value, Some(&local.ty), context);
                HirExpr {
                    ty: Ty::Unit,
                    kind: HirExprKind::Assign(local.id, Box::new(value)),
                }
            }
            Expr::Call(_, _) => self.lower_call(expression, context),
            Expr::Block(statements, tail) => {
                context.scopes.push(HashMap::new());
                let mut lowered_statements = Vec::new();
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) => {
                            let annotation = binding
                                .annotation
                                .as_ref()
                                .map(|ty| self.lower_source_type(ty));
                            let value =
                                self.lower_expr(&binding.value, annotation.as_ref(), context);
                            let ty = annotation.unwrap_or_else(|| value.ty.clone());
                            if matches!(ty, Ty::Function(_)) {
                                self.error(format!(
                                    "partial application/function-valued local `{}` is not supported in M0",
                                    binding.name
                                ));
                            }
                            let duplicate = context
                                .scopes
                                .last()
                                .expect("block scope")
                                .contains_key(&binding.name);
                            if duplicate {
                                self.error(format!(
                                    "duplicate binding `{}` in the same scope",
                                    binding.name
                                ));
                            }
                            let id = context.fresh_local();
                            if !duplicate {
                                context.scopes.last_mut().expect("block scope").insert(
                                    binding.name.clone(),
                                    LocalInfo {
                                        id,
                                        ty: ty.clone(),
                                        mutable: binding.mutable,
                                    },
                                );
                            }
                            lowered_statements.push(HirStmt::Let(HirBinding {
                                id,
                                name: binding.name.clone(),
                                ty,
                                value,
                            }));
                        }
                        Stmt::Expr(expression) => {
                            lowered_statements
                                .push(HirStmt::Expr(self.lower_expr(expression, None, context)));
                        }
                    }
                }
                let lowered_tail = tail
                    .as_ref()
                    .map(|tail| Box::new(self.lower_expr(tail, expected, context)));
                let ty = lowered_tail
                    .as_ref()
                    .map_or(Ty::Unit, |tail| tail.ty.clone());
                context.scopes.pop();
                HirExpr {
                    ty,
                    kind: HirExprKind::Block(lowered_statements, lowered_tail),
                }
            }
            Expr::Closure(_, _) => {
                self.error("closures are not supported in M0");
                error_expr()
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.lower_expr(condition, Some(&Ty::Bool), context);
                let (then_branch, else_branch) = if let Some(else_ast) = else_branch.as_ref() {
                    let (then_branch, else_branch) = if expected.is_some() {
                        (
                            self.lower_expr(then_branch, expected, context),
                            self.lower_expr(else_ast, expected, context),
                        )
                    } else if is_unconstrained_integer(then_branch)
                        && !is_unconstrained_integer(else_ast)
                    {
                        let else_branch = self.lower_expr(else_ast, None, context);
                        let branch_hint = if matches!(else_branch.ty, Ty::Never | Ty::Error) {
                            None
                        } else {
                            Some(&else_branch.ty)
                        };
                        let then_branch = self.lower_expr(then_branch, branch_hint, context);
                        (then_branch, else_branch)
                    } else {
                        let then_branch = self.lower_expr(then_branch, None, context);
                        let branch_hint = if matches!(then_branch.ty, Ty::Never | Ty::Error) {
                            None
                        } else {
                            Some(&then_branch.ty)
                        };
                        let else_branch = self.lower_expr(else_ast, branch_hint, context);
                        (then_branch, else_branch)
                    };
                    (then_branch, Some(Box::new(else_branch)))
                } else {
                    (self.lower_expr(then_branch, Some(&Ty::Unit), context), None)
                };
                let ty = if let Some(else_branch) = &else_branch {
                    self.unify_types(&then_branch.ty, &else_branch.ty, "branches of `if`")
                } else {
                    self.require_same_type(
                        &then_branch.ty,
                        &Ty::Unit,
                        "then branch of `if` without `else`",
                    );
                    Ty::Unit
                };
                HirExpr {
                    ty,
                    kind: HirExprKind::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    },
                }
            }
            Expr::Return(value) => {
                if context.function_name.is_none() {
                    self.error("`return` may only appear in a function body");
                }
                let declared_result = context.declared_result.clone();
                let value = value.as_ref().map(|value| {
                    Box::new(self.lower_expr(value, declared_result.as_ref(), context))
                });
                let returned_ty = value.as_ref().map_or(Ty::Unit, |value| value.ty.clone());
                context.returned_types.push(returned_ty);
                HirExpr {
                    ty: Ty::Never,
                    kind: HirExprKind::Return(value),
                }
            }
        };

        if let Some(expected) = expected {
            self.require_same_type(&lowered.ty, expected, "expression");
        }
        lowered
    }

    fn lower_binary(
        &mut self,
        left: &Expr,
        operator: BinaryOp,
        right: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        use BinaryOp::*;
        let (left, right, ty) = match operator {
            And | Or => {
                let left = self.lower_expr(left, Some(&Ty::Bool), context);
                let right = self.lower_expr(right, Some(&Ty::Bool), context);
                (left, right, Ty::Bool)
            }
            Add | Sub | Mul | Div | Rem | Lt | Le | Gt | Ge => {
                let numeric_hint = expected.filter(|ty| ty.is_integer());
                let (left, right) = self.lower_numeric_pair(left, right, numeric_hint, context);
                if !left.ty.is_integer() {
                    self.error(format!(
                        "operator `{}` requires integer operands, found `{}`",
                        binary_spelling(operator),
                        left.ty
                    ));
                }
                self.require_same_type(
                    &left.ty,
                    &right.ty,
                    format!("operands of `{}`", binary_spelling(operator)),
                );
                let ty = if matches!(operator, Lt | Le | Gt | Ge) {
                    Ty::Bool
                } else {
                    left.ty.clone()
                };
                (left, right, ty)
            }
            Eq | Ne => {
                let (left, right) = self.lower_numeric_or_bool_pair(left, right, context);
                if !(left.ty.is_integer() || left.ty == Ty::Bool || left.ty == Ty::Error) {
                    self.error(format!(
                        "operator `{}` does not support `{}`",
                        binary_spelling(operator),
                        left.ty
                    ));
                }
                self.require_same_type(
                    &left.ty,
                    &right.ty,
                    format!("operands of `{}`", binary_spelling(operator)),
                );
                (left, right, Ty::Bool)
            }
        };
        HirExpr {
            ty,
            kind: HirExprKind::Binary(Box::new(left), operator, Box::new(right)),
        }
    }

    fn lower_numeric_pair(
        &mut self,
        left: &Expr,
        right: &Expr,
        hint: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> (HirExpr, HirExpr) {
        if let Some(hint) = hint {
            return (
                self.lower_expr(left, Some(hint), context),
                self.lower_expr(right, Some(hint), context),
            );
        }
        match (left, right) {
            (Expr::Integer(_), Expr::Integer(_)) => (
                self.lower_expr(left, Some(&Ty::I32), context),
                self.lower_expr(right, Some(&Ty::I32), context),
            ),
            (Expr::Integer(_), _) => {
                let right = self.lower_expr(right, None, context);
                let left = self.lower_expr(left, Some(&right.ty), context);
                (left, right)
            }
            (_, Expr::Integer(_)) => {
                let left = self.lower_expr(left, None, context);
                let right = self.lower_expr(right, Some(&left.ty), context);
                (left, right)
            }
            _ => {
                let left = self.lower_expr(left, None, context);
                let right = self.lower_expr(right, Some(&left.ty), context);
                (left, right)
            }
        }
    }

    fn lower_numeric_or_bool_pair(
        &mut self,
        left: &Expr,
        right: &Expr,
        context: &mut LowerCtx,
    ) -> (HirExpr, HirExpr) {
        match (left, right) {
            (Expr::Integer(_), Expr::Integer(_)) => (
                self.lower_expr(left, Some(&Ty::I32), context),
                self.lower_expr(right, Some(&Ty::I32), context),
            ),
            (Expr::Integer(_), _) => {
                let right = self.lower_expr(right, None, context);
                let left = self.lower_expr(left, Some(&right.ty), context);
                (left, right)
            }
            _ => {
                let left = self.lower_expr(left, None, context);
                let right = self.lower_expr(right, Some(&left.ty), context);
                (left, right)
            }
        }
    }

    fn lower_call(&mut self, expression: &Expr, context: &mut LowerCtx) -> HirExpr {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(name) = root else {
            self.error("M0 only supports direct calls to named functions");
            return error_expr();
        };
        if !self.functions.contains_key(name) {
            self.error(format!("`{name}` is not a function"));
            return error_expr();
        }

        let function_ty = self.function_type(name);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        if groups.len() < function_ty.groups.len() {
            self.error(format!(
                "partial application of `{name}` is not supported in M0: supplied {} of {} parameter groups",
                groups.len(),
                function_ty.groups.len()
            ));
            return error_expr();
        }
        if groups.len() > function_ty.groups.len() {
            self.error(format!(
                "too many parameter groups in call to `{name}`: expected {}, found {}",
                function_ty.groups.len(),
                groups.len()
            ));
            return error_expr();
        }

        let mut arguments = Vec::new();
        for (group_index, (arguments_ast, params)) in
            groups.iter().zip(&function_ty.groups).enumerate()
        {
            if arguments_ast.len() != params.len() {
                self.error(format!(
                    "argument count mismatch in group {} of `{name}`: expected {}, found {}",
                    group_index + 1,
                    params.len(),
                    arguments_ast.len()
                ));
            }
            for (argument, parameter) in arguments_ast.iter().zip(params) {
                arguments.push(self.lower_expr(argument, Some(parameter), context));
            }
        }

        HirExpr {
            ty: (*function_ty.result).clone(),
            kind: HirExprKind::Call {
                function: name.clone(),
                arguments,
            },
        }
    }

    fn require_same_type(&mut self, actual: &Ty, expected: &Ty, context: impl fmt::Display) {
        if actual == expected
            || *actual == Ty::Never
            || *actual == Ty::Error
            || *expected == Ty::Error
        {
            return;
        }
        self.error(format!(
            "type mismatch for {context}: expected `{expected}`, found `{actual}`"
        ));
    }

    fn unify_types(&mut self, left: &Ty, right: &Ty, context: impl fmt::Display) -> Ty {
        if left == right {
            return left.clone();
        }
        if *left == Ty::Never {
            return right.clone();
        }
        if *right == Ty::Never {
            return left.clone();
        }
        if *left == Ty::Error || *right == Ty::Error {
            return Ty::Error;
        }
        self.error(format!(
            "type mismatch for {context}: `{left}` and `{right}` cannot be unified"
        ));
        Ty::Error
    }

    fn validate_entry_point(&mut self) {
        let Some(signature) = self.signatures.get("main").cloned() else {
            self.error("binary program has no `main` function");
            return;
        };
        let result = match signature.result {
            Some(result) => result,
            None => self.lower_function("main"),
        };
        let signature = &self.signatures["main"];
        if signature.groups.len() != 1 || !signature.groups[0].is_empty() {
            self.error("`main` must have exactly one empty parameter group: `main()`");
        }
        if !matches!(result, Ty::Unit | Ty::I32 | Ty::Error) {
            self.error(format!(
                "M0 `main` must return `()` or `i32`, found `{result}`"
            ));
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(message));
    }
}

fn error_expr() -> HirExpr {
    HirExpr {
        ty: Ty::Error,
        kind: HirExprKind::Unit,
    }
}

fn flatten_call<'a>(expression: &'a Expr, groups: &mut Vec<&'a [Expr]>) -> &'a Expr {
    match expression {
        Expr::Call(callee, arguments) => {
            let root = flatten_call(callee, groups);
            groups.push(arguments);
            root
        }
        _ => expression,
    }
}

fn integer_fits(value: i128, ty: &Ty) -> bool {
    match ty {
        Ty::I32 => i32::try_from(value).is_ok(),
        Ty::I64 => i64::try_from(value).is_ok(),
        Ty::U32 => u32::try_from(value).is_ok(),
        Ty::U64 => u64::try_from(value).is_ok(),
        _ => false,
    }
}

fn is_unconstrained_integer(expression: &Expr) -> bool {
    match expression {
        Expr::Integer(_) => true,
        Expr::Unary(UnaryOp::Neg, operand) => matches!(operand.as_ref(), Expr::Integer(_)),
        Expr::Block(_, Some(tail)) => is_unconstrained_integer(tail),
        _ => false,
    }
}

fn binary_spelling(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConstValue {
    Integer(i128),
    Bool(bool),
    Unit,
}

fn evaluate_globals(program: &HirProgram) -> Result<HashMap<String, ConstValue>, Vec<Diagnostic>> {
    let globals: HashMap<_, _> = program
        .globals
        .iter()
        .map(|global| (global.name.clone(), global))
        .collect();
    let mut evaluator = ConstantEvaluator {
        globals,
        values: HashMap::new(),
        active: HashSet::new(),
        diagnostics: Vec::new(),
    };
    for global in &program.globals {
        evaluator.evaluate_global(&global.name);
    }
    if evaluator.diagnostics.is_empty() {
        Ok(evaluator.values)
    } else {
        Err(evaluator.diagnostics)
    }
}

struct ConstantEvaluator<'a> {
    globals: HashMap<String, &'a HirGlobal>,
    values: HashMap<String, ConstValue>,
    active: HashSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl ConstantEvaluator<'_> {
    fn evaluate_global(&mut self, name: &str) -> Option<ConstValue> {
        if let Some(value) = self.values.get(name) {
            return Some(value.clone());
        }
        if !self.active.insert(name.to_owned()) {
            self.error(format!("cyclic global constant involving `{name}`"));
            return None;
        }
        let global = *self.globals.get(name)?;
        let mut locals = HashMap::new();
        let value = self.evaluate_expr(&global.value, &mut locals);
        self.active.remove(name);
        if let Some(value) = &value {
            self.values.insert(name.to_owned(), value.clone());
        }
        value
    }

    fn evaluate_expr(
        &mut self,
        expression: &HirExpr,
        locals: &mut HashMap<LocalId, ConstValue>,
    ) -> Option<ConstValue> {
        match &expression.kind {
            HirExprKind::Integer(value) => Some(ConstValue::Integer(*value)),
            HirExprKind::Bool(value) => Some(ConstValue::Bool(*value)),
            HirExprKind::Unit => Some(ConstValue::Unit),
            HirExprKind::Local(id) => locals.get(id).cloned().or_else(|| {
                self.error("invalid local in constant expression");
                None
            }),
            HirExprKind::Global(name) => self.evaluate_global(name),
            HirExprKind::Unary(operator, operand) => {
                let operand = self.evaluate_expr(operand, locals)?;
                self.evaluate_unary(*operator, operand, &expression.ty)
            }
            HirExprKind::Binary(left, BinaryOp::And, right) => {
                let ConstValue::Bool(left) = self.evaluate_expr(left, locals)? else {
                    return None;
                };
                if !left {
                    Some(ConstValue::Bool(false))
                } else {
                    self.evaluate_expr(right, locals)
                }
            }
            HirExprKind::Binary(left, BinaryOp::Or, right) => {
                let ConstValue::Bool(left) = self.evaluate_expr(left, locals)? else {
                    return None;
                };
                if left {
                    Some(ConstValue::Bool(true))
                } else {
                    self.evaluate_expr(right, locals)
                }
            }
            HirExprKind::Binary(left, operator, right) => {
                let left_value = self.evaluate_expr(left, locals)?;
                let right_value = self.evaluate_expr(right, locals)?;
                self.evaluate_binary(left_value, *operator, right_value, &left.ty)
            }
            HirExprKind::Block(statements, tail) => {
                let saved = locals.clone();
                for statement in statements {
                    match statement {
                        HirStmt::Let(binding) => {
                            let value = self.evaluate_expr(&binding.value, locals)?;
                            locals.insert(binding.id, value);
                        }
                        HirStmt::Expr(expression) => {
                            self.evaluate_expr(expression, locals)?;
                        }
                    }
                }
                let result = match tail {
                    Some(tail) => self.evaluate_expr(tail, locals),
                    None => Some(ConstValue::Unit),
                };
                *locals = saved;
                result
            }
            HirExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let ConstValue::Bool(condition) = self.evaluate_expr(condition, locals)? else {
                    return None;
                };
                if condition {
                    self.evaluate_expr(then_branch, locals)
                } else if let Some(else_branch) = else_branch {
                    self.evaluate_expr(else_branch, locals)
                } else {
                    Some(ConstValue::Unit)
                }
            }
            HirExprKind::Assign(_, _)
            | HirExprKind::Call { .. }
            | HirExprKind::Function(_)
            | HirExprKind::Return(_) => {
                self.error("global initializer is not a compile-time constant");
                None
            }
        }
    }

    fn evaluate_unary(
        &mut self,
        operator: UnaryOp,
        operand: ConstValue,
        ty: &Ty,
    ) -> Option<ConstValue> {
        match (operator, operand) {
            (UnaryOp::Not, ConstValue::Bool(value)) => Some(ConstValue::Bool(!value)),
            (UnaryOp::Neg, ConstValue::Integer(value)) => value
                .checked_neg()
                .filter(|value| integer_fits(*value, ty))
                .map(ConstValue::Integer)
                .or_else(|| {
                    self.error(format!("constant arithmetic overflows `{ty}`"));
                    None
                }),
            _ => None,
        }
    }

    fn evaluate_binary(
        &mut self,
        left: ConstValue,
        operator: BinaryOp,
        right: ConstValue,
        operand_ty: &Ty,
    ) -> Option<ConstValue> {
        use BinaryOp::*;
        match (left, right) {
            (ConstValue::Integer(left), ConstValue::Integer(right)) => {
                let arithmetic = match operator {
                    Add => left.checked_add(right),
                    Sub => left.checked_sub(right),
                    Mul => left.checked_mul(right),
                    Div if right == 0 => {
                        self.error("division by zero in global constant");
                        return None;
                    }
                    Div => left.checked_div(right),
                    Rem if right == 0 => {
                        self.error("remainder by zero in global constant");
                        return None;
                    }
                    Rem => left.checked_rem(right),
                    Eq => return Some(ConstValue::Bool(left == right)),
                    Ne => return Some(ConstValue::Bool(left != right)),
                    Lt => return Some(ConstValue::Bool(left < right)),
                    Le => return Some(ConstValue::Bool(left <= right)),
                    Gt => return Some(ConstValue::Bool(left > right)),
                    Ge => return Some(ConstValue::Bool(left >= right)),
                    And | Or => unreachable!("short-circuit operators handled separately"),
                };
                arithmetic
                    .filter(|value| integer_fits(*value, operand_ty))
                    .map(ConstValue::Integer)
                    .or_else(|| {
                        self.error(format!("constant arithmetic overflows `{operand_ty}`"));
                        None
                    })
            }
            (ConstValue::Bool(left), ConstValue::Bool(right)) => match operator {
                Eq => Some(ConstValue::Bool(left == right)),
                Ne => Some(ConstValue::Bool(left != right)),
                _ => None,
            },
            _ => None,
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(message));
    }
}

struct Emitter<'a> {
    program: &'a HirProgram,
    constants: HashMap<String, ConstValue>,
}

impl<'a> Emitter<'a> {
    fn new(program: &'a HirProgram, constants: HashMap<String, ConstValue>) -> Self {
        Self { program, constants }
    }

    fn emit_module(&self) -> Result<String, Diagnostic> {
        let mut output = String::new();
        output.push_str("; ModuleID = 'salicin'\nsource_filename = \"salicin\"\n\n");

        for global in &self.program.globals {
            if global.ty == Ty::Unit {
                continue;
            }
            let llvm_ty = llvm_value_type(&global.ty)?;
            let value = self.constants.get(&global.name).ok_or_else(|| {
                Diagnostic::new(format!("constant `{}` was not evaluated", global.name))
            })?;
            output.push_str(&format!(
                "@{} = internal unnamed_addr constant {} {}\n",
                global_symbol(&global.name),
                llvm_ty,
                const_ir(value, &global.ty)?
            ));
        }
        if !self.program.globals.is_empty() {
            output.push('\n');
        }

        for function in &self.program.functions {
            let mut emitter = FunctionEmitter::new(function);
            output.push_str(&emitter.emit()?);
            output.push('\n');
        }

        let main = self
            .program
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("entry point checked by analyzer");
        output.push_str("define i32 @main() {\nentry:\n");
        match main.result {
            Ty::Unit => {
                output.push_str(&format!("  call void @{}()\n", function_symbol("main")));
                output.push_str("  ret i32 0\n");
            }
            Ty::I32 => {
                output.push_str(&format!(
                    "  %status = call i32 @{}()\n",
                    function_symbol("main")
                ));
                output.push_str("  ret i32 %status\n");
            }
            _ => unreachable!("entry result checked by analyzer"),
        }
        output.push_str("}\n");
        Ok(output)
    }
}

#[derive(Debug, Clone)]
struct Operand {
    ty: Ty,
    value: Option<String>,
}

impl Operand {
    fn unit() -> Self {
        Self {
            ty: Ty::Unit,
            value: None,
        }
    }

    fn never() -> Self {
        Self {
            ty: Ty::Never,
            value: None,
        }
    }

    fn value(&self) -> Result<&str, Diagnostic> {
        self.value.as_deref().ok_or_else(|| {
            Diagnostic::new(format!("internal error: `{}` has no LLVM value", self.ty))
        })
    }
}

struct FunctionEmitter<'a> {
    function: &'a HirFunction,
    output: String,
    next_register: usize,
    next_label: usize,
    locals: HashMap<LocalId, String>,
    current_label: String,
    terminated: bool,
}

impl<'a> FunctionEmitter<'a> {
    fn new(function: &'a HirFunction) -> Self {
        Self {
            function,
            output: String::new(),
            next_register: 0,
            next_label: 0,
            locals: HashMap::new(),
            current_label: "entry".to_owned(),
            terminated: false,
        }
    }

    fn emit(&mut self) -> Result<String, Diagnostic> {
        let result = llvm_return_type(&self.function.result)?;
        self.output.push_str(&format!(
            "define internal {result} @{}(",
            function_symbol(&self.function.name)
        ));
        let mut emitted_parameter_count = 0;
        for (index, parameter) in self.function.params.iter().enumerate() {
            if parameter.ty == Ty::Unit {
                continue;
            }
            if emitted_parameter_count != 0 {
                self.output.push_str(", ");
            }
            self.output
                .push_str(&format!("{} %arg.{index}", llvm_value_type(&parameter.ty)?));
            emitted_parameter_count += 1;
        }
        self.output.push_str(") {\nentry:\n");

        for (index, parameter) in self.function.params.iter().enumerate() {
            if parameter.ty == Ty::Unit {
                continue;
            }
            let pointer = self.fresh_register();
            let ty = llvm_value_type(&parameter.ty)?;
            self.instruction(format!(
                "{pointer} = alloca {ty} ; {}",
                llvm_comment(&parameter.name)
            ));
            self.instruction(format!("store {ty} %arg.{index}, ptr {pointer}"));
            self.locals.insert(parameter.id, pointer);
        }

        let body = self.emit_expr(&self.function.body)?;
        if !self.terminated {
            match self.function.result {
                Ty::Unit => self.terminate("ret void"),
                _ => {
                    let ty = llvm_value_type(&self.function.result)?;
                    self.terminate(format!("ret {ty} {}", body.value()?));
                }
            }
        }
        self.output.push_str("}\n");
        Ok(std::mem::take(&mut self.output))
    }

    fn emit_expr(&mut self, expression: &HirExpr) -> Result<Operand, Diagnostic> {
        if self.terminated {
            return Ok(Operand::never());
        }
        match &expression.kind {
            HirExprKind::Integer(value) => Ok(Operand {
                ty: expression.ty.clone(),
                value: Some(value.to_string()),
            }),
            HirExprKind::Bool(value) => Ok(Operand {
                ty: Ty::Bool,
                value: Some(if *value { "1" } else { "0" }.to_owned()),
            }),
            HirExprKind::Unit => Ok(Operand::unit()),
            HirExprKind::Local(id) => {
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let pointer = self.locals.get(id).cloned().ok_or_else(|| {
                    Diagnostic::new(format!("internal error: unknown local id {id}"))
                })?;
                let register = self.fresh_register();
                let ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!("{register} = load {ty}, ptr {pointer}"));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Global(name) => {
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let register = self.fresh_register();
                let ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!(
                    "{register} = load {ty}, ptr @{}",
                    global_symbol(name)
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Function(name) => Err(Diagnostic::new(format!(
                "function value `{name}` reached M0 LLVM emission"
            ))),
            HirExprKind::Unary(operator, operand) => {
                let operand = self.emit_expr(operand)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let register = self.fresh_register();
                match operator {
                    UnaryOp::Neg => {
                        let ty = llvm_value_type(&operand.ty)?;
                        self.instruction(format!("{register} = sub {ty} 0, {}", operand.value()?));
                    }
                    UnaryOp::Not => {
                        self.instruction(format!("{register} = xor i1 {}, true", operand.value()?))
                    }
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Binary(left, BinaryOp::And, right) => {
                self.emit_short_circuit(left, right, false)
            }
            HirExprKind::Binary(left, BinaryOp::Or, right) => {
                self.emit_short_circuit(left, right, true)
            }
            HirExprKind::Binary(left, operator, right) => {
                let left = self.emit_expr(left)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let right = self.emit_expr(right)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let register = self.fresh_register();
                let ty = llvm_value_type(&left.ty)?;
                let instruction = match operator {
                    BinaryOp::Add => "add",
                    BinaryOp::Sub => "sub",
                    BinaryOp::Mul => "mul",
                    BinaryOp::Div if left.ty.is_signed() => "sdiv",
                    BinaryOp::Div => "udiv",
                    BinaryOp::Rem if left.ty.is_signed() => "srem",
                    BinaryOp::Rem => "urem",
                    BinaryOp::Eq => "icmp eq",
                    BinaryOp::Ne => "icmp ne",
                    BinaryOp::Lt if left.ty.is_signed() => "icmp slt",
                    BinaryOp::Lt => "icmp ult",
                    BinaryOp::Le if left.ty.is_signed() => "icmp sle",
                    BinaryOp::Le => "icmp ule",
                    BinaryOp::Gt if left.ty.is_signed() => "icmp sgt",
                    BinaryOp::Gt => "icmp ugt",
                    BinaryOp::Ge if left.ty.is_signed() => "icmp sge",
                    BinaryOp::Ge => "icmp uge",
                    BinaryOp::And | BinaryOp::Or => unreachable!(),
                };
                self.instruction(format!(
                    "{register} = {instruction} {ty} {}, {}",
                    left.value()?,
                    right.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirExprKind::Assign(id, value) => {
                let value = self.emit_expr(value)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if value.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let pointer = self.locals.get(id).ok_or_else(|| {
                    Diagnostic::new(format!("internal error: unknown local id {id}"))
                })?;
                let ty = llvm_value_type(&value.ty)?;
                self.instruction(format!("store {ty} {}, ptr {pointer}", value.value()?));
                Ok(Operand::unit())
            }
            HirExprKind::Call {
                function,
                arguments,
            } => {
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    let argument = self.emit_expr(argument)?;
                    if self.terminated {
                        return Ok(Operand::never());
                    }
                    if argument.ty == Ty::Unit {
                        continue;
                    }
                    emitted_arguments.push(format!(
                        "{} {}",
                        llvm_value_type(&argument.ty)?,
                        argument.value()?
                    ));
                }
                let call = format!(
                    "call {} @{}({})",
                    llvm_return_type(&expression.ty)?,
                    function_symbol(function),
                    emitted_arguments.join(", ")
                );
                if expression.ty == Ty::Unit {
                    self.instruction(call);
                    Ok(Operand::unit())
                } else {
                    let register = self.fresh_register();
                    self.instruction(format!("{register} = {call}"));
                    Ok(Operand {
                        ty: expression.ty.clone(),
                        value: Some(register),
                    })
                }
            }
            HirExprKind::Block(statements, tail) => {
                for statement in statements {
                    if self.terminated {
                        break;
                    }
                    match statement {
                        HirStmt::Let(binding) => {
                            let value = self.emit_expr(&binding.value)?;
                            if self.terminated {
                                break;
                            }
                            if binding.ty == Ty::Unit {
                                continue;
                            }
                            let pointer = self.fresh_register();
                            let ty = llvm_value_type(&binding.ty)?;
                            self.instruction(format!(
                                "{pointer} = alloca {ty} ; {}",
                                llvm_comment(&binding.name)
                            ));
                            self.instruction(format!(
                                "store {ty} {}, ptr {pointer}",
                                value.value()?
                            ));
                            self.locals.insert(binding.id, pointer);
                        }
                        HirStmt::Expr(expression) => {
                            self.emit_expr(expression)?;
                        }
                    }
                }
                if self.terminated {
                    Ok(Operand::never())
                } else if let Some(tail) = tail {
                    self.emit_expr(tail)
                } else {
                    Ok(Operand::unit())
                }
            }
            HirExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.emit_if(expression, condition, then_branch, else_branch.as_deref()),
            HirExprKind::Return(value) => {
                if let Some(value) = value {
                    let value = self.emit_expr(value)?;
                    if self.terminated {
                        return Ok(Operand::never());
                    }
                    if value.ty == Ty::Unit {
                        self.terminate("ret void");
                    } else {
                        let ty = llvm_value_type(&value.ty)?;
                        self.terminate(format!("ret {ty} {}", value.value()?));
                    }
                } else {
                    self.terminate("ret void");
                }
                Ok(Operand::never())
            }
        }
    }

    fn emit_short_circuit(
        &mut self,
        left: &HirExpr,
        right: &HirExpr,
        short_value: bool,
    ) -> Result<Operand, Diagnostic> {
        let left = self.emit_expr(left)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let left_label = self.current_label.clone();
        let right_label = self.fresh_label("logic.rhs");
        let merge_label = self.fresh_label("logic.end");
        if short_value {
            self.terminate(format!(
                "br i1 {}, label %{merge_label}, label %{right_label}",
                left.value()?
            ));
        } else {
            self.terminate(format!(
                "br i1 {}, label %{right_label}, label %{merge_label}",
                left.value()?
            ));
        }

        self.start_block(&right_label);
        let right = self.emit_expr(right)?;
        let right_end = self.current_label.clone();
        let right_reaches_merge = !self.terminated;
        if right_reaches_merge {
            self.terminate(format!("br label %{merge_label}"));
        }

        self.start_block(&merge_label);
        let short = if short_value { 1 } else { 0 };
        if !right_reaches_merge {
            return Ok(Operand {
                ty: Ty::Bool,
                value: Some(short.to_string()),
            });
        }
        let register = self.fresh_register();
        self.instruction(format!(
            "{register} = phi i1 [{short}, %{left_label}], [{}, %{right_end}]",
            right.value()?
        ));
        Ok(Operand {
            ty: Ty::Bool,
            value: Some(register),
        })
    }

    fn emit_if(
        &mut self,
        expression: &HirExpr,
        condition: &HirExpr,
        then_branch: &HirExpr,
        else_branch: Option<&HirExpr>,
    ) -> Result<Operand, Diagnostic> {
        let condition = self.emit_expr(condition)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let then_label = self.fresh_label("if.then");
        let else_label = self.fresh_label("if.else");
        let merge_label = self.fresh_label("if.end");
        self.terminate(format!(
            "br i1 {}, label %{then_label}, label %{else_label}",
            condition.value()?
        ));

        self.start_block(&then_label);
        let then_value = self.emit_expr(then_branch)?;
        let then_end = self.current_label.clone();
        let then_reaches_merge = !self.terminated;
        if then_reaches_merge {
            self.terminate(format!("br label %{merge_label}"));
        }

        self.start_block(&else_label);
        let else_value = if let Some(else_branch) = else_branch {
            self.emit_expr(else_branch)?
        } else {
            Operand::unit()
        };
        let else_end = self.current_label.clone();
        let else_reaches_merge = !self.terminated;
        if else_reaches_merge {
            self.terminate(format!("br label %{merge_label}"));
        }

        if !then_reaches_merge && !else_reaches_merge {
            self.terminated = true;
            return Ok(Operand::never());
        }
        self.start_block(&merge_label);
        if expression.ty == Ty::Unit {
            return Ok(Operand::unit());
        }
        if !then_reaches_merge {
            return Ok(else_value);
        }
        if !else_reaches_merge {
            return Ok(then_value);
        }
        let register = self.fresh_register();
        let ty = llvm_value_type(&expression.ty)?;
        self.instruction(format!(
            "{register} = phi {ty} [{}, %{then_end}], [{}, %{else_end}]",
            then_value.value()?,
            else_value.value()?
        ));
        Ok(Operand {
            ty: expression.ty.clone(),
            value: Some(register),
        })
    }

    fn fresh_register(&mut self) -> String {
        let register = format!("%v{}", self.next_register);
        self.next_register += 1;
        register
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!("{prefix}.{}", self.next_label);
        self.next_label += 1;
        label
    }

    fn instruction(&mut self, instruction: impl fmt::Display) {
        debug_assert!(!self.terminated);
        self.output.push_str("  ");
        self.output.push_str(&instruction.to_string());
        self.output.push('\n');
    }

    fn terminate(&mut self, instruction: impl fmt::Display) {
        self.instruction(instruction);
        self.terminated = true;
    }

    fn start_block(&mut self, label: &str) {
        self.output.push_str(label);
        self.output.push_str(":\n");
        self.current_label = label.to_owned();
        self.terminated = false;
    }
}

fn llvm_return_type(ty: &Ty) -> Result<&'static str, Diagnostic> {
    if *ty == Ty::Unit {
        Ok("void")
    } else {
        llvm_value_type(ty)
    }
}

fn llvm_value_type(ty: &Ty) -> Result<&'static str, Diagnostic> {
    ty.llvm().ok_or_else(|| {
        Diagnostic::new(format!(
            "internal error: `{ty}` has no first-class LLVM representation"
        ))
    })
}

fn const_ir(value: &ConstValue, ty: &Ty) -> Result<String, Diagnostic> {
    match (value, ty) {
        (ConstValue::Integer(value), ty) if ty.is_integer() => Ok(value.to_string()),
        (ConstValue::Bool(value), Ty::Bool) => Ok(if *value { "1" } else { "0" }.to_owned()),
        (ConstValue::Unit, Ty::Unit) => Ok(String::new()),
        _ => Err(Diagnostic::new(format!(
            "internal error: constant value does not have type `{ty}`"
        ))),
    }
}

fn function_symbol(name: &str) -> String {
    format!("sali.fn.{}", hex_name(name))
}

fn global_symbol(name: &str) -> String {
    format!("sali.global.{}", hex_name(name))
}

fn hex_name(name: &str) -> String {
    let mut output = String::with_capacity(name.len() * 2);
    for byte in name.as_bytes() {
        use fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn llvm_comment(name: &str) -> String {
    name.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Param;

    fn function(name: &str, groups: Vec<Vec<Param>>, result: Type, body: Expr) -> Item {
        Item::Function(Function {
            name: name.to_owned(),
            groups,
            return_type: Some(result),
            body: Some(body),
        })
    }

    fn param(name: &str, ty: Type) -> Param {
        Param {
            mode: PassMode::Inferred,
            name: name.to_owned(),
            ty,
        }
    }

    #[test]
    fn emits_flattened_curried_call_and_i32_wrapper() {
        let add = function(
            "add",
            vec![vec![param("x", Type::I32)], vec![param("y", Type::I32)]],
            Type::I32,
            Expr::Binary(
                Box::new(Expr::Name("x".into())),
                BinaryOp::Add,
                Box::new(Expr::Name("y".into())),
            ),
        );
        let call = Expr::Call(
            Box::new(Expr::Call(
                Box::new(Expr::Name("add".into())),
                vec![Expr::Integer(20)],
            )),
            vec![Expr::Integer(22)],
        );
        let main = function("main", vec![vec![]], Type::I32, call);
        let ir = compile(&Program {
            items: vec![add, main],
        })
        .unwrap();
        assert!(ir.contains("call i32 @sali.fn.616464(i32 20, i32 22)"));
        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("ret i32 %status"));
    }

    #[test]
    fn emits_global_if_mutation_and_short_circuit() {
        let global = Item::Global(Binding {
            mutable: false,
            name: "answer".into(),
            annotation: Some(Type::I64),
            value: Expr::Binary(
                Box::new(Expr::Integer(40)),
                BinaryOp::Add,
                Box::new(Expr::Integer(2)),
            ),
        });
        let body = Expr::Block(
            vec![
                Stmt::Let(Binding {
                    mutable: true,
                    name: "x".into(),
                    annotation: Some(Type::I32),
                    value: Expr::Integer(0),
                }),
                Stmt::Expr(Expr::If {
                    condition: Box::new(Expr::Binary(
                        Box::new(Expr::Bool(true)),
                        BinaryOp::And,
                        Box::new(Expr::Bool(false)),
                    )),
                    then_branch: Box::new(Expr::Block(
                        vec![Stmt::Expr(Expr::Assign(
                            "x".into(),
                            Box::new(Expr::Integer(1)),
                        ))],
                        None,
                    )),
                    else_branch: None,
                }),
            ],
            Some(Box::new(Expr::Name("x".into()))),
        );
        let main = function("main", vec![vec![]], Type::I32, body);
        let ir = compile(&Program {
            items: vec![global, main],
        })
        .unwrap();
        assert!(ir.contains("@sali.global.616e73776572 = internal unnamed_addr constant i64 42"));
        assert!(ir.contains("phi i1"));
        assert!(ir.contains("store i32 1"));
    }

    #[test]
    fn unit_entry_uses_i32_c_wrapper() {
        let main = function("main", vec![vec![]], Type::Void, Expr::Unit);
        let ir = compile(&Program { items: vec![main] }).unwrap();
        assert!(ir.contains("define internal void @sali.fn.6d61696e()"));
        assert!(ir.contains("call void @sali.fn.6d61696e()"));
        assert!(ir.contains("ret i32 0"));
    }

    #[test]
    fn short_circuit_rhs_may_return_without_a_second_terminator() {
        let body = Expr::Block(
            vec![
                Stmt::Expr(Expr::Binary(
                    Box::new(Expr::Bool(true)),
                    BinaryOp::And,
                    Box::new(Expr::Return(Some(Box::new(Expr::Integer(1))))),
                )),
                Stmt::Expr(Expr::Binary(
                    Box::new(Expr::Bool(false)),
                    BinaryOp::Or,
                    Box::new(Expr::Return(Some(Box::new(Expr::Integer(2))))),
                )),
            ],
            Some(Box::new(Expr::Integer(0))),
        );
        let main = function("main", vec![vec![]], Type::I32, body);
        let ir = compile(&Program { items: vec![main] }).unwrap();
        assert!(ir.contains("ret i32 1"));
        assert!(ir.contains("ret i32 2"));
        assert!(!ir.contains("phi i1"));
    }

    #[test]
    fn accepts_minimum_signed_integer_literals() {
        let minimum_i64 = function(
            "minimum_i64",
            vec![vec![]],
            Type::I64,
            Expr::Unary(
                UnaryOp::Neg,
                Box::new(Expr::Integer(9_223_372_036_854_775_808)),
            ),
        );
        let main = function(
            "main",
            vec![vec![]],
            Type::I32,
            Expr::Unary(UnaryOp::Neg, Box::new(Expr::Integer(2_147_483_648))),
        );
        let ir = compile(&Program {
            items: vec![minimum_i64, main],
        })
        .unwrap();
        assert!(ir.contains("sub i64 0, 9223372036854775808"));
        assert!(ir.contains("sub i32 0, 2147483648"));
    }

    #[test]
    fn infers_if_integer_literal_from_the_other_branch() {
        let choose = function(
            "choose",
            vec![vec![param("flag", Type::Bool), param("wide", Type::I64)]],
            Type::I64,
            Expr::If {
                condition: Box::new(Expr::Name("flag".into())),
                then_branch: Box::new(Expr::Block(
                    vec![],
                    Some(Box::new(Expr::Name("wide".into()))),
                )),
                else_branch: Some(Box::new(Expr::Block(
                    vec![],
                    Some(Box::new(Expr::Integer(0))),
                ))),
            },
        );
        let main = function("main", vec![vec![]], Type::I32, Expr::Integer(0));
        let ir = compile(&Program {
            items: vec![choose, main],
        })
        .unwrap();
        assert!(ir.contains("phi i64"));
    }

    #[test]
    fn rejects_explicit_move_until_move_tracking_exists() {
        let consume = function(
            "consume",
            vec![vec![Param {
                mode: PassMode::Move,
                name: "value".into(),
                ty: Type::I32,
            }]],
            Type::I32,
            Expr::Name("value".into()),
        );
        let main = function("main", vec![vec![]], Type::I32, Expr::Integer(0));
        let errors = compile(&Program {
            items: vec![consume, main],
        })
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("uses `move`")));
    }

    #[test]
    fn rejects_partial_application_with_specific_diagnostic() {
        let add = function(
            "add",
            vec![vec![param("x", Type::I32)], vec![param("y", Type::I32)]],
            Type::I32,
            Expr::Binary(
                Box::new(Expr::Name("x".into())),
                BinaryOp::Add,
                Box::new(Expr::Name("y".into())),
            ),
        );
        let main = function(
            "main",
            vec![vec![]],
            Type::I32,
            Expr::Call(Box::new(Expr::Name("add".into())), vec![Expr::Integer(1)]),
        );
        let errors = compile(&Program {
            items: vec![add, main],
        })
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("partial application")));
    }
}
