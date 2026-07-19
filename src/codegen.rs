//! Type checking and textual LLVM IR generation for Salicin's M0 subset.
//!
//! The backend intentionally consumes the parser AST directly but first lowers
//! it to a small typed representation.  No malformed program reaches the LLVM
//! emitter, which keeps the generated IR simple enough to inspect in tests.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ast::{
    BinaryOp, Binding, CallArg, EnumDef, Expr, Function, Item, MatchArm, PassMode, Pattern,
    PatternFields, Program, Stmt, StructDef, Type, UnaryOp, VariantFields,
};

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
    Struct(String),
    Enum(String),
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
            Self::Struct(name) | Self::Enum(name) => f.write_str(name),
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
struct FieldLayout {
    name: String,
    ty: Ty,
}

#[derive(Debug, Clone)]
struct StructLayout {
    name: String,
    fields: Vec<FieldLayout>,
}

#[derive(Debug, Clone)]
struct VariantLayout {
    name: String,
    fields: Vec<FieldLayout>,
    payload_offset: usize,
    named: bool,
}

#[derive(Debug, Clone)]
struct EnumLayout {
    name: String,
    variants: Vec<VariantLayout>,
}

#[derive(Debug, Clone)]
struct HirProgram {
    structs: Vec<StructLayout>,
    enums: Vec<EnumLayout>,
    globals: Vec<HirGlobal>,
    functions: Vec<HirFunction>,
}

impl HirProgram {
    fn struct_layout(&self, name: &str) -> Option<&StructLayout> {
        self.structs.iter().find(|layout| layout.name == name)
    }

    fn enum_layout(&self, name: &str) -> Option<&EnumLayout> {
        self.enums.iter().find(|layout| layout.name == name)
    }
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
    mode: PassMode,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HirPlace {
    local: LocalId,
    root_ty: Ty,
    projections: Vec<usize>,
    ty: Ty,
    capability: LocalCapability,
    root_mutable: bool,
    loan: Option<LoanId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HirReadKind {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    Auto,
    Copy,
    Move,
    SharedBorrow,
    MutBorrow,
}

#[derive(Debug, Clone)]
enum HirArgument {
    Copy(HirExpr),
    Move(HirExpr),
    SharedBorrow(HirPlace),
    MutBorrow(HirPlace),
}

#[derive(Debug, Clone)]
struct HirPatternBinding {
    id: LocalId,
    name: String,
    ty: Ty,
    path: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HirMatcher {
    Variant(usize),
    All,
}

#[derive(Debug, Clone)]
struct HirMatchArm {
    matcher: HirMatcher,
    bindings: Vec<HirPatternBinding>,
    guard: Option<HirExpr>,
    body: HirExpr,
}

#[derive(Debug, Clone)]
enum HirExprKind {
    Integer(i128),
    Bool(bool),
    Unit,
    Read {
        place: HirPlace,
        kind: HirReadKind,
    },
    Global(String),
    Function(String),
    Unary(UnaryOp, Box<HirExpr>),
    Binary(Box<HirExpr>, BinaryOp, Box<HirExpr>),
    Assign(HirPlace, Box<HirExpr>),
    Call {
        function: String,
        arguments: Vec<HirArgument>,
    },
    Partial {
        function: String,
        consumed_groups: usize,
        captures: Vec<HirArgument>,
    },
    PartialCapture {
        binding: LocalId,
        index: usize,
    },
    LocalClosure(ClosureInfo),
    ConstructStruct {
        name: String,
        fields: Vec<(usize, HirExpr)>,
    },
    ConstructEnum {
        name: String,
        variant: usize,
        fields: Vec<(usize, HirExpr)>,
    },
    Field {
        base: Box<HirExpr>,
        index: usize,
    },
    Borrow {
        place: HirPlace,
        mutable: bool,
    },
    Block(Vec<HirStmt>, Option<Box<HirExpr>>),
    If {
        condition: Box<HirExpr>,
        then_branch: Box<HirExpr>,
        else_branch: Option<Box<HirExpr>>,
    },
    Return(Option<Box<HirExpr>>),
    Match {
        scrutinee: Box<HirExpr>,
        arms: Vec<HirMatchArm>,
    },
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
    capability: LocalCapability,
    alias: Option<HirPlace>,
    partial: Option<PartialInfo>,
    closure: Option<ClosureInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum LocalCapability {
    Owned,
    SharedParam,
    MutParam,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PlaceKey {
    local: LocalId,
    projections: Vec<usize>,
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
enum MoveStatus {
    Moved,
    MaybeMoved,
}

type LoanId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoanKind {
    Shared,
    Mutable,
}

#[derive(Debug, Clone)]
struct Loan {
    place: PlaceKey,
    kind: LoanKind,
}

#[derive(Debug, Clone)]
struct FlowState {
    reachable: bool,
    moves: HashMap<PlaceKey, MoveStatus>,
    loans: HashMap<LoanId, Loan>,
}

impl Default for FlowState {
    fn default() -> Self {
        Self {
            reachable: true,
            moves: HashMap::new(),
            loans: HashMap::new(),
        }
    }
}

impl FlowState {
    fn join(flows: &[Self]) -> Self {
        let reachable: Vec<_> = flows.iter().filter(|flow| flow.reachable).collect();
        match reachable.as_slice() {
            [] => Self {
                reachable: false,
                moves: HashMap::new(),
                loans: HashMap::new(),
            },
            [only] => (*only).clone(),
            _ => {
                let places: HashSet<_> = reachable
                    .iter()
                    .flat_map(|flow| flow.moves.keys().cloned())
                    .collect();
                let moves = places
                    .into_iter()
                    .map(|place| {
                        let statuses: Vec<_> = reachable
                            .iter()
                            .map(|flow| flow.moves.get(&place).copied())
                            .collect();
                        let status = if statuses
                            .iter()
                            .all(|status| *status == Some(MoveStatus::Moved))
                        {
                            MoveStatus::Moved
                        } else {
                            MoveStatus::MaybeMoved
                        };
                        (place, status)
                    })
                    .collect();
                let loans = reachable
                    .iter()
                    .flat_map(|flow| flow.loans.iter().map(|(id, loan)| (*id, loan.clone())))
                    .collect();
                Self {
                    reachable: true,
                    moves,
                    loans,
                }
            }
        }
    }
}

struct ScopeFrame {
    names: HashMap<String, LocalInfo>,
    locals: Vec<LocalId>,
    lexical_loans: Vec<LoanId>,
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

#[derive(Debug, Clone)]
struct PartialInfo {
    function: String,
    consumed_groups: usize,
    capture_count: usize,
}

#[derive(Debug, Clone)]
struct ClosureInfo {
    function: String,
    params: Vec<ParamSig>,
    result: Ty,
    captures: Vec<ClosureCapture>,
    is_fn_mut: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClosureCaptureMode {
    Shared,
    Mutable,
}

#[derive(Debug, Clone)]
struct ClosureCapture {
    place: HirPlace,
    mode: ClosureCaptureMode,
}

#[derive(Debug, Clone)]
struct ClosureCaptureUse {
    name: String,
    mode: ClosureCaptureMode,
}

struct LowerCtx {
    scopes: Vec<ScopeFrame>,
    flow: FlowState,
    next_local: LocalId,
    next_loan: LoanId,
    declared_result: Option<Ty>,
    returned_types: Vec<Ty>,
    function_name: Option<String>,
}

impl LowerCtx {
    fn for_function(name: &str, result: Option<Ty>) -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            flow: FlowState::default(),
            next_local: 0,
            next_loan: 0,
            declared_result: result,
            returned_types: Vec::new(),
            function_name: Some(name.to_owned()),
        }
    }

    fn for_global() -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            flow: FlowState::default(),
            next_local: 0,
            next_loan: 0,
            declared_result: None,
            returned_types: Vec::new(),
            function_name: None,
        }
    }

    fn lookup(&self, name: &str) -> Option<&LocalInfo> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.names.get(name))
    }

    fn fresh_local(&mut self) -> LocalId {
        let id = self.next_local;
        self.next_local += 1;
        id
    }

    fn push_scope(&mut self) {
        self.scopes.push(ScopeFrame::new());
    }

    fn pop_scope(&mut self) {
        let scope = self.scopes.pop().expect("cannot pop all scopes");
        for loan in scope.lexical_loans {
            self.flow.loans.remove(&loan);
        }
        self.flow
            .moves
            .retain(|place, _| !scope.locals.contains(&place.local));
    }

    fn flow_without_current_scope(&self, mut flow: FlowState) -> FlowState {
        let scope = self.scopes.last().expect("at least one scope");
        for loan in &scope.lexical_loans {
            flow.loans.remove(loan);
        }
        flow.moves
            .retain(|place, _| !scope.locals.contains(&place.local));
        flow
    }

    fn insert_local(&mut self, name: String, local: LocalInfo) -> bool {
        let scope = self.scopes.last_mut().expect("at least one scope");
        if scope.names.contains_key(&name) {
            return false;
        }
        scope.locals.push(local.id);
        scope.names.insert(name, local);
        true
    }
}

struct Analyzer {
    functions: HashMap<String, Function>,
    globals: HashMap<String, Binding>,
    struct_defs: HashMap<String, StructDef>,
    enum_defs: HashMap<String, EnumDef>,
    struct_layouts: HashMap<String, StructLayout>,
    enum_layouts: HashMap<String, EnumLayout>,
    function_order: Vec<String>,
    global_order: Vec<String>,
    struct_order: Vec<String>,
    enum_order: Vec<String>,
    signatures: HashMap<String, FunctionSig>,
    global_annotations: HashMap<String, Option<Ty>>,
    function_states: HashMap<String, ResolutionState>,
    global_states: HashMap<String, ResolutionState>,
    hir_functions: HashMap<String, HirFunction>,
    lifted_functions: Vec<HirFunction>,
    next_closure: usize,
    hir_globals: HashMap<String, HirGlobal>,
    diagnostics: Vec<Diagnostic>,
}

impl Analyzer {
    fn new(program: &Program) -> Self {
        let mut analyzer = Self {
            functions: HashMap::new(),
            globals: HashMap::new(),
            struct_defs: HashMap::new(),
            enum_defs: HashMap::new(),
            struct_layouts: HashMap::new(),
            enum_layouts: HashMap::new(),
            function_order: Vec::new(),
            global_order: Vec::new(),
            struct_order: Vec::new(),
            enum_order: Vec::new(),
            signatures: HashMap::new(),
            global_annotations: HashMap::new(),
            function_states: HashMap::new(),
            global_states: HashMap::new(),
            hir_functions: HashMap::new(),
            lifted_functions: Vec::new(),
            next_closure: 0,
            hir_globals: HashMap::new(),
            diagnostics: Vec::new(),
        };
        analyzer.collect_items(program);
        analyzer
    }

    fn collect_items(&mut self, program: &Program) {
        let mut names = HashSet::new();
        for item in &program.items {
            let name = match item {
                Item::Function(function) => &function.name,
                Item::Global(binding) => &binding.name,
                Item::Struct(definition) => &definition.name,
                Item::Enum(definition) => &definition.name,
            };
            if !names.insert(name.clone()) {
                self.error(format!("duplicate top-level name `{name}`"));
                continue;
            }
            match item {
                Item::Function(function) => {
                    self.function_order.push(function.name.clone());
                    self.functions
                        .insert(function.name.clone(), function.clone());
                }
                Item::Global(binding) => {
                    if binding.mutable {
                        self.error(format!(
                            "mutable global `{}` is not supported yet",
                            binding.name
                        ));
                    }
                    self.global_order.push(binding.name.clone());
                    self.globals.insert(binding.name.clone(), binding.clone());
                }
                Item::Struct(definition) => {
                    self.struct_order.push(definition.name.clone());
                    self.struct_defs
                        .insert(definition.name.clone(), definition.clone());
                }
                Item::Enum(definition) => {
                    self.enum_order.push(definition.name.clone());
                    self.enum_defs
                        .insert(definition.name.clone(), definition.clone());
                }
            }
        }

        self.collect_nominal_layouts();

        for name in self.function_order.clone() {
            let function = self.functions[&name].clone();
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
            self.signatures.insert(name, FunctionSig { groups, result });
        }
        for name in self.global_order.clone() {
            let binding = self.globals[&name].clone();
            let annotation = binding
                .annotation
                .as_ref()
                .map(|ty| self.lower_source_type(ty));
            self.global_annotations.insert(name, annotation);
        }

        self.validate_nominal_layouts();
    }

    fn collect_nominal_layouts(&mut self) {
        for name in self.struct_order.clone() {
            let definition = self.struct_defs[&name].clone();
            let mut seen = HashSet::new();
            let mut fields = Vec::new();
            for field in definition.fields {
                if !seen.insert(field.name.clone()) {
                    self.error(format!(
                        "duplicate field `{}` in struct `{name}`",
                        field.name
                    ));
                    continue;
                }
                fields.push(FieldLayout {
                    name: field.name,
                    ty: self.lower_source_type(&field.ty),
                });
            }
            self.struct_layouts
                .insert(name.clone(), StructLayout { name, fields });
        }

        for name in self.enum_order.clone() {
            let definition = self.enum_defs[&name].clone();
            if definition.variants.is_empty() {
                self.error(format!("enum `{name}` must declare at least one variant"));
            }
            let mut seen_variants = HashSet::new();
            let mut variants = Vec::new();
            let mut payload_offset = 0;
            for variant in definition.variants {
                if !seen_variants.insert(variant.name.clone()) {
                    self.error(format!(
                        "duplicate variant `{}` in enum `{name}`",
                        variant.name
                    ));
                    continue;
                }
                let (source_fields, named) = match variant.fields {
                    VariantFields::Unit => (Vec::new(), false),
                    VariantFields::Positional(types) => (
                        types
                            .into_iter()
                            .enumerate()
                            .map(|(index, ty)| (index.to_string(), ty))
                            .collect(),
                        false,
                    ),
                    VariantFields::Named(fields) => (
                        fields
                            .into_iter()
                            .map(|field| (field.name, field.ty))
                            .collect(),
                        true,
                    ),
                };
                let mut seen_fields = HashSet::new();
                let mut fields = Vec::new();
                for (field_name, source_ty) in source_fields {
                    if !seen_fields.insert(field_name.clone()) {
                        self.error(format!(
                            "duplicate field `{field_name}` in variant `{name}.{}`",
                            variant.name
                        ));
                        continue;
                    }
                    fields.push(FieldLayout {
                        name: field_name,
                        ty: self.lower_source_type(&source_ty),
                    });
                }
                let field_count = fields.len();
                variants.push(VariantLayout {
                    name: variant.name,
                    fields,
                    payload_offset,
                    named,
                });
                payload_offset += field_count;
            }
            self.enum_layouts
                .insert(name.clone(), EnumLayout { name, variants });
        }
    }

    fn validate_nominal_layouts(&mut self) {
        let mut states = HashMap::new();
        let mut stack = Vec::new();
        let names: Vec<_> = self
            .struct_order
            .iter()
            .chain(&self.enum_order)
            .cloned()
            .collect();
        for name in names {
            self.visit_nominal_layout(&name, &mut states, &mut stack);
        }
    }

    fn visit_nominal_layout(
        &mut self,
        name: &str,
        states: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
    ) {
        match states.get(name).copied() {
            Some(2) => return,
            Some(1) => {
                let start = stack.iter().position(|item| item == name).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(name.to_owned());
                self.error(format!(
                    "recursive value layout has infinite size: {}",
                    cycle.join(" -> ")
                ));
                return;
            }
            _ => {}
        }
        states.insert(name.to_owned(), 1);
        stack.push(name.to_owned());
        let dependencies: Vec<String> = if let Some(layout) = self.struct_layouts.get(name) {
            layout
                .fields
                .iter()
                .filter_map(|field| nominal_name(&field.ty).map(str::to_owned))
                .collect()
        } else if let Some(layout) = self.enum_layouts.get(name) {
            layout
                .variants
                .iter()
                .flat_map(|variant| &variant.fields)
                .filter_map(|field| nominal_name(&field.ty).map(str::to_owned))
                .collect()
        } else {
            Vec::new()
        };
        for dependency in dependencies {
            self.visit_nominal_layout(&dependency, states, stack);
        }
        stack.pop();
        states.insert(name.to_owned(), 2);
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

        let mut functions: Vec<_> = self
            .function_order
            .iter()
            .map(|name| self.hir_functions[name].clone())
            .collect();
        functions.extend(self.lifted_functions.clone());
        Some(HirProgram {
            structs: self
                .struct_order
                .iter()
                .map(|name| self.struct_layouts[name].clone())
                .collect(),
            enums: self
                .enum_order
                .iter()
                .map(|name| self.enum_layouts[name].clone())
                .collect(),
            globals: self
                .global_order
                .iter()
                .map(|name| self.hir_globals[name].clone())
                .collect(),
            functions,
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
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) {
                    Ty::Struct(name.clone())
                } else if self.enum_defs.contains_key(name) {
                    Ty::Enum(name.clone())
                } else {
                    self.error(format!("unknown type `{name}`"));
                    Ty::Error
                }
            }
            Type::Named(name, _) => {
                self.error(format!("generic type `{name}` is not supported yet"));
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
                if context.scopes[0].names.contains_key(&param.name) {
                    self.error(format!(
                        "duplicate parameter `{}` in function `{name}`",
                        param.name
                    ));
                    continue;
                }
                let id = context.fresh_local();
                let capability = match param.mode {
                    PassMode::Borrow => LocalCapability::SharedParam,
                    PassMode::MutBorrow => LocalCapability::MutParam,
                    PassMode::Inferred | PassMode::Copy | PassMode::Move => LocalCapability::Owned,
                };
                context.scopes[0].locals.push(id);
                context.scopes[0].names.insert(
                    param.name.clone(),
                    LocalInfo {
                        id,
                        ty: param.ty.clone(),
                        mutable: param.mode == PassMode::MutBorrow,
                        capability,
                        alias: None,
                        partial: None,
                        closure: None,
                    },
                );
                params.push(HirParam {
                    id,
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                    mode: param.mode,
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
        if param.mode == PassMode::Copy && !is_copy_type(&param.ty) {
            self.error(format!(
                "parameter `{}` in function `{function}` requires `Copy`, but nominal type `{}` does not implement Copy",
                param.name, param.ty
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
                if let Some(local) = context.lookup(name).cloned() {
                    if local.partial.is_some() {
                        self.error(format!(
                            "local partial application `{name}` cannot escape; call it directly"
                        ));
                        error_expr()
                    } else if local.closure.is_some() {
                        self.error(format!(
                            "local closure `{name}` cannot escape; call it directly"
                        ));
                        error_expr()
                    } else {
                        let place = self
                            .lower_place(expression, context)
                            .expect("a resolved local name is a place");
                        self.access_place(place, AccessKind::Auto, context)
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
                } else if let Some((enum_name, variant)) =
                    self.resolve_short_variant(name, expected)
                {
                    if self.enum_layouts[&enum_name].variants[variant]
                        .fields
                        .is_empty()
                    {
                        HirExpr {
                            ty: Ty::Enum(enum_name.clone()),
                            kind: HirExprKind::ConstructEnum {
                                name: enum_name,
                                variant,
                                fields: Vec::new(),
                            },
                        }
                    } else {
                        self.error(format!("variant `{name}` requires constructor arguments"));
                        error_expr()
                    }
                } else {
                    self.error(format!("unknown name `{name}`"));
                    error_expr()
                }
            }
            Expr::Borrow { mutable, value } => {
                let Some(mut place) = self.lower_place(value, context) else {
                    return error_expr();
                };
                if *mutable {
                    self.ensure_writable(&place);
                }
                let kind = if *mutable {
                    LoanKind::Mutable
                } else {
                    LoanKind::Shared
                };
                let loan = self.acquire_loan(&place, kind, true, context);
                place.capability = if *mutable {
                    LocalCapability::MutParam
                } else {
                    LocalCapability::SharedParam
                };
                place.loan = loan;
                HirExpr {
                    ty: place.ty.clone(),
                    kind: HirExprKind::Borrow {
                        place,
                        mutable: *mutable,
                    },
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
            Expr::Assign(place, value) => {
                let Some(place) = self.lower_place(place, context) else {
                    return error_expr();
                };
                self.ensure_writable(&place);
                self.ensure_available(&place, context);
                self.ensure_no_conflicting_loan(&place, AccessKind::MutBorrow, context);
                let value = self.lower_expr(value, Some(&place.ty), context);
                self.mark_initialized(&place, context);
                HirExpr {
                    ty: Ty::Unit,
                    kind: HirExprKind::Assign(place, Box::new(value)),
                }
            }
            Expr::Call(_, _) => self.lower_call(expression, expected, context),
            Expr::Member(base, field) => self.lower_member(base, field, context),
            Expr::Block(statements, tail) => {
                context.push_scope();
                let mut lowered_statements = Vec::new();
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) => {
                            let annotation = binding
                                .annotation
                                .as_ref()
                                .map(|ty| self.lower_source_type(ty));
                            let value = match &binding.value {
                                Expr::Closure(params, body) => {
                                    if annotation.is_some() {
                                        self.error(format!(
                                            "closure binding `{}` cannot have a type annotation yet",
                                            binding.name
                                        ));
                                    }
                                    self.lower_local_closure(params, body, context)
                                }
                                _ => self.lower_expr(&binding.value, annotation.as_ref(), context),
                            };
                            let ty = annotation.unwrap_or_else(|| value.ty.clone());
                            let partial = match &value.kind {
                                HirExprKind::Partial {
                                    function,
                                    consumed_groups,
                                    captures,
                                } => Some(PartialInfo {
                                    function: function.clone(),
                                    consumed_groups: *consumed_groups,
                                    capture_count: captures.len(),
                                }),
                                _ => None,
                            };
                            let closure = match &value.kind {
                                HirExprKind::LocalClosure(closure) => Some(closure.clone()),
                                _ => None,
                            };
                            let (capability, alias) = match &value.kind {
                                HirExprKind::Borrow { place, mutable } => (
                                    if *mutable {
                                        LocalCapability::MutParam
                                    } else {
                                        LocalCapability::SharedParam
                                    },
                                    Some(place.clone()),
                                ),
                                _ => (LocalCapability::Owned, None),
                            };
                            if matches!(ty, Ty::Function(_))
                                && partial.is_none()
                                && closure.is_none()
                            {
                                self.error(format!(
                                    "function-valued local `{}` must be a direct partial application",
                                    binding.name
                                ));
                            }
                            if partial.is_some() && binding.mutable {
                                self.error(format!(
                                    "local partial application `{}` must be immutable",
                                    binding.name
                                ));
                            }
                            if closure.as_ref().is_some_and(|closure| closure.is_fn_mut)
                                && !binding.mutable
                            {
                                self.error(format!(
                                    "FnMut closure `{}` requires a mutable binding (`let mut`)",
                                    binding.name
                                ));
                            }
                            let duplicate = context
                                .scopes
                                .last()
                                .expect("block scope")
                                .names
                                .contains_key(&binding.name);
                            if duplicate {
                                self.error(format!(
                                    "duplicate binding `{}` in the same scope",
                                    binding.name
                                ));
                            }
                            let id = context.fresh_local();
                            if !duplicate {
                                context.insert_local(
                                    binding.name.clone(),
                                    LocalInfo {
                                        id,
                                        ty: ty.clone(),
                                        mutable: binding.mutable,
                                        capability,
                                        alias,
                                        partial,
                                        closure,
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
                context.pop_scope();
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
                let entry_flow = context.flow.clone();
                let (then_branch, else_branch, exit_flows) = if let Some(else_ast) =
                    else_branch.as_ref()
                {
                    let (then_branch, then_flow, else_branch, else_flow) = if expected.is_some() {
                        let (then_branch, then_flow) =
                            self.lower_expr_from_flow(then_branch, expected, &entry_flow, context);
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, expected, &entry_flow, context);
                        (then_branch, then_flow, else_branch, else_flow)
                    } else if is_unconstrained_integer(then_branch)
                        && !is_unconstrained_integer(else_ast)
                    {
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, None, &entry_flow, context);
                        let branch_hint = if matches!(else_branch.ty, Ty::Never | Ty::Error) {
                            None
                        } else {
                            Some(&else_branch.ty)
                        };
                        let (then_branch, then_flow) = self.lower_expr_from_flow(
                            then_branch,
                            branch_hint,
                            &entry_flow,
                            context,
                        );
                        (then_branch, then_flow, else_branch, else_flow)
                    } else {
                        let (then_branch, then_flow) =
                            self.lower_expr_from_flow(then_branch, None, &entry_flow, context);
                        let branch_hint = if matches!(then_branch.ty, Ty::Never | Ty::Error) {
                            None
                        } else {
                            Some(&then_branch.ty)
                        };
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, branch_hint, &entry_flow, context);
                        (then_branch, then_flow, else_branch, else_flow)
                    };
                    (
                        then_branch,
                        Some(Box::new(else_branch)),
                        vec![then_flow, else_flow],
                    )
                } else {
                    let (then_branch, then_flow) = self.lower_expr_from_flow(
                        then_branch,
                        Some(&Ty::Unit),
                        &entry_flow,
                        context,
                    );
                    (then_branch, None, vec![then_flow, entry_flow])
                };
                context.flow = FlowState::join(&exit_flows);
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
                context.flow.reachable = false;
                HirExpr {
                    ty: Ty::Never,
                    kind: HirExprKind::Return(value),
                }
            }
            Expr::Match { scrutinee, arms } => self.lower_match(scrutinee, arms, expected, context),
        };

        if let Some(expected) = expected {
            self.require_same_type(&lowered.ty, expected, "expression");
        }
        lowered
    }

    fn lower_expr_from_flow(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        entry: &FlowState,
        context: &mut LowerCtx,
    ) -> (HirExpr, FlowState) {
        context.flow = entry.clone();
        let expression = self.lower_expr(expression, expected, context);
        (expression, context.flow.clone())
    }

    fn lower_local_closure(
        &mut self,
        source_params: &[crate::ast::Param],
        body: &Expr,
        outer: &mut LowerCtx,
    ) -> HirExpr {
        if matches!(body, Expr::Closure(_, _)) {
            self.error("curried closures are not supported yet");
            return error_expr();
        }

        let mut bound: HashSet<String> = source_params
            .iter()
            .map(|param| param.name.clone())
            .collect();
        let mut capture_uses = Vec::new();
        if !self.scan_simple_closure_captures(body, &mut bound, outer, &mut capture_uses) {
            return error_expr();
        }

        let function = format!("__closure.{}", self.next_closure);
        self.next_closure += 1;
        let mut context = LowerCtx::for_function(&function, None);
        let mut hir_params = Vec::new();
        let mut captures = Vec::new();

        let is_fn_mut = capture_uses
            .iter()
            .any(|capture| capture.mode == ClosureCaptureMode::Mutable);
        for capture in capture_uses {
            let name = capture.name;
            let local = outer
                .lookup(&name)
                .cloned()
                .expect("capture scanner only records outer locals");
            if local.partial.is_some() || local.closure.is_some() {
                self.error(format!("closure cannot capture local callable `{name}`"));
                continue;
            }
            if local.alias.is_some() || local.capability != LocalCapability::Owned {
                self.error(format!(
                    "closure capture of borrowed local `{name}` is not supported yet"
                ));
                continue;
            }
            if !matches!(local.ty, Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 | Ty::Bool) {
                self.error(format!(
                    "closure capture `{name}` must be a Copy scalar for now"
                ));
                continue;
            }

            let mut place = HirPlace {
                local: local.id,
                root_ty: local.ty.clone(),
                projections: Vec::new(),
                ty: local.ty.clone(),
                capability: local.capability,
                root_mutable: local.mutable,
                loan: None,
            };
            let (loan_kind, parameter_mode, capability, mutable) = match capture.mode {
                ClosureCaptureMode::Shared => (
                    LoanKind::Shared,
                    PassMode::Borrow,
                    LocalCapability::SharedParam,
                    false,
                ),
                ClosureCaptureMode::Mutable => {
                    self.ensure_writable(&place);
                    (
                        LoanKind::Mutable,
                        PassMode::MutBorrow,
                        LocalCapability::MutParam,
                        true,
                    )
                }
            };
            place.loan = self.acquire_loan(&place, loan_kind, true, outer);
            captures.push(ClosureCapture {
                place,
                mode: capture.mode,
            });

            let id = context.fresh_local();
            context.scopes[0].locals.push(id);
            context.scopes[0].names.insert(
                name.clone(),
                LocalInfo {
                    id,
                    ty: local.ty.clone(),
                    mutable,
                    capability,
                    alias: None,
                    partial: None,
                    closure: None,
                },
            );
            hir_params.push(HirParam {
                id,
                name: format!("capture.{name}"),
                ty: local.ty,
                mode: parameter_mode,
            });
        }

        let mut params = Vec::new();
        for param in source_params {
            if param.mode != PassMode::Inferred {
                self.error(format!(
                    "closure parameter `{}` must use inferred passing mode for now",
                    param.name
                ));
            }
            let ty = self.lower_source_type(&param.ty);
            if context.scopes[0].names.contains_key(&param.name) {
                self.error(format!("duplicate closure parameter `{}`", param.name));
                continue;
            }
            let id = context.fresh_local();
            context.scopes[0].locals.push(id);
            context.scopes[0].names.insert(
                param.name.clone(),
                LocalInfo {
                    id,
                    ty: ty.clone(),
                    mutable: false,
                    capability: LocalCapability::Owned,
                    alias: None,
                    partial: None,
                    closure: None,
                },
            );
            params.push(ParamSig {
                name: param.name.clone(),
                ty: ty.clone(),
                mode: PassMode::Inferred,
            });
            hir_params.push(HirParam {
                id,
                name: param.name.clone(),
                ty,
                mode: PassMode::Inferred,
            });
        }

        let lowered_body = self.lower_expr(body, None, &mut context);
        let mut result = if lowered_body.ty == Ty::Never {
            None
        } else {
            Some(lowered_body.ty.clone())
        };
        for returned in &context.returned_types {
            result = Some(match result {
                Some(current) => self.unify_types(
                    &current,
                    returned,
                    format!("return values in closure `{function}`"),
                ),
                None => returned.clone(),
            });
        }
        let result = result.unwrap_or(Ty::Unit);
        self.lifted_functions.push(HirFunction {
            name: function.clone(),
            params: hir_params,
            result: result.clone(),
            body: lowered_body,
        });

        let info = ClosureInfo {
            function,
            params: params.clone(),
            result: result.clone(),
            captures,
            is_fn_mut,
        };
        HirExpr {
            ty: Ty::Function(FunctionTy {
                groups: vec![params.into_iter().map(|param| param.ty).collect()],
                result: Box::new(result),
            }),
            kind: HirExprKind::LocalClosure(info),
        }
    }

    fn scan_simple_closure_captures(
        &mut self,
        expression: &Expr,
        bound: &mut HashSet<String>,
        outer: &LowerCtx,
        captures: &mut Vec<ClosureCaptureUse>,
    ) -> bool {
        match expression {
            Expr::Unit | Expr::Integer(_) | Expr::Bool(_) => true,
            Expr::Name(name) => {
                if !bound.contains(name) && outer.lookup(name).is_some() {
                    record_closure_capture(captures, name, ClosureCaptureMode::Shared);
                }
                true
            }
            Expr::Unary(_, operand) => {
                self.scan_simple_closure_captures(operand, bound, outer, captures)
            }
            Expr::Binary(left, _, right) => {
                self.scan_simple_closure_captures(left, bound, outer, captures)
                    & self.scan_simple_closure_captures(right, bound, outer, captures)
            }
            Expr::Assign(place, value) => {
                let mut valid = self.scan_simple_closure_captures(value, bound, outer, captures);
                if let Some(name) = place_root_name(place) {
                    if !bound.contains(name) && outer.lookup(name).is_some() {
                        if !matches!(place.as_ref(), Expr::Name(_)) {
                            self.error(
                                "FnMut closure assignment only supports a captured root local for now",
                            );
                            valid = false;
                        } else {
                            record_closure_capture(captures, name, ClosureCaptureMode::Mutable);
                        }
                    }
                }
                valid
            }
            Expr::Block(statements, tail) => {
                let saved = bound.clone();
                let mut valid = true;
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) => {
                            valid &= self.scan_simple_closure_captures(
                                &binding.value,
                                bound,
                                outer,
                                captures,
                            );
                            bound.insert(binding.name.clone());
                        }
                        Stmt::Expr(expression) => {
                            valid &= self
                                .scan_simple_closure_captures(expression, bound, outer, captures);
                        }
                    }
                }
                if let Some(tail) = tail {
                    valid &= self.scan_simple_closure_captures(tail, bound, outer, captures);
                }
                *bound = saved;
                valid
            }
            _ => {
                self.error(
                    "closure body form requires mutable or consuming capture analysis, which is not supported yet",
                );
                false
            }
        }
    }

    fn lower_place(&mut self, expression: &Expr, context: &mut LowerCtx) -> Option<HirPlace> {
        match expression {
            Expr::Name(name) => {
                let Some(local) = context.lookup(name).cloned() else {
                    if self.globals.contains_key(name) {
                        self.error(format!(
                            "global constant `{name}` is not a borrowable place"
                        ));
                    } else {
                        self.error(format!("unknown local `{name}` in place expression"));
                    }
                    return None;
                };
                if local.partial.is_some() || local.closure.is_some() {
                    self.error(format!("local callable `{name}` is not a data place"));
                    return None;
                }
                if let Some(alias) = local.alias {
                    return Some(alias);
                }
                Some(HirPlace {
                    local: local.id,
                    root_ty: local.ty.clone(),
                    projections: Vec::new(),
                    ty: local.ty,
                    capability: local.capability,
                    root_mutable: local.mutable,
                    loan: None,
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
                let layout = self.struct_layouts[struct_name].clone();
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
                place.projections.push(index);
                place.ty = field.ty.clone();
                Some(place)
            }
            _ => {
                self.error("expression is not a local place");
                None
            }
        }
    }

    fn access_place(
        &mut self,
        place: HirPlace,
        requested: AccessKind,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let access = if requested == AccessKind::Auto {
            if is_copy_type(&place.ty) {
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
                if !is_copy_type(&place.ty) {
                    self.error(format!(
                        "type `{}` does not implement Copy and cannot be copied",
                        place.ty
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

    fn ensure_available(&mut self, place: &HirPlace, context: &LowerCtx) {
        if !context.flow.reachable {
            return;
        }
        let requested = PlaceKey::from(place);
        let mut possibly_moved = false;
        for (moved, status) in &context.flow.moves {
            if places_overlap(&requested, moved) {
                match status {
                    MoveStatus::Moved => {
                        self.error("use of moved value");
                        return;
                    }
                    MoveStatus::MaybeMoved => possibly_moved = true,
                }
            }
        }
        if possibly_moved {
            self.error("use of possibly moved value");
        }
    }

    fn ensure_no_conflicting_loan(
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

    fn ensure_writable(&mut self, place: &HirPlace) {
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

    fn mark_moved(&mut self, place: &HirPlace, context: &mut LowerCtx) {
        if !context.flow.reachable {
            return;
        }
        let moved = PlaceKey::from(place);
        context
            .flow
            .moves
            .retain(|existing, _| !is_place_prefix(&moved, existing));
        context.flow.moves.insert(moved, MoveStatus::Moved);
    }

    fn mark_initialized(&mut self, place: &HirPlace, context: &mut LowerCtx) {
        let initialized = PlaceKey::from(place);
        context
            .flow
            .moves
            .retain(|moved, _| !is_place_prefix(&initialized, moved));
    }

    fn acquire_loan(
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

    fn release_loans(&mut self, loans: &[LoanId], context: &mut LowerCtx) {
        for loan in loans {
            context.flow.loans.remove(loan);
        }
    }

    fn lower_member(&mut self, base: &Expr, member: &str, context: &mut LowerCtx) -> HirExpr {
        if let Expr::Name(enum_name) = base {
            if let Some(layout) = self.enum_layouts.get(enum_name).cloned() {
                let Some((variant, variant_layout)) = layout
                    .variants
                    .iter()
                    .enumerate()
                    .find(|(_, variant)| variant.name == member)
                else {
                    self.error(format!("unknown variant `{member}` on enum `{enum_name}`"));
                    return error_expr();
                };
                if !variant_layout.fields.is_empty() {
                    self.error(format!(
                        "variant `{enum_name}.{member}` requires constructor arguments"
                    ));
                    return error_expr();
                }
                return HirExpr {
                    ty: Ty::Enum(enum_name.clone()),
                    kind: HirExprKind::ConstructEnum {
                        name: enum_name.clone(),
                        variant,
                        fields: Vec::new(),
                    },
                };
            }
        }

        if let Some(place) = self.lower_place_without_diagnostic(base, context) {
            let Ty::Struct(struct_name) = &place.ty else {
                self.error(format!(
                    "member access requires a struct value, found `{}`",
                    place.ty
                ));
                return error_expr();
            };
            let layout = self.struct_layouts[struct_name].clone();
            let Some((index, field)) = layout
                .fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == member)
            else {
                self.error(format!(
                    "unknown field `{member}` on struct `{struct_name}`"
                ));
                return error_expr();
            };
            let mut field_place = place;
            field_place.projections.push(index);
            field_place.ty = field.ty.clone();
            return self.access_place(field_place, AccessKind::Auto, context);
        }

        let base = self.lower_expr(base, None, context);
        let Ty::Struct(struct_name) = &base.ty else {
            self.error(format!(
                "member access requires a struct value, found `{}`",
                base.ty
            ));
            return error_expr();
        };
        let layout = self.struct_layouts[struct_name].clone();
        let Some((index, field)) = layout
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == member)
        else {
            self.error(format!(
                "unknown field `{member}` on struct `{struct_name}`"
            ));
            return error_expr();
        };
        HirExpr {
            ty: field.ty.clone(),
            kind: HirExprKind::Field {
                base: Box::new(base),
                index,
            },
        }
    }

    fn lower_place_without_diagnostic(
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
            _ => None,
        }
    }

    fn lower_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let scrutinee = self.lower_expr(scrutinee, None, context);
        let Ty::Enum(enum_name) = &scrutinee.ty else {
            self.error(format!(
                "match currently requires an enum value, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };
        let layout = self.enum_layouts[enum_name].clone();
        let mut lowered_arms = Vec::new();
        let mut covered = vec![false; layout.variants.len()];
        let mut result_ty: Option<Ty> = None;
        let entry_flow = context.flow.clone();
        let mut fallthrough_flows = vec![Some(entry_flow.clone()); layout.variants.len()];
        let mut exit_flows = Vec::new();

        for arm in arms {
            context.push_scope();
            let (matcher, bindings) = self.lower_enum_pattern(&arm.pattern, &layout, context);
            let matched_variants: Vec<_> = match matcher {
                HirMatcher::Variant(index) => vec![index],
                HirMatcher::All => (0..layout.variants.len()).collect(),
            };
            let incoming_flows: Vec<_> = matched_variants
                .iter()
                .filter_map(|index| fallthrough_flows[*index].clone())
                .collect();
            context.flow = FlowState::join(&incoming_flows);
            let guard = arm
                .guard
                .as_ref()
                .map(|guard| self.lower_expr(guard, Some(&Ty::Bool), context));
            let guard_flow = guard.as_ref().map(|_| context.flow.clone());
            let branch_expected = expected.or_else(|| {
                result_ty
                    .as_ref()
                    .filter(|ty| !matches!(ty, Ty::Never | Ty::Error))
            });
            let body = self.lower_expr(&arm.body, branch_expected, context);
            let guard_fallthrough = guard_flow.map(|flow| context.flow_without_current_scope(flow));
            context.pop_scope();
            exit_flows.push(context.flow.clone());

            for variant in &matched_variants {
                if fallthrough_flows[*variant].is_some() {
                    fallthrough_flows[*variant] = guard_fallthrough.clone();
                }
            }

            result_ty = Some(match result_ty {
                Some(current) => self.unify_types(&current, &body.ty, "match arm results"),
                None => body.ty.clone(),
            });
            if guard.is_none() {
                match matcher {
                    HirMatcher::Variant(index) => covered[index] = true,
                    HirMatcher::All => covered.fill(true),
                }
            }
            lowered_arms.push(HirMatchArm {
                matcher,
                bindings,
                guard,
                body,
            });
        }

        exit_flows.extend(fallthrough_flows.into_iter().flatten());
        context.flow = FlowState::join(&exit_flows);

        let missing: Vec<_> = layout
            .variants
            .iter()
            .zip(covered)
            .filter_map(|(variant, covered)| (!covered).then_some(variant.name.as_str()))
            .collect();
        if !missing.is_empty() {
            self.error(format!(
                "match on `{enum_name}` is not exhaustive; missing {}",
                missing.join(", ")
            ));
        }
        if arms.is_empty() {
            self.error("match must contain at least one arm");
        }
        HirExpr {
            ty: result_ty.unwrap_or(Ty::Error),
            kind: HirExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms: lowered_arms,
            },
        }
    }

    fn lower_enum_pattern(
        &mut self,
        pattern: &Pattern,
        layout: &EnumLayout,
        context: &mut LowerCtx,
    ) -> (HirMatcher, Vec<HirPatternBinding>) {
        let mut bindings = Vec::new();
        match pattern {
            Pattern::Wildcard => (HirMatcher::All, bindings),
            Pattern::Binding(name) => {
                self.bind_pattern(
                    name,
                    Ty::Enum(layout.name.clone()),
                    Vec::new(),
                    context,
                    &mut bindings,
                );
                (HirMatcher::All, bindings)
            }
            Pattern::Constructor { path, fields } => {
                let Some(variant_name) = path.last() else {
                    self.error("empty constructor path in pattern");
                    return (HirMatcher::All, bindings);
                };
                if path.len() > 2 || (path.len() == 2 && path[0] != layout.name) {
                    self.error(format!(
                        "pattern constructor `{}` does not belong to enum `{}`",
                        path.join("."),
                        layout.name
                    ));
                    return (HirMatcher::All, bindings);
                }
                let Some(variant_index) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == *variant_name)
                else {
                    self.error(format!(
                        "unknown pattern variant `{variant_name}` for enum `{}`",
                        layout.name
                    ));
                    return (HirMatcher::All, bindings);
                };
                let variant = &layout.variants[variant_index];
                let patterns = self.normalize_pattern_fields(
                    fields,
                    &variant.fields,
                    variant.named,
                    &format!("pattern `{}.{}`", layout.name, variant.name),
                );
                for (field_index, field_pattern) in patterns {
                    let field = &variant.fields[field_index];
                    self.lower_irrefutable_pattern(
                        &field_pattern,
                        &field.ty,
                        vec![1 + variant.payload_offset + field_index],
                        context,
                        &mut bindings,
                    );
                }
                (HirMatcher::Variant(variant_index), bindings)
            }
            Pattern::Integer(_) | Pattern::Bool(_) => {
                self.error(format!(
                    "pattern type mismatch: enum `{}` cannot be matched by a scalar literal",
                    layout.name
                ));
                (HirMatcher::All, bindings)
            }
        }
    }

    fn normalize_pattern_fields(
        &mut self,
        patterns: &PatternFields,
        fields: &[FieldLayout],
        named: bool,
        description: &str,
    ) -> Vec<(usize, Pattern)> {
        match patterns {
            PatternFields::Unit => {
                if !fields.is_empty() {
                    self.error(format!(
                        "{description} requires {} field patterns",
                        fields.len()
                    ));
                }
                Vec::new()
            }
            PatternFields::Positional(patterns) => {
                if patterns.len() != fields.len() {
                    self.error(format!(
                        "field count mismatch in {description}: expected {}, found {}",
                        fields.len(),
                        patterns.len()
                    ));
                }
                patterns.iter().cloned().enumerate().collect()
            }
            PatternFields::Named(patterns) => {
                if !named {
                    self.error(format!("{description} has positional fields"));
                    return Vec::new();
                }
                let mut seen = HashSet::new();
                let mut result = Vec::new();
                for pattern in patterns {
                    let Some(index) = fields.iter().position(|field| field.name == pattern.name)
                    else {
                        self.error(format!("unknown field `{}` in {description}", pattern.name));
                        continue;
                    };
                    if !seen.insert(index) {
                        self.error(format!(
                            "duplicate field `{}` in {description}",
                            pattern.name
                        ));
                        continue;
                    }
                    result.push((index, pattern.pattern.clone()));
                }
                for (index, field) in fields.iter().enumerate() {
                    if !seen.contains(&index) {
                        self.error(format!("missing field `{}` in {description}", field.name));
                    }
                }
                result
            }
        }
    }

    fn lower_irrefutable_pattern(
        &mut self,
        pattern: &Pattern,
        ty: &Ty,
        path: Vec<usize>,
        context: &mut LowerCtx,
        bindings: &mut Vec<HirPatternBinding>,
    ) {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Binding(name) => self.bind_pattern(name, ty.clone(), path, context, bindings),
            Pattern::Integer(_) if ty.is_integer() => self
                .error("literal payload patterns are not supported yet; use a binding and a guard"),
            Pattern::Bool(_) if *ty == Ty::Bool => self
                .error("literal payload patterns are not supported yet; use a binding and a guard"),
            Pattern::Constructor {
                path: constructor,
                fields,
            } => {
                let Ty::Struct(struct_name) = ty else {
                    self.error(format!(
                        "pattern type mismatch: constructor `{}` cannot match `{ty}`",
                        constructor.join(".")
                    ));
                    return;
                };
                if constructor.last() != Some(struct_name) {
                    self.error(format!(
                        "pattern type mismatch: expected struct `{struct_name}`, found `{}`",
                        constructor.join(".")
                    ));
                    return;
                }
                let layout = self.struct_layouts[struct_name].clone();
                let nested = self.normalize_pattern_fields(
                    fields,
                    &layout.fields,
                    true,
                    &format!("pattern `{struct_name}`"),
                );
                for (index, pattern) in nested {
                    let mut nested_path = path.clone();
                    nested_path.push(index);
                    self.lower_irrefutable_pattern(
                        &pattern,
                        &layout.fields[index].ty,
                        nested_path,
                        context,
                        bindings,
                    );
                }
            }
            Pattern::Integer(_) | Pattern::Bool(_) => self.error(format!(
                "pattern type mismatch: literal pattern cannot match `{ty}`"
            )),
        }
    }

    fn bind_pattern(
        &mut self,
        name: &str,
        ty: Ty,
        path: Vec<usize>,
        context: &mut LowerCtx,
        bindings: &mut Vec<HirPatternBinding>,
    ) {
        if context
            .scopes
            .last()
            .expect("match arm scope")
            .names
            .contains_key(name)
        {
            self.error(format!("duplicate pattern binding `{name}`"));
            return;
        }
        let id = context.fresh_local();
        context.insert_local(
            name.to_owned(),
            LocalInfo {
                id,
                ty: ty.clone(),
                mutable: false,
                capability: LocalCapability::Owned,
                alias: None,
                partial: None,
                closure: None,
            },
        );
        bindings.push(HirPatternBinding {
            id,
            name: name.to_owned(),
            ty,
            path,
        });
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
                let skip_flow = context.flow.clone();
                let (right, right_flow) =
                    self.lower_expr_from_flow(right, Some(&Ty::Bool), &skip_flow, context);
                context.flow = FlowState::join(&[skip_flow, right_flow]);
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

    fn lower_call(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        if let Expr::Name(name) = root {
            if self.struct_layouts.contains_key(name) {
                return self.lower_struct_constructor(name, &groups, context);
            }
            if self.functions.contains_key(name) {
                return self.lower_named_function_call(name, &groups, context);
            }
            if let Some(local) = context.lookup(name).cloned() {
                if local.closure.is_some() {
                    return self.lower_local_closure_call(name, &local, &groups, context);
                }
                if local.partial.is_some() {
                    return self.lower_local_partial_call(name, &local, &groups, context);
                }
            }
            if let Some((enum_name, variant)) = self.resolve_short_variant(name, expected) {
                return self.lower_enum_constructor(&enum_name, variant, &groups, context);
            }
            self.error(format!("`{name}` is not a function or constructor"));
            return error_expr();
        }
        if let Expr::Member(base, variant_name) = root {
            if let Expr::Name(enum_name) = base.as_ref() {
                if let Some(layout) = self.enum_layouts.get(enum_name) {
                    if let Some(variant) = layout
                        .variants
                        .iter()
                        .position(|variant| variant.name == *variant_name)
                    {
                        return self.lower_enum_constructor(enum_name, variant, &groups, context);
                    }
                    self.error(format!(
                        "unknown variant `{variant_name}` on enum `{enum_name}`"
                    ));
                    return error_expr();
                }
            }
        }
        self.error("calls require a named function, constructor, or local partial application");
        error_expr()
    }

    fn lower_local_closure_call(
        &mut self,
        local_name: &str,
        local: &LocalInfo,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        let closure = local
            .closure
            .as_ref()
            .expect("closure call requires closure metadata");
        if groups.len() != 1 {
            self.error(format!(
                "closure `{local_name}` requires one complete argument group; closure partial application is not supported"
            ));
            return error_expr();
        }
        if groups[0].len() != closure.params.len() {
            self.error(format!(
                "argument count mismatch in closure `{local_name}`: expected {}, found {}",
                closure.params.len(),
                groups[0].len()
            ));
        }

        let mut arguments: Vec<_> = closure
            .captures
            .iter()
            .cloned()
            .map(|capture| match capture.mode {
                ClosureCaptureMode::Shared => HirArgument::SharedBorrow(capture.place),
                ClosureCaptureMode::Mutable => HirArgument::MutBorrow(capture.place),
            })
            .collect();
        let mut temporary_loans = Vec::new();
        for (argument, parameter) in groups[0].iter().zip(&closure.params) {
            if argument.label.is_some() {
                self.error(format!(
                    "named arguments are not allowed in call to closure `{local_name}`"
                ));
            }
            arguments.push(self.lower_call_argument(
                &argument.value,
                parameter,
                context,
                &mut temporary_loans,
            ));
        }
        self.release_loans(&temporary_loans, context);
        HirExpr {
            ty: closure.result.clone(),
            kind: HirExprKind::Call {
                function: closure.function.clone(),
                arguments,
            },
        }
    }

    fn lower_named_function_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        let function_ty = self.function_type(name);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        let signature = self.signatures[name].clone();
        if groups.len() > function_ty.groups.len() {
            self.error(format!(
                "too many parameter groups in call to `{name}`: expected {}, found {}",
                function_ty.groups.len(),
                groups.len()
            ));
            return error_expr();
        }

        let mut arguments = Vec::new();
        let mut temporary_loans = Vec::new();
        for (group_index, (arguments_ast, params)) in
            groups.iter().zip(&signature.groups).enumerate()
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
                if argument.label.is_some() {
                    self.error(format!(
                        "named arguments are not allowed in call to `{name}`"
                    ));
                }
                arguments.push(self.lower_call_argument(
                    &argument.value,
                    parameter,
                    context,
                    &mut temporary_loans,
                ));
            }
        }

        let complete = groups.len() == function_ty.groups.len();
        if !complete
            && arguments
                .iter()
                .any(|argument| !matches!(argument, HirArgument::Copy(_)))
        {
            self.error("partial application may only capture Copy arguments for now");
        }
        self.release_loans(&temporary_loans, context);

        if complete {
            HirExpr {
                ty: (*function_ty.result).clone(),
                kind: HirExprKind::Call {
                    function: name.to_owned(),
                    arguments,
                },
            }
        } else {
            let remaining = function_ty.groups[groups.len()..].to_vec();
            HirExpr {
                ty: Ty::Function(FunctionTy {
                    groups: remaining,
                    result: function_ty.result.clone(),
                }),
                kind: HirExprKind::Partial {
                    function: name.to_owned(),
                    consumed_groups: groups.len(),
                    captures: arguments,
                },
            }
        }
    }

    fn lower_local_partial_call(
        &mut self,
        local_name: &str,
        local: &LocalInfo,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        let partial = local
            .partial
            .as_ref()
            .expect("partial call requires partial metadata");
        let function_ty = self.function_type(&partial.function);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        let signature = self.signatures[&partial.function].clone();
        let remaining_groups = function_ty.groups.len() - partial.consumed_groups;
        if groups.len() > remaining_groups {
            self.error(format!(
                "too many parameter groups in call to `{local_name}`: expected at most {remaining_groups}, found {}",
                groups.len()
            ));
            return error_expr();
        }

        let captured_params: Vec<_> = signature
            .groups
            .iter()
            .take(partial.consumed_groups)
            .flatten()
            .cloned()
            .collect();
        if captured_params.len() != partial.capture_count {
            self.error(format!(
                "internal error: invalid capture count for partial `{local_name}`"
            ));
            return error_expr();
        }
        let mut arguments: Vec<_> = captured_params
            .into_iter()
            .enumerate()
            .map(|(index, parameter)| {
                let capture = HirExpr {
                    ty: parameter.ty.clone(),
                    kind: HirExprKind::PartialCapture {
                        binding: local.id,
                        index,
                    },
                };
                match effective_pass_mode(parameter.mode, &parameter.ty) {
                    PassMode::Copy => HirArgument::Copy(capture),
                    PassMode::Move => HirArgument::Move(capture),
                    PassMode::Borrow | PassMode::MutBorrow => {
                        unreachable!("borrowed partial applications are rejected at creation")
                    }
                    PassMode::Inferred => unreachable!("effective mode is explicit"),
                }
            })
            .collect();

        let mut temporary_loans = Vec::new();
        for (relative_group, arguments_ast) in groups.iter().enumerate() {
            let group_index = partial.consumed_groups + relative_group;
            let params = &signature.groups[group_index];
            if arguments_ast.len() != params.len() {
                self.error(format!(
                    "argument count mismatch in group {} of `{local_name}`: expected {}, found {}",
                    relative_group + 1,
                    params.len(),
                    arguments_ast.len()
                ));
            }
            for (argument, parameter) in arguments_ast.iter().zip(params) {
                if argument.label.is_some() {
                    self.error(format!(
                        "named arguments are not allowed in call to `{local_name}`"
                    ));
                }
                arguments.push(self.lower_call_argument(
                    &argument.value,
                    parameter,
                    context,
                    &mut temporary_loans,
                ));
            }
        }

        let consumed_groups = partial.consumed_groups + groups.len();
        if consumed_groups != function_ty.groups.len()
            && arguments
                .iter()
                .any(|argument| !matches!(argument, HirArgument::Copy(_)))
        {
            self.error("partial application may only capture Copy arguments for now");
        }
        self.release_loans(&temporary_loans, context);
        if consumed_groups == function_ty.groups.len() {
            HirExpr {
                ty: (*function_ty.result).clone(),
                kind: HirExprKind::Call {
                    function: partial.function.clone(),
                    arguments,
                },
            }
        } else {
            HirExpr {
                ty: Ty::Function(FunctionTy {
                    groups: function_ty.groups[consumed_groups..].to_vec(),
                    result: function_ty.result.clone(),
                }),
                kind: HirExprKind::Partial {
                    function: partial.function.clone(),
                    consumed_groups,
                    captures: arguments,
                },
            }
        }
    }

    fn lower_call_argument(
        &mut self,
        argument: &Expr,
        parameter: &ParamSig,
        context: &mut LowerCtx,
        temporary_loans: &mut Vec<LoanId>,
    ) -> HirArgument {
        let mode = effective_pass_mode(parameter.mode, &parameter.ty);
        match mode {
            PassMode::Copy | PassMode::Move => {
                let value =
                    if let Some(place) = self.lower_place_without_diagnostic(argument, context) {
                        let access = if mode == PassMode::Copy {
                            AccessKind::Copy
                        } else {
                            AccessKind::Move
                        };
                        self.access_place(place, access, context)
                    } else {
                        self.lower_expr(argument, Some(&parameter.ty), context)
                    };
                self.require_same_type(
                    &value.ty,
                    &parameter.ty,
                    format!("argument for parameter `{}`", parameter.name),
                );
                if mode == PassMode::Copy {
                    if !is_copy_type(&parameter.ty) {
                        self.error(format!(
                            "parameter `{}` requires Copy, but `{}` does not implement Copy",
                            parameter.name, parameter.ty
                        ));
                    }
                    HirArgument::Copy(value)
                } else {
                    HirArgument::Move(value)
                }
            }
            PassMode::Borrow | PassMode::MutBorrow => {
                let Some(place) = self.lower_place_without_diagnostic(argument, context) else {
                    self.error(format!(
                        "borrowed argument for parameter `{}` must be a local place",
                        parameter.name
                    ));
                    return HirArgument::Copy(error_expr());
                };
                self.require_same_type(
                    &place.ty,
                    &parameter.ty,
                    format!("argument for parameter `{}`", parameter.name),
                );
                let mutable = mode == PassMode::MutBorrow;
                if mutable {
                    self.ensure_writable(&place);
                }
                let kind = if mutable {
                    LoanKind::Mutable
                } else {
                    LoanKind::Shared
                };
                if let Some(loan) = self.acquire_loan(&place, kind, false, context) {
                    temporary_loans.push(loan);
                }
                if mutable {
                    HirArgument::MutBorrow(place)
                } else {
                    HirArgument::SharedBorrow(place)
                }
            }
            PassMode::Inferred => unreachable!("effective mode is explicit"),
        }
    }

    fn lower_struct_constructor(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 {
            self.error(format!(
                "struct constructor `{name}` expects exactly one argument group"
            ));
            return error_expr();
        }
        let layout = self.struct_layouts[name].clone();
        let fields = self.lower_constructor_fields(
            groups[0],
            &layout.fields,
            true,
            &format!("struct `{name}`"),
            context,
        );
        HirExpr {
            ty: Ty::Struct(name.to_owned()),
            kind: HirExprKind::ConstructStruct {
                name: name.to_owned(),
                fields,
            },
        }
    }

    fn lower_enum_constructor(
        &mut self,
        enum_name: &str,
        variant: usize,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 {
            self.error(format!(
                "enum variant constructor `{enum_name}` expects exactly one argument group"
            ));
            return error_expr();
        }
        let layout = self.enum_layouts[enum_name].clone();
        let variant_layout = &layout.variants[variant];
        if variant_layout.fields.is_empty() {
            self.error(format!(
                "unit variant `{enum_name}.{}` is a value and must not be called",
                variant_layout.name
            ));
            return error_expr();
        }
        let fields = self.lower_constructor_fields(
            groups[0],
            &variant_layout.fields,
            variant_layout.named,
            &format!("variant `{enum_name}.{}`", variant_layout.name),
            context,
        );
        HirExpr {
            ty: Ty::Enum(enum_name.to_owned()),
            kind: HirExprKind::ConstructEnum {
                name: enum_name.to_owned(),
                variant,
                fields,
            },
        }
    }

    fn lower_constructor_fields(
        &mut self,
        arguments: &[CallArg],
        fields: &[FieldLayout],
        labels_allowed: bool,
        constructor: &str,
        context: &mut LowerCtx,
    ) -> Vec<(usize, HirExpr)> {
        let labeled = arguments
            .iter()
            .filter(|argument| argument.label.is_some())
            .count();
        if labeled != 0 && labeled != arguments.len() {
            self.error(format!(
                "cannot mix labeled and positional arguments in {constructor}"
            ));
            return Vec::new();
        }

        if labeled == 0 {
            if arguments.len() != fields.len() {
                self.error(format!(
                    "argument count mismatch for {constructor}: expected {}, found {}",
                    fields.len(),
                    arguments.len()
                ));
            }
            return arguments
                .iter()
                .zip(fields)
                .enumerate()
                .map(|(index, (argument, field))| {
                    (
                        index,
                        self.lower_expr(&argument.value, Some(&field.ty), context),
                    )
                })
                .collect();
        }

        if !labels_allowed {
            self.error(format!("{constructor} does not accept labeled arguments"));
            return Vec::new();
        }
        let mut initialized = HashSet::new();
        let mut lowered = Vec::new();
        for argument in arguments {
            let label = argument
                .label
                .as_deref()
                .expect("all arguments are labeled");
            let Some((index, field)) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == label)
            else {
                self.error(format!("unknown field `{label}` in {constructor}"));
                continue;
            };
            if !initialized.insert(index) {
                self.error(format!("duplicate field `{label}` in {constructor}"));
                continue;
            }
            lowered.push((
                index,
                self.lower_expr(&argument.value, Some(&field.ty), context),
            ));
        }
        for (index, field) in fields.iter().enumerate() {
            if !initialized.contains(&index) {
                self.error(format!("missing field `{}` in {constructor}", field.name));
            }
        }
        lowered
    }

    fn resolve_short_variant(
        &mut self,
        name: &str,
        expected: Option<&Ty>,
    ) -> Option<(String, usize)> {
        if let Some(Ty::Enum(enum_name)) = expected {
            if let Some(index) = self.enum_layouts[enum_name]
                .variants
                .iter()
                .position(|variant| variant.name == name)
            {
                return Some((enum_name.clone(), index));
            }
        }
        let candidates: Vec<_> = self
            .enum_layouts
            .iter()
            .filter_map(|(enum_name, layout)| {
                layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                    .map(|variant| (enum_name.clone(), variant))
            })
            .collect();
        match candidates.as_slice() {
            [candidate] => Some(candidate.clone()),
            [] => None,
            _ => {
                self.error(format!(
                    "variant name `{name}` is ambiguous; qualify it with its enum"
                ));
                None
            }
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

fn flatten_call<'a>(expression: &'a Expr, groups: &mut Vec<&'a [CallArg]>) -> &'a Expr {
    match expression {
        Expr::Call(callee, arguments) => {
            let root = flatten_call(callee, groups);
            groups.push(arguments);
            root
        }
        _ => expression,
    }
}

fn place_root_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(name) => Some(name),
        Expr::Member(base, _) => place_root_name(base),
        _ => None,
    }
}

fn record_closure_capture(
    captures: &mut Vec<ClosureCaptureUse>,
    name: &str,
    mode: ClosureCaptureMode,
) {
    if let Some(capture) = captures.iter_mut().find(|capture| capture.name == name) {
        if mode == ClosureCaptureMode::Mutable {
            capture.mode = mode;
        }
    } else {
        captures.push(ClosureCaptureUse {
            name: name.to_owned(),
            mode,
        });
    }
}

fn is_copy_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 | Ty::Bool | Ty::Unit | Ty::Never | Ty::Error
    )
}

fn effective_pass_mode(mode: PassMode, ty: &Ty) -> PassMode {
    match mode {
        PassMode::Inferred if is_copy_type(ty) => PassMode::Copy,
        PassMode::Inferred => PassMode::Move,
        mode => mode,
    }
}

fn is_place_prefix(prefix: &PlaceKey, place: &PlaceKey) -> bool {
    prefix.local == place.local
        && prefix.projections.len() <= place.projections.len()
        && place.projections.starts_with(&prefix.projections)
}

fn places_overlap(left: &PlaceKey, right: &PlaceKey) -> bool {
    is_place_prefix(left, right) || is_place_prefix(right, left)
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

fn nominal_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => Some(name),
        _ => None,
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
    Aggregate(Vec<ConstValue>),
}

fn evaluate_globals(program: &HirProgram) -> Result<HashMap<String, ConstValue>, Vec<Diagnostic>> {
    let globals: HashMap<_, _> = program
        .globals
        .iter()
        .map(|global| (global.name.clone(), global))
        .collect();
    let mut evaluator = ConstantEvaluator {
        program,
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
    program: &'a HirProgram,
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
            HirExprKind::Read { place, .. } => {
                let mut value = locals.get(&place.local).cloned().or_else(|| {
                    self.error("invalid local in constant expression");
                    None
                })?;
                for index in &place.projections {
                    let ConstValue::Aggregate(fields) = value else {
                        self.error("invalid field read in constant expression");
                        return None;
                    };
                    let Some(field) = fields.get(*index).cloned() else {
                        self.error("invalid field index in constant expression");
                        return None;
                    };
                    value = field;
                }
                Some(value)
            }
            HirExprKind::Global(name) => self.evaluate_global(name),
            HirExprKind::ConstructStruct { name, fields } => {
                let layout = self.program.struct_layout(name)?;
                let mut values = layout
                    .fields
                    .iter()
                    .map(|field| zero_const(&field.ty, self.program))
                    .collect::<Option<Vec<_>>>()?;
                for (index, field) in fields {
                    values[*index] = self.evaluate_expr(field, locals)?;
                }
                Some(ConstValue::Aggregate(values))
            }
            HirExprKind::ConstructEnum {
                name,
                variant,
                fields,
            } => {
                let layout = self.program.enum_layout(name)?;
                let variant_layout = &layout.variants[*variant];
                let mut values = vec![ConstValue::Integer(*variant as i128)];
                values.extend(
                    layout
                        .variants
                        .iter()
                        .flat_map(|variant| &variant.fields)
                        .map(|field| zero_const(&field.ty, self.program))
                        .collect::<Option<Vec<_>>>()?,
                );
                for (index, field) in fields {
                    values[1 + variant_layout.payload_offset + index] =
                        self.evaluate_expr(field, locals)?;
                }
                Some(ConstValue::Aggregate(values))
            }
            HirExprKind::Field { base, index } => {
                let ConstValue::Aggregate(fields) = self.evaluate_expr(base, locals)? else {
                    return None;
                };
                fields.get(*index).cloned()
            }
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
            | HirExprKind::Borrow { .. }
            | HirExprKind::Call { .. }
            | HirExprKind::Partial { .. }
            | HirExprKind::PartialCapture { .. }
            | HirExprKind::LocalClosure(_)
            | HirExprKind::Function(_)
            | HirExprKind::Return(_)
            | HirExprKind::Match { .. } => {
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

        for layout in &self.program.structs {
            let fields = layout
                .fields
                .iter()
                .map(|field| llvm_field_type(&field.ty))
                .collect::<Result<Vec<_>, _>>()?;
            output.push_str(&format!(
                "%{} = type {{ {} }}\n",
                type_symbol(&layout.name),
                fields.join(", ")
            ));
        }
        for layout in &self.program.enums {
            let mut fields = vec!["i32".to_owned()];
            for field in layout.variants.iter().flat_map(|variant| &variant.fields) {
                fields.push(llvm_field_type(&field.ty)?);
            }
            output.push_str(&format!(
                "%{} = type {{ {} }}\n",
                type_symbol(&layout.name),
                fields.join(", ")
            ));
        }
        if !self.program.structs.is_empty() || !self.program.enums.is_empty() {
            output.push('\n');
        }

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
                const_ir(value, &global.ty, self.program)?
            ));
        }
        if !self.program.globals.is_empty() {
            output.push('\n');
        }

        for function in &self.program.functions {
            let mut emitter = FunctionEmitter::new(function, self.program);
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
    program: &'a HirProgram,
    output: String,
    next_register: usize,
    next_label: usize,
    locals: HashMap<LocalId, String>,
    partial_captures: HashMap<LocalId, Vec<Option<(Ty, String)>>>,
    current_label: String,
    terminated: bool,
}

impl<'a> FunctionEmitter<'a> {
    fn new(function: &'a HirFunction, program: &'a HirProgram) -> Self {
        Self {
            function,
            program,
            output: String::new(),
            next_register: 0,
            next_label: 0,
            locals: HashMap::new(),
            partial_captures: HashMap::new(),
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
            let abi_ty = if matches!(parameter.mode, PassMode::Borrow | PassMode::MutBorrow) {
                "ptr".to_owned()
            } else {
                llvm_value_type(&parameter.ty)?
            };
            self.output.push_str(&format!("{abi_ty} %arg.{index}"));
            emitted_parameter_count += 1;
        }
        self.output.push_str(") {\nentry:\n");

        for (index, parameter) in self.function.params.iter().enumerate() {
            if parameter.ty == Ty::Unit {
                continue;
            }
            if matches!(parameter.mode, PassMode::Borrow | PassMode::MutBorrow) {
                self.locals.insert(parameter.id, format!("%arg.{index}"));
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
            HirExprKind::Read { place, kind } => {
                let _ = kind;
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let pointer = self.emit_place_address(place)?;
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
                "function value `{name}` reached LLVM emission"
            ))),
            HirExprKind::ConstructStruct { name, fields } => {
                let aggregate_ty = llvm_value_type(&Ty::Struct(name.clone()))?;
                let mut aggregate = "zeroinitializer".to_owned();
                for (index, field) in fields {
                    let field = self.emit_expr(field)?;
                    if self.terminated {
                        return Ok(Operand::never());
                    }
                    if field.ty == Ty::Unit {
                        continue;
                    }
                    let register = self.fresh_register();
                    self.instruction(format!(
                        "{register} = insertvalue {aggregate_ty} {aggregate}, {} {}, {index}",
                        llvm_value_type(&field.ty)?,
                        field.value()?
                    ));
                    aggregate = register;
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(aggregate),
                })
            }
            HirExprKind::ConstructEnum {
                name,
                variant,
                fields,
            } => {
                let layout = self.program.enum_layout(name).ok_or_else(|| {
                    Diagnostic::new(format!("internal error: missing enum layout `{name}`"))
                })?;
                let variant_layout = &layout.variants[*variant];
                let aggregate_ty = llvm_value_type(&Ty::Enum(name.clone()))?;
                let tag_register = self.fresh_register();
                self.instruction(format!(
                    "{tag_register} = insertvalue {aggregate_ty} zeroinitializer, i32 {variant}, 0"
                ));
                let mut aggregate = tag_register;
                for (index, field) in fields {
                    let field = self.emit_expr(field)?;
                    if self.terminated {
                        return Ok(Operand::never());
                    }
                    if field.ty == Ty::Unit {
                        continue;
                    }
                    let register = self.fresh_register();
                    let payload_index = 1 + variant_layout.payload_offset + index;
                    self.instruction(format!(
                        "{register} = insertvalue {aggregate_ty} {aggregate}, {} {}, {payload_index}",
                        llvm_value_type(&field.ty)?,
                        field.value()?
                    ));
                    aggregate = register;
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(aggregate),
                })
            }
            HirExprKind::Field { base, index } => {
                let base = self.emit_expr(base)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if expression.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let register = self.fresh_register();
                self.instruction(format!(
                    "{register} = extractvalue {} {}, {index}",
                    llvm_value_type(&base.ty)?,
                    base.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
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
            HirExprKind::Assign(place, value) => {
                let value = self.emit_expr(value)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                if value.ty == Ty::Unit {
                    return Ok(Operand::unit());
                }
                let ty = llvm_value_type(&value.ty)?;
                let pointer = self.emit_place_address(place)?;
                self.instruction(format!("store {ty} {}, ptr {pointer}", value.value()?));
                Ok(Operand::unit())
            }
            HirExprKind::Call {
                function,
                arguments,
            } => {
                let mut emitted_arguments = Vec::new();
                for argument in arguments {
                    match argument {
                        HirArgument::Copy(argument) | HirArgument::Move(argument) => {
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
                        HirArgument::SharedBorrow(place) | HirArgument::MutBorrow(place) => {
                            if place.ty == Ty::Unit {
                                continue;
                            }
                            let pointer = self.emit_place_address(place)?;
                            emitted_arguments.push(format!("ptr {pointer}"));
                        }
                    }
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
            HirExprKind::Partial { .. } => Err(Diagnostic::new(
                "local partial application escaped its binding",
            )),
            HirExprKind::Borrow { .. } => Err(Diagnostic::new(
                "borrow value escaped the local binding that owns its loan",
            )),
            HirExprKind::PartialCapture { binding, index } => {
                let capture = self
                    .partial_captures
                    .get(binding)
                    .and_then(|captures| captures.get(*index))
                    .cloned()
                    .ok_or_else(|| {
                        Diagnostic::new(format!(
                            "internal error: unknown capture {index} for partial binding {binding}"
                        ))
                    })?;
                let Some((ty, pointer)) = capture else {
                    return Ok(Operand::unit());
                };
                let register = self.fresh_register();
                self.instruction(format!(
                    "{register} = load {}, ptr {pointer}",
                    llvm_value_type(&ty)?
                ));
                Ok(Operand {
                    ty,
                    value: Some(register),
                })
            }
            HirExprKind::LocalClosure(closure) => Err(Diagnostic::new(format!(
                "local closure `{}` escaped its binding",
                closure.function
            ))),
            HirExprKind::Block(statements, tail) => {
                for statement in statements {
                    if self.terminated {
                        break;
                    }
                    match statement {
                        HirStmt::Let(binding) => {
                            if matches!(binding.value.kind, HirExprKind::LocalClosure(_)) {
                                continue;
                            }
                            if let HirExprKind::Partial { captures, .. } = &binding.value.kind {
                                let mut stored = Vec::new();
                                for capture in captures {
                                    let capture = match capture {
                                        HirArgument::Copy(capture) | HirArgument::Move(capture) => {
                                            self.emit_expr(capture)?
                                        }
                                        HirArgument::SharedBorrow(_)
                                        | HirArgument::MutBorrow(_) => {
                                            return Err(Diagnostic::new(
                                                "borrowed argument reached partial application emission",
                                            ));
                                        }
                                    };
                                    if self.terminated {
                                        break;
                                    }
                                    if capture.ty == Ty::Unit {
                                        stored.push(None);
                                        continue;
                                    }
                                    let pointer = self.fresh_register();
                                    let ty = llvm_value_type(&capture.ty)?;
                                    self.instruction(format!(
                                        "{pointer} = alloca {ty} ; partial capture"
                                    ));
                                    self.instruction(format!(
                                        "store {ty} {}, ptr {pointer}",
                                        capture.value()?
                                    ));
                                    stored.push(Some((capture.ty, pointer)));
                                }
                                if !self.terminated {
                                    self.partial_captures.insert(binding.id, stored);
                                }
                                continue;
                            }
                            if matches!(binding.value.kind, HirExprKind::Borrow { .. }) {
                                continue;
                            }
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
            HirExprKind::Match { scrutinee, arms } => self.emit_match(expression, scrutinee, arms),
        }
    }

    fn emit_match(
        &mut self,
        expression: &HirExpr,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
    ) -> Result<Operand, Diagnostic> {
        let scrutinee = self.emit_expr(scrutinee)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let Ty::Enum(enum_name) = &scrutinee.ty else {
            return Err(Diagnostic::new(
                "internal error: non-enum scrutinee reached match emission",
            ));
        };
        let layout = self.program.enum_layout(enum_name).ok_or_else(|| {
            Diagnostic::new(format!("internal error: missing enum layout `{enum_name}`"))
        })?;
        let tag = self.fresh_register();
        self.instruction(format!(
            "{tag} = extractvalue {} {}, 0",
            llvm_value_type(&scrutinee.ty)?,
            scrutinee.value()?
        ));

        let mut candidates = Vec::new();
        let mut labels = Vec::new();
        for variant in 0..layout.variants.len() {
            let mut variant_candidates = Vec::new();
            for (arm_index, arm) in arms.iter().enumerate() {
                if matches!(arm.matcher, HirMatcher::All)
                    || arm.matcher == HirMatcher::Variant(variant)
                {
                    variant_candidates.push(arm_index);
                    if arm.guard.is_none() {
                        break;
                    }
                }
            }
            let variant_labels: Vec<_> = (0..variant_candidates.len())
                .map(|_| self.fresh_label("match.candidate"))
                .collect();
            candidates.push(variant_candidates);
            labels.push(variant_labels);
        }
        let default_label = self.fresh_label("match.invalid");
        let merge_label = self.fresh_label("match.end");
        let cases = labels
            .iter()
            .enumerate()
            .filter_map(|(variant, labels)| {
                labels
                    .first()
                    .map(|label| format!("i32 {variant}, label %{label}"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        self.terminate(format!(
            "switch i32 {tag}, label %{default_label} [ {cases} ]"
        ));

        self.start_block(&default_label);
        self.terminate("unreachable");

        let mut incoming = Vec::new();
        for variant in 0..layout.variants.len() {
            for (position, arm_index) in candidates[variant].iter().copied().enumerate() {
                self.start_block(&labels[variant][position]);
                let arm = &arms[arm_index];
                self.emit_pattern_bindings(&scrutinee, &arm.bindings)?;

                if let Some(guard) = &arm.guard {
                    let guard = self.emit_expr(guard)?;
                    if !self.terminated {
                        let body_label = self.fresh_label("match.body");
                        let false_label = labels[variant]
                            .get(position + 1)
                            .cloned()
                            .unwrap_or_else(|| default_label.clone());
                        self.terminate(format!(
                            "br i1 {}, label %{body_label}, label %{false_label}",
                            guard.value()?
                        ));
                        self.start_block(&body_label);
                    }
                }

                let body = self.emit_expr(&arm.body)?;
                if !self.terminated {
                    let predecessor = self.current_label.clone();
                    self.terminate(format!("br label %{merge_label}"));
                    incoming.push((body, predecessor));
                }
            }
        }

        if incoming.is_empty() {
            self.terminated = true;
            return Ok(Operand::never());
        }
        self.start_block(&merge_label);
        if expression.ty == Ty::Unit {
            return Ok(Operand::unit());
        }
        if incoming.len() == 1 {
            return Ok(incoming.pop().expect("one incoming match value").0);
        }
        let register = self.fresh_register();
        let incoming = incoming
            .iter()
            .map(|(operand, label)| Ok(format!("[{}, %{label}]", operand.value()?)))
            .collect::<Result<Vec<_>, Diagnostic>>()?
            .join(", ");
        self.instruction(format!(
            "{register} = phi {} {incoming}",
            llvm_value_type(&expression.ty)?
        ));
        Ok(Operand {
            ty: expression.ty.clone(),
            value: Some(register),
        })
    }

    fn emit_pattern_bindings(
        &mut self,
        scrutinee: &Operand,
        bindings: &[HirPatternBinding],
    ) -> Result<(), Diagnostic> {
        for binding in bindings {
            if binding.ty == Ty::Unit {
                continue;
            }
            let value = if binding.path.is_empty() {
                scrutinee.value()?.to_owned()
            } else {
                let register = self.fresh_register();
                let path = binding
                    .path
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                self.instruction(format!(
                    "{register} = extractvalue {} {}, {path}",
                    llvm_value_type(&scrutinee.ty)?,
                    scrutinee.value()?
                ));
                register
            };
            let pointer = self.fresh_register();
            let ty = llvm_value_type(&binding.ty)?;
            self.instruction(format!(
                "{pointer} = alloca {ty} ; {}",
                llvm_comment(&binding.name)
            ));
            self.instruction(format!("store {ty} {value}, ptr {pointer}"));
            self.locals.insert(binding.id, pointer);
        }
        Ok(())
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

    fn emit_place_address(&mut self, place: &HirPlace) -> Result<String, Diagnostic> {
        let root_pointer = self.locals.get(&place.local).cloned().ok_or_else(|| {
            Diagnostic::new(format!("internal error: unknown local id {}", place.local))
        })?;
        if place.projections.is_empty() {
            return Ok(root_pointer);
        }
        let pointer = self.fresh_register();
        let indices = place
            .projections
            .iter()
            .map(|index| format!("i32 {index}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.instruction(format!(
            "{pointer} = getelementptr inbounds {}, ptr {root_pointer}, i32 0, {indices}",
            llvm_value_type(&place.root_ty)?
        ));
        Ok(pointer)
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

fn llvm_return_type(ty: &Ty) -> Result<String, Diagnostic> {
    if *ty == Ty::Unit {
        Ok("void".to_owned())
    } else {
        llvm_value_type(ty)
    }
}

fn llvm_value_type(ty: &Ty) -> Result<String, Diagnostic> {
    match ty {
        Ty::I32 | Ty::U32 => Ok("i32".to_owned()),
        Ty::I64 | Ty::U64 => Ok("i64".to_owned()),
        Ty::Bool => Ok("i1".to_owned()),
        Ty::Struct(name) | Ty::Enum(name) => Ok(format!("%{}", type_symbol(name))),
        Ty::Unit | Ty::Never | Ty::Function(_) | Ty::Error => Err(Diagnostic::new(format!(
            "internal error: `{ty}` has no first-class LLVM representation"
        ))),
    }
}

fn llvm_field_type(ty: &Ty) -> Result<String, Diagnostic> {
    if *ty == Ty::Unit {
        Ok("[0 x i8]".to_owned())
    } else {
        llvm_value_type(ty)
    }
}

fn zero_const(ty: &Ty, program: &HirProgram) -> Option<ConstValue> {
    match ty {
        Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 => Some(ConstValue::Integer(0)),
        Ty::Bool => Some(ConstValue::Bool(false)),
        Ty::Unit => Some(ConstValue::Unit),
        Ty::Struct(name) => Some(ConstValue::Aggregate(
            program
                .struct_layout(name)?
                .fields
                .iter()
                .map(|field| zero_const(&field.ty, program))
                .collect::<Option<Vec<_>>>()?,
        )),
        Ty::Enum(name) => {
            let layout = program.enum_layout(name)?;
            let mut fields = vec![ConstValue::Integer(0)];
            fields.extend(
                layout
                    .variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .map(|field| zero_const(&field.ty, program))
                    .collect::<Option<Vec<_>>>()?,
            );
            Some(ConstValue::Aggregate(fields))
        }
        Ty::Never | Ty::Function(_) | Ty::Error => None,
    }
}

fn const_ir(value: &ConstValue, ty: &Ty, program: &HirProgram) -> Result<String, Diagnostic> {
    match (value, ty) {
        (ConstValue::Integer(value), ty) if ty.is_integer() => Ok(value.to_string()),
        (ConstValue::Bool(value), Ty::Bool) => Ok(if *value { "1" } else { "0" }.to_owned()),
        (ConstValue::Unit, Ty::Unit) => Ok("zeroinitializer".to_owned()),
        (ConstValue::Aggregate(values), Ty::Struct(name)) => {
            let layout = program.struct_layout(name).ok_or_else(|| {
                Diagnostic::new(format!("internal error: missing struct layout `{name}`"))
            })?;
            let fields = values
                .iter()
                .zip(&layout.fields)
                .map(|(value, field)| {
                    Ok(format!(
                        "{} {}",
                        llvm_field_type(&field.ty)?,
                        const_ir(value, &field.ty, program)?
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            Ok(format!("{{ {} }}", fields.join(", ")))
        }
        (ConstValue::Aggregate(values), Ty::Enum(name)) => {
            let layout = program.enum_layout(name).ok_or_else(|| {
                Diagnostic::new(format!("internal error: missing enum layout `{name}`"))
            })?;
            let mut types = vec![Ty::U32];
            types.extend(
                layout
                    .variants
                    .iter()
                    .flat_map(|variant| &variant.fields)
                    .map(|field| field.ty.clone()),
            );
            let fields = values
                .iter()
                .zip(types)
                .map(|(value, ty)| {
                    Ok(format!(
                        "{} {}",
                        llvm_field_type(&ty)?,
                        const_ir(value, &ty, program)?
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            Ok(format!("{{ {} }}", fields.join(", ")))
        }
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

fn type_symbol(name: &str) -> String {
    format!("sali.type.{}", hex_name(name))
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

    fn compile_text(source: &str) -> Result<String, Vec<Diagnostic>> {
        let program = crate::parser::parse(source).expect("test source must parse");
        compile(&program)
    }

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

    fn arg(value: Expr) -> CallArg {
        CallArg { label: None, value }
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
                vec![arg(Expr::Integer(20))],
            )),
            vec![arg(Expr::Integer(22))],
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
                            Box::new(Expr::Name("x".into())),
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
    fn emits_nominal_aggregates_and_tag_switches() {
        let ir = compile_text(
            r#"
let Pair = struct(left: i32, right: i32)
let Choice = enum {
  Pair(Pair),
  Empty,
}
let global: Pair = Pair(left: 40, right: 2)
let read(choice: Choice): i32 = choice match {
  Choice.Pair(pair) => pair.left + pair.right,
  Choice.Empty => 0,
}
let main(): i32 = read(Choice.Pair(global))
"#,
        )
        .unwrap();
        assert!(ir.contains("%sali.type.50616972 = type { i32, i32 }"));
        assert!(ir.contains("%sali.type.43686f696365 = type { i32, %sali.type.50616972 }"));
        assert!(ir.contains("switch i32"));
        assert!(ir.contains("@sali.global.676c6f62616c = internal unnamed_addr constant"));
    }

    #[test]
    fn rejects_recursive_value_layouts() {
        let errors = compile_text(
            r#"
let First = struct(next: Second)
let Second = enum {
  Again(First),
  End,
}
let main(): i32 = 0
"#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| {
            error.message.contains("recursive value layout")
                && error.message.contains("First -> Second -> First")
        }));
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
    fn tracks_explicit_move_even_for_a_copy_type() {
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
        let main = function(
            "main",
            vec![vec![]],
            Type::I32,
            Expr::Block(
                vec![
                    Stmt::Let(Binding {
                        mutable: false,
                        name: "value".into(),
                        annotation: None,
                        value: Expr::Integer(7),
                    }),
                    Stmt::Let(Binding {
                        mutable: false,
                        name: "consumed".into(),
                        annotation: None,
                        value: Expr::Call(
                            Box::new(Expr::Name("consume".into())),
                            vec![arg(Expr::Name("value".into()))],
                        ),
                    }),
                ],
                Some(Box::new(Expr::Name("value".into()))),
            ),
        );
        let errors = compile(&Program {
            items: vec![consume, main],
        })
        .unwrap_err();
        assert!(errors.iter().any(|error| error.message.contains("moved")));
    }

    #[test]
    fn reports_a_value_moved_on_only_one_if_path_as_possibly_moved() {
        let errors = compile_text(
            r#"
let Boxed = struct(value: i32)
let consume(move boxed: Boxed): i32 = boxed.value
let choose(flag: bool): i32 = {
  let boxed = Boxed(value: 42)
  if flag {
    consume(boxed)
  }
  boxed.value
}
let main(): i32 = choose(false)
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("possibly moved")));
    }

    #[test]
    fn discards_moves_on_an_if_path_that_returns() {
        let ir = compile_text(
            r#"
let Boxed = struct(value: i32)
let consume(move boxed: Boxed): i32 = boxed.value
let choose(flag: bool): i32 = {
  let boxed = Boxed(value: 42)
  if flag {
    consume(boxed)
    return 0
  }
  boxed.value
}
let main(): i32 = choose(false)
"#,
        )
        .unwrap();
        assert!(ir.contains("call i32 @sali.fn.63686f6f7365(i1 0)"));
    }

    #[test]
    fn reports_a_value_moved_on_both_if_paths_as_moved() {
        let errors = compile_text(
            r#"
let Boxed = struct(value: i32)
let consume(move boxed: Boxed): i32 = boxed.value
let choose(flag: bool): i32 = {
  let boxed = Boxed(value: 42)
  if flag {
    consume(boxed)
  } else {
    consume(boxed)
  }
  boxed.value
}
let main(): i32 = choose(false)
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message == "use of moved value"));
        assert!(!errors
            .iter()
            .any(|error| error.message.contains("possibly moved")));
    }

    #[test]
    fn reports_a_move_on_a_short_circuit_rhs_as_possible() {
        let errors = compile_text(
            r#"
let Boxed = struct(value: i32)
let consume(move boxed: Boxed): bool = boxed.value == 42
let choose(flag: bool): i32 = {
  let boxed = Boxed(value: 42)
  flag && consume(boxed)
  boxed.value
}
let main(): i32 = choose(false)
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("possibly moved")));
    }

    #[test]
    fn analyzes_mutually_exclusive_if_arms_from_the_same_entry_flow() {
        let ir = compile_text(
            r#"
let Boxed = struct(value: i32)
let consume(move boxed: Boxed): i32 = boxed.value
let choose(flag: bool): i32 = {
  let boxed = Boxed(value: 42)
  if flag {
    consume(boxed)
  } else {
    boxed.value
  }
}
let main(): i32 = choose(true)
"#,
        )
        .unwrap();
        assert!(ir.contains("call i32 @sali.fn.63686f6f7365(i1 1)"));
    }

    #[test]
    fn analyzes_mutually_exclusive_match_arms_from_variant_entry_flows() {
        compile_text(
            r#"
let Choice = enum {
  First,
  Second,
}
let Boxed = struct(value: i32)
let consume(move boxed: Boxed): i32 = boxed.value
let choose(choice: Choice): i32 = {
  let boxed = Boxed(value: 42)
  choice match {
    Choice.First => consume(boxed),
    Choice.Second => boxed.value,
  }
}
let main(): i32 = choose(Choice.First)
"#,
        )
        .unwrap();
    }

    #[test]
    fn carries_guard_moves_into_later_match_candidates() {
        let errors = compile_text(
            r#"
let Choice = enum {
  Only,
}
let Boxed = struct(value: i32)
let consume(move boxed: Boxed): bool = boxed.value == 0
let choose(choice: Choice): i32 = {
  let boxed = Boxed(value: 42)
  choice match {
    Choice.Only if consume(boxed) => 0,
    Choice.Only => boxed.value,
  }
}
let main(): i32 = choose(Choice.Only)
"#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| error.message.contains("moved")));
    }

    #[test]
    fn lifts_a_local_closure_with_a_shared_scalar_capture() {
        let ir = compile_text(
            r#"
let main(): i32 = {
  let base = 40
  let add_base = { (increment: i32) -> base + increment }
  add_base(2)
}
"#,
        )
        .unwrap();
        let symbol = function_symbol("__closure.0");
        assert!(ir.contains(&format!(
            "define internal i32 @{symbol}(ptr %arg.0, i32 %arg.1)"
        )));
        assert!(ir.contains(&format!("call i32 @{symbol}(ptr")));
    }

    #[test]
    fn rejects_aliasing_a_local_closure() {
        let errors = compile_text(
            r#"
let main(): i32 = {
  let base = 40
  let add_base = { (increment: i32) -> base + increment }
  let alias = add_base
  0
}
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("cannot escape")));
    }

    #[test]
    fn lifts_and_repeatedly_calls_an_fn_mut_scalar_closure() {
        let ir = compile_text(
            r#"
let main(): i32 = {
  let mut value = 40
  let mut next = { ->
    value = value + 1
    value
  }
  next()
  next()
}
"#,
        )
        .unwrap();
        let symbol = function_symbol("__closure.0");
        assert!(ir.contains(&format!("define internal i32 @{symbol}(ptr %arg.0)")));
        assert_eq!(ir.matches(&format!("call i32 @{symbol}(ptr")).count(), 2);
        assert!(ir.contains("store i32"));
    }

    #[test]
    fn requires_a_mutable_binding_for_an_fn_mut_closure() {
        let errors = compile_text(
            r#"
let main(): i32 = {
  let mut value = 40
  let next = { ->
    value = value + 2
    value
  }
  next()
}
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| { error.message.contains("FnMut") && error.message.contains("mutable") }));
    }

    #[test]
    fn keeps_an_fn_mut_capture_mutably_borrowed_for_its_scope() {
        let errors = compile_text(
            r#"
let main(): i32 = {
  let mut value = 40
  let mut next = { ->
    value = value + 2
    value
  }
  value
}
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("borrowed")));
    }

    #[test]
    fn rejects_an_fn_mut_capture_that_conflicts_with_an_existing_borrow() {
        let errors = compile_text(
            r#"
let main(): i32 = {
  let mut value = 40
  let shared = borrow value
  let mut next = { ->
    value = value + 2
    value
  }
  0
}
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("borrowed")));
    }

    #[test]
    fn emits_local_non_escaping_partial_application() {
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
            Expr::Block(
                vec![Stmt::Let(Binding {
                    mutable: false,
                    name: "add_one".into(),
                    annotation: None,
                    value: Expr::Call(
                        Box::new(Expr::Name("add".into())),
                        vec![arg(Expr::Integer(1))],
                    ),
                })],
                Some(Box::new(Expr::Call(
                    Box::new(Expr::Name("add_one".into())),
                    vec![arg(Expr::Integer(41))],
                ))),
            ),
        );
        let ir = compile(&Program {
            items: vec![add, main],
        })
        .unwrap();
        assert!(ir.contains("call i32 @sali.fn.616464(i32"));
    }
}
