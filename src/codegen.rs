//! Type checking and textual LLVM IR generation for Salicin's M0 subset.
//!
//! The backend intentionally consumes the parser AST directly but first lowers
//! it to a small typed representation.  No malformed program reaches the LLVM
//! emitter, which keeps the generated IR simple enough to inspect in tests.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::ast::{
    BinaryOp, Binding, CallArg, CompileParam, CompileParamKind, EnumDef, Expr, ExtendDef,
    ExtendMember, Function, Item, MatchArm, PassMode, Pattern, PatternFields, Program, Stmt,
    StructDef, TraitDef, TraitMember, Type, UnaryOp, VariantDef, VariantFields,
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

fn builtin_prelude_items() -> [Item; 2] {
    let type_parameter = |name: &str| CompileParam {
        name: name.to_owned(),
        kind: CompileParamKind::Type,
    };
    let named_type = |name: &str| Type::Named(name.to_owned(), Vec::new());

    [
        Item::Enum(EnumDef {
            name: "Option".to_owned(),
            compile_groups: vec![vec![type_parameter("T")]],
            variants: vec![
                VariantDef {
                    name: "Some".to_owned(),
                    fields: VariantFields::Positional(vec![named_type("T")]),
                },
                VariantDef {
                    name: "None".to_owned(),
                    fields: VariantFields::Unit,
                },
            ],
        }),
        Item::Enum(EnumDef {
            name: "Result".to_owned(),
            compile_groups: vec![vec![type_parameter("T"), type_parameter("E")]],
            variants: vec![
                VariantDef {
                    name: "Ok".to_owned(),
                    fields: VariantFields::Positional(vec![named_type("T")]),
                },
                VariantDef {
                    name: "Err".to_owned(),
                    fields: VariantFields::Positional(vec![named_type("E")]),
                },
            ],
        }),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Ty {
    I32,
    I64,
    U32,
    U64,
    Bool,
    Unit,
    Array(Box<Ty>, u64),
    Struct(String),
    Enum(String),
    Never,
    Function(FunctionTy),
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FunctionInstanceKey {
    template: String,
    arguments: Vec<Ty>,
}

#[derive(Debug, Clone)]
struct FunctionInstanceInfo {
    key: FunctionInstanceKey,
    canonical: String,
}

const MAX_FUNCTION_INSTANCES: usize = 256;
const MAX_NOMINAL_INSTANCES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NominalKind {
    Struct,
    Enum,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NominalInstanceKey {
    kind: NominalKind,
    template: String,
    arguments: Vec<Ty>,
}

#[derive(Debug, Clone)]
struct NominalInstanceInfo {
    key: NominalInstanceKey,
    canonical: String,
}

#[derive(Debug, Clone)]
struct InferredTypeArgument {
    ty: Ty,
    source: Option<Type>,
    origin: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeProbe {
    Known(Ty),
    KnownSource(Ty, Type),
    Defaultable(Ty),
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinFallibleKind {
    Option,
    Result,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BuiltinFallibleInfo {
    kind: BuiltinFallibleKind,
    payload: Ty,
    payload_source: Option<Type>,
    error: Option<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReturnBoundary {
    kind: BuiltinFallibleKind,
    container: Ty,
    success: Ty,
    error: Option<Ty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnValueCandidate {
    Container,
    Success,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CoalescePayloadHint {
    ty: Ty,
    source: Option<Type>,
}

struct InferredCoalesceLhs<'a> {
    kind: BuiltinFallibleKind,
    name: String,
    type_groups: Vec<&'a [CallArg]>,
    variant: &'a str,
    value_groups: Vec<&'a [CallArg]>,
}

#[derive(Clone, Copy)]
struct InferredEnumHints<'a> {
    payload: Option<&'a CoalescePayloadHint>,
    result: Option<&'a Ty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NominalInstanceState {
    Building,
    Ready,
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
            Self::Array(element, length) => write!(f, "Array({element}, {length})"),
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
    Array(Vec<HirExpr>),
    Index {
        base: Box<HirExpr>,
        index: HirIndex,
        length: u64,
    },
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
    While {
        condition: Box<HirExpr>,
        body: Box<HirExpr>,
    },
    Loop {
        body: Box<HirExpr>,
    },
    Break(Option<Box<HirExpr>>),
    Match {
        scrutinee: Box<HirExpr>,
        arms: Vec<HirMatchArm>,
    },
}

#[derive(Debug, Clone)]
enum HirIndex {
    Static(u64),
    Dynamic(Box<HirExpr>),
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
    groups: Vec<Vec<ParamSig>>,
    result: Ty,
    captures: Vec<ClosureCapture>,
    is_fn_mut: bool,
    is_fn_once: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClosureCaptureMode {
    Shared,
    Mutable,
    Move,
}

#[derive(Debug, Clone)]
struct ClosureCapture {
    place: HirPlace,
    mode: ClosureCaptureMode,
    value: Option<Box<HirExpr>>,
}

#[derive(Debug, Clone)]
struct ClosureCaptureUse {
    name: String,
    mode: ClosureCaptureMode,
}

struct LoopFrame {
    result_ty: Option<Ty>,
    unit_only: bool,
    scope_depth: usize,
    break_flows: Vec<FlowState>,
}

struct LowerCtx {
    scopes: Vec<ScopeFrame>,
    flow: FlowState,
    next_local: LocalId,
    next_loan: LoanId,
    declared_result: Option<Ty>,
    return_boundary: Option<ReturnBoundary>,
    returned_types: Vec<Ty>,
    function_name: Option<String>,
    type_substitutions: HashMap<String, Type>,
    loops: Vec<LoopFrame>,
}

impl LowerCtx {
    fn for_function(name: &str, result: Option<Ty>) -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            flow: FlowState::default(),
            next_local: 0,
            next_loan: 0,
            declared_result: result,
            return_boundary: None,
            returned_types: Vec::new(),
            function_name: Some(name.to_owned()),
            type_substitutions: HashMap::new(),
            loops: Vec::new(),
        }
    }

    fn for_global() -> Self {
        Self {
            scopes: vec![ScopeFrame::new()],
            flow: FlowState::default(),
            next_local: 0,
            next_loan: 0,
            declared_result: None,
            return_boundary: None,
            returned_types: Vec::new(),
            function_name: None,
            type_substitutions: HashMap::new(),
            loops: Vec::new(),
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

    fn flow_without_scopes_from(&self, scope_depth: usize, mut flow: FlowState) -> FlowState {
        for scope in &self.scopes[scope_depth..] {
            for loan in &scope.lexical_loans {
                flow.loans.remove(loan);
            }
            flow.moves
                .retain(|place, _| !scope.locals.contains(&place.local));
        }
        flow
    }

    fn outer_local_ids(&self) -> HashSet<LocalId> {
        self.scopes
            .iter()
            .flat_map(|scope| scope.locals.iter().copied())
            .collect()
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

#[derive(Default)]
struct InherentMemberSet {
    methods: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitRefKey {
    name: String,
    arguments: Vec<Ty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TraitImplKey {
    self_ty: Ty,
    trait_ref: TraitRefKey,
}

#[derive(Debug, Clone)]
struct TraitImplInfo {
    key: TraitImplKey,
    associated_types: HashMap<String, Ty>,
    methods: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct AddCandidate {
    method: String,
    rhs: Ty,
    output: Ty,
}

#[derive(Debug, Clone)]
struct TraitSchema {
    compile_parameters: Vec<CompileParam>,
    associated_types: Vec<String>,
    methods: HashMap<String, Function>,
    method_order: Vec<String>,
    valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FunctionShape {
    groups: Vec<Vec<(PassMode, Ty)>>,
    result: Ty,
}

#[derive(Clone)]
struct NominalSnapshot {
    struct_defs: HashMap<String, StructDef>,
    enum_defs: HashMap<String, EnumDef>,
    struct_layouts: HashMap<String, StructLayout>,
    enum_layouts: HashMap<String, EnumLayout>,
    struct_order: Vec<String>,
    enum_order: Vec<String>,
    instance_names: HashMap<NominalInstanceKey, String>,
    instances: HashMap<String, NominalInstanceInfo>,
    states: HashMap<NominalInstanceKey, NominalInstanceState>,
    invalid_recursive_nominals: HashSet<String>,
}

struct Analyzer {
    functions: HashMap<String, Function>,
    function_templates: HashMap<String, Function>,
    function_template_order: Vec<String>,
    function_instance_names: HashMap<FunctionInstanceKey, String>,
    function_instances: HashMap<String, FunctionInstanceInfo>,
    function_type_substitutions: HashMap<String, HashMap<String, Type>>,
    abstract_type_parameters: HashMap<String, String>,
    globals: HashMap<String, Binding>,
    struct_defs: HashMap<String, StructDef>,
    enum_defs: HashMap<String, EnumDef>,
    struct_templates: HashMap<String, StructDef>,
    enum_templates: HashMap<String, EnumDef>,
    struct_template_order: Vec<String>,
    enum_template_order: Vec<String>,
    nominal_instance_names: HashMap<NominalInstanceKey, String>,
    nominal_instances: HashMap<String, NominalInstanceInfo>,
    nominal_instance_states: HashMap<NominalInstanceKey, NominalInstanceState>,
    invalid_recursive_nominals: HashSet<String>,
    struct_layouts: HashMap<String, StructLayout>,
    enum_layouts: HashMap<String, EnumLayout>,
    inherent_members: HashMap<String, InherentMemberSet>,
    traits: HashMap<String, TraitSchema>,
    trait_impl_headers: HashSet<TraitImplKey>,
    trait_impls: HashMap<TraitImplKey, TraitImplInfo>,
    trait_methods_by_receiver: HashMap<(Ty, String), Vec<TraitImplKey>>,
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
            function_templates: HashMap::new(),
            function_template_order: Vec::new(),
            function_instance_names: HashMap::new(),
            function_instances: HashMap::new(),
            function_type_substitutions: HashMap::new(),
            abstract_type_parameters: HashMap::new(),
            globals: HashMap::new(),
            struct_defs: HashMap::new(),
            enum_defs: HashMap::new(),
            struct_templates: HashMap::new(),
            enum_templates: HashMap::new(),
            struct_template_order: Vec::new(),
            enum_template_order: Vec::new(),
            nominal_instance_names: HashMap::new(),
            nominal_instances: HashMap::new(),
            nominal_instance_states: HashMap::new(),
            invalid_recursive_nominals: HashSet::new(),
            struct_layouts: HashMap::new(),
            enum_layouts: HashMap::new(),
            inherent_members: HashMap::new(),
            traits: HashMap::new(),
            trait_impl_headers: HashSet::new(),
            trait_impls: HashMap::new(),
            trait_methods_by_receiver: HashMap::new(),
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
        let mut extensions = Vec::new();
        let prelude = builtin_prelude_items();
        for item in prelude.iter().chain(&program.items) {
            let name = match item {
                Item::Function(function) => &function.name,
                Item::Global(binding) => &binding.name,
                Item::Struct(definition) => &definition.name,
                Item::Enum(definition) => &definition.name,
                Item::Trait(definition) => &definition.name,
                Item::Extend(extension) => {
                    extensions.push(extension.clone());
                    continue;
                }
            };
            if !names.insert(name.clone()) {
                self.error(format!("duplicate top-level name `{name}`"));
                continue;
            }
            match item {
                Item::Function(function) => {
                    if function.compile_groups.is_empty() {
                        self.function_order.push(function.name.clone());
                        self.functions
                            .insert(function.name.clone(), function.clone());
                    } else {
                        self.function_template_order.push(function.name.clone());
                        self.function_templates
                            .insert(function.name.clone(), function.clone());
                    }
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
                    if definition.compile_groups.is_empty() {
                        let key = NominalInstanceKey {
                            kind: NominalKind::Struct,
                            template: definition.name.clone(),
                            arguments: Vec::new(),
                        };
                        self.nominal_instance_names
                            .insert(key.clone(), definition.name.clone());
                        self.nominal_instances.insert(
                            definition.name.clone(),
                            NominalInstanceInfo {
                                key: key.clone(),
                                canonical: definition.name.clone(),
                            },
                        );
                        self.nominal_instance_states
                            .insert(key, NominalInstanceState::Building);
                        self.struct_order.push(definition.name.clone());
                        self.struct_defs
                            .insert(definition.name.clone(), definition.clone());
                    } else {
                        if definition.compile_groups.len() != 1 {
                            self.error(format!(
                                "generic struct `{}` must use exactly one compile-time parameter group",
                                definition.name
                            ));
                            continue;
                        }
                        self.struct_template_order.push(definition.name.clone());
                        self.struct_templates
                            .insert(definition.name.clone(), definition.clone());
                    }
                }
                Item::Enum(definition) => {
                    if definition.compile_groups.is_empty() {
                        let key = NominalInstanceKey {
                            kind: NominalKind::Enum,
                            template: definition.name.clone(),
                            arguments: Vec::new(),
                        };
                        self.nominal_instance_names
                            .insert(key.clone(), definition.name.clone());
                        self.nominal_instances.insert(
                            definition.name.clone(),
                            NominalInstanceInfo {
                                key: key.clone(),
                                canonical: definition.name.clone(),
                            },
                        );
                        self.nominal_instance_states
                            .insert(key, NominalInstanceState::Building);
                        self.enum_order.push(definition.name.clone());
                        self.enum_defs
                            .insert(definition.name.clone(), definition.clone());
                    } else {
                        if definition.compile_groups.len() != 1 {
                            self.error(format!(
                                "generic enum `{}` must use exactly one compile-time parameter group",
                                definition.name
                            ));
                            continue;
                        }
                        self.enum_template_order.push(definition.name.clone());
                        self.enum_templates
                            .insert(definition.name.clone(), definition.clone());
                    }
                }
                Item::Trait(definition) => self.collect_trait_schema(definition.clone()),
                Item::Extend(_) => unreachable!("extensions were collected separately"),
            }
        }

        self.validate_generic_nominal_cycles();
        self.collect_nominal_layouts();
        self.validate_trait_schemas();
        for extension in extensions {
            self.collect_extension(extension);
        }

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

        self.validate_nominal_templates();
        self.validate_function_templates();
    }

    fn add_lang_item_has_required_shape(definition: &TraitDef) -> bool {
        let [compile_group] = definition.compile_groups.as_slice() else {
            return false;
        };
        let [rhs_parameter] = compile_group.as_slice() else {
            return false;
        };
        if rhs_parameter.kind != CompileParamKind::Type || definition.members.len() != 2 {
            return false;
        }

        let rhs_type = Type::Named(rhs_parameter.name.clone(), Vec::new());
        let self_type = Type::Named("Self".to_owned(), Vec::new());
        let output_type = Type::Named("Output".to_owned(), Vec::new());
        let mut found_output = false;
        let mut found_add = false;

        for member in &definition.members {
            match member {
                TraitMember::AssociatedType {
                    name,
                    compile_groups,
                    default,
                } => {
                    if found_output
                        || name != "Output"
                        || !compile_groups.is_empty()
                        || default.is_some()
                    {
                        return false;
                    }
                    found_output = true;
                }
                TraitMember::Function(function) => {
                    if found_add
                        || function.name != "add"
                        || !function.compile_groups.is_empty()
                        || function.body.is_some()
                        || function.return_type.as_ref() != Some(&output_type)
                    {
                        return false;
                    }
                    let [receiver_group, rhs_group] = function.groups.as_slice() else {
                        return false;
                    };
                    let [receiver] = receiver_group.as_slice() else {
                        return false;
                    };
                    let [rhs] = rhs_group.as_slice() else {
                        return false;
                    };
                    if receiver.name != "self"
                        || receiver.mode != PassMode::Move
                        || receiver.ty != self_type
                        || rhs.mode != PassMode::Move
                        || rhs.ty != rhs_type
                    {
                        return false;
                    }
                    found_add = true;
                }
            }
        }

        found_output && found_add
    }

    fn collect_trait_schema(&mut self, definition: TraitDef) {
        let mut valid = true;
        if definition.name == "Add" && !Self::add_lang_item_has_required_shape(&definition) {
            self.error(
                "`Add` language trait must have shape `let Add(Rhs: type) = trait { let Output: type; let add(move self)(move rhs: Rhs): Output }`",
            );
            valid = false;
        }
        if definition.compile_groups.len() > 1 {
            self.error(format!(
                "trait `{}` supports at most one compile-time parameter group",
                definition.name
            ));
            valid = false;
        }
        let compile_parameters = definition
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let mut compile_parameter_names = HashSet::new();
        for parameter in &compile_parameters {
            if parameter.name == "Self" {
                self.error(format!(
                    "trait `{}` cannot declare reserved type parameter `Self`",
                    definition.name
                ));
                valid = false;
            }
            if !compile_parameter_names.insert(parameter.name.clone()) {
                self.error(format!(
                    "duplicate type parameter `{}` in trait `{}`",
                    parameter.name, definition.name
                ));
                valid = false;
            }
        }
        let mut member_names = HashSet::new();
        let mut associated_types = Vec::new();
        let mut methods = HashMap::new();
        let mut method_order = Vec::new();
        for member in definition.members {
            match member {
                TraitMember::AssociatedType {
                    name,
                    compile_groups,
                    default,
                } => {
                    if !member_names.insert(name.clone()) {
                        self.error(format!(
                            "duplicate trait member `{}.{name}`",
                            definition.name
                        ));
                        valid = false;
                        continue;
                    }
                    if name == "Self" || compile_parameter_names.contains(&name) {
                        self.error(format!(
                            "associated type `{}.{name}` conflicts with a trait type parameter",
                            definition.name
                        ));
                        valid = false;
                    }
                    if !compile_groups.is_empty() {
                        self.error(format!(
                            "generic associated type `{}.{name}` is not supported",
                            definition.name
                        ));
                        valid = false;
                    }
                    if default.is_some() {
                        self.error(format!(
                            "default associated type `{}.{name}` is not supported",
                            definition.name
                        ));
                        valid = false;
                    }
                    associated_types.push(name);
                }
                TraitMember::Function(mut function) => {
                    let name = function.name.clone();
                    if !member_names.insert(name.clone()) {
                        self.error(format!(
                            "duplicate trait member `{}.{name}`",
                            definition.name
                        ));
                        valid = false;
                        continue;
                    }
                    if !function.compile_groups.is_empty() {
                        self.error(format!(
                            "generic trait method `{}.{name}` is not supported",
                            definition.name
                        ));
                        valid = false;
                    }
                    if function.body.is_some() {
                        self.error(format!(
                            "default trait method `{}.{name}` is not supported",
                            definition.name
                        ));
                        valid = false;
                        function.body = None;
                    }
                    if function.return_type.is_none() {
                        self.error(format!(
                            "trait method `{}.{name}` requires an explicit return type",
                            definition.name
                        ));
                        valid = false;
                    }
                    let is_method = function
                        .groups
                        .first()
                        .is_some_and(|group| group.len() == 1 && group[0].name == "self");
                    if !is_method {
                        self.error(format!(
                            "associated trait function `{}.{name}` is not supported; declare a `self` receiver",
                            definition.name
                        ));
                        valid = false;
                    }
                    method_order.push(name.clone());
                    methods.insert(name, function);
                }
            }
        }
        self.traits.insert(
            definition.name,
            TraitSchema {
                compile_parameters,
                associated_types,
                methods,
                method_order,
                valid,
            },
        );
    }

    fn validate_trait_schemas(&mut self) {
        let mut trait_names = self.traits.keys().cloned().collect::<Vec<_>>();
        trait_names.sort();
        for trait_name in trait_names {
            let schema = self.traits[&trait_name].clone();
            let mut type_names = schema
                .compile_parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<HashSet<_>>();
            type_names.insert("Self".to_owned());
            type_names.extend(schema.associated_types.iter().cloned());
            let mut valid = schema.valid;
            for method_name in &schema.method_order {
                let method = &schema.methods[method_name];
                let mut method_type_names = type_names.clone();
                method_type_names.extend(
                    method
                        .compile_groups
                        .iter()
                        .flatten()
                        .map(|parameter| parameter.name.clone()),
                );
                for parameter in method.groups.iter().flatten() {
                    valid &= self.validate_trait_source_type(
                        &trait_name,
                        method_name,
                        &parameter.ty,
                        &method_type_names,
                    );
                    if parameter.mode == PassMode::Copy
                        && !Self::trait_source_type_is_definitely_copy(&parameter.ty)
                    {
                        self.error(format!(
                            "trait method `{}.{method_name}` parameter `{}` requires `Copy`, but its type is not provably Copy without a trait bound",
                            trait_name,
                            parameter.name
                        ));
                        valid = false;
                    }
                }
                if let Some(result) = &method.return_type {
                    valid &= self.validate_trait_source_type(
                        &trait_name,
                        method_name,
                        result,
                        &method_type_names,
                    );
                }
            }
            self.traits
                .get_mut(&trait_name)
                .expect("trait schema exists")
                .valid = valid;
        }
    }

    fn trait_source_type_is_definitely_copy(source: &Type) -> bool {
        match source {
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Void => true,
            Type::Array(element, _) => Self::trait_source_type_is_definitely_copy(element),
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => true,
            Type::Infer | Type::Named(_, _) => false,
        }
    }

    fn validate_trait_source_type(
        &mut self,
        trait_name: &str,
        member_name: &str,
        source: &Type,
        type_names: &HashSet<String>,
    ) -> bool {
        match source {
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Void => true,
            Type::Infer => {
                self.error(format!(
                    "trait member `{trait_name}.{member_name}` cannot use inferred type `_`"
                ));
                false
            }
            Type::Array(element, length) => {
                let mut valid = true;
                if *length > i32::MAX as u64 {
                    self.error(format!(
                        "array length {length} in trait member `{trait_name}.{member_name}` exceeds the first-version limit"
                    ));
                    valid = false;
                }
                valid &=
                    self.validate_trait_source_type(trait_name, member_name, element, type_names);
                valid
            }
            Type::Named(name, arguments) if type_names.contains(name) => {
                if arguments.is_empty() {
                    true
                } else {
                    self.error(format!(
                        "trait type parameter `{name}` in `{trait_name}.{member_name}` does not accept type arguments"
                    ));
                    false
                }
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => true,
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name) {
                    true
                } else if self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
                {
                    self.error(format!(
                        "generic type `{name}` in trait member `{trait_name}.{member_name}` requires type arguments"
                    ));
                    false
                } else {
                    self.error(format!(
                        "unknown type `{name}` in trait member `{trait_name}.{member_name}`"
                    ));
                    false
                }
            }
            Type::Named(name, arguments) => {
                let expected = self
                    .struct_templates
                    .get(name)
                    .map(|template| template.compile_groups.iter().flatten().count())
                    .or_else(|| {
                        self.enum_templates
                            .get(name)
                            .map(|template| template.compile_groups.iter().flatten().count())
                    });
                let Some(expected) = expected else {
                    if self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name) {
                        self.error(format!(
                            "non-generic type `{name}` in trait member `{trait_name}.{member_name}` does not accept type arguments"
                        ));
                    } else {
                        self.error(format!(
                            "unknown generic type `{name}` in trait member `{trait_name}.{member_name}`"
                        ));
                    }
                    return false;
                };
                let mut valid = true;
                if arguments.len() != expected {
                    self.error(format!(
                        "type argument count mismatch for `{name}` in trait member `{trait_name}.{member_name}`: expected {expected}, found {}",
                        arguments.len()
                    ));
                    valid = false;
                }
                for argument in arguments {
                    valid &= self.validate_trait_source_type(
                        trait_name,
                        member_name,
                        argument,
                        type_names,
                    );
                }
                valid
            }
        }
    }

    fn source_type_is_concrete(&self, source: &Type) -> bool {
        match source {
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Void => true,
            Type::Infer => false,
            Type::Array(element, _) => self.source_type_is_concrete(element),
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => true,
            Type::Named(name, arguments) if arguments.is_empty() => {
                self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name)
            }
            Type::Named(name, arguments) => {
                let expected = self
                    .struct_templates
                    .get(name)
                    .map(|template| template.compile_groups.iter().flatten().count())
                    .or_else(|| {
                        self.enum_templates
                            .get(name)
                            .map(|template| template.compile_groups.iter().flatten().count())
                    });
                expected == Some(arguments.len())
                    && arguments
                        .iter()
                        .all(|argument| self.source_type_is_concrete(argument))
            }
        }
    }

    fn resolve_trait_impl_target(&mut self, source: &Type) -> Option<Ty> {
        let Type::Named(name, arguments) = source else {
            self.error("trait implementation target must be a nominal type");
            return None;
        };
        if (self.struct_templates.contains_key(name) || self.enum_templates.contains_key(name))
            && (arguments.is_empty() || !self.source_type_is_concrete(source))
        {
            self.error(format!(
                "generic trait implementation for `{name}` is not supported; use a concrete type such as `{name}(i32)`"
            ));
            return None;
        }
        if arguments.is_empty()
            && !self.struct_defs.contains_key(name)
            && !self.enum_defs.contains_key(name)
        {
            self.error(format!("unknown extension target `{name}`"));
            return None;
        }
        let target = self.lower_source_type(source);
        match target {
            Ty::Struct(_) | Ty::Enum(_) => Some(target),
            Ty::Error => None,
            _ => {
                self.error("trait implementation target must be a nominal type");
                None
            }
        }
    }

    fn resolve_trait_impl_ref(
        &mut self,
        source: &Type,
    ) -> Option<(TraitRefKey, TraitSchema, HashMap<String, Type>)> {
        let Type::Named(name, source_arguments) = source else {
            self.error("trait reference must name a trait");
            return None;
        };
        let Some(schema) = self.traits.get(name).cloned() else {
            self.error(format!("unknown trait `{name}`"));
            return None;
        };
        if !schema.valid {
            return None;
        }
        if source_arguments.len() != schema.compile_parameters.len() {
            self.error(format!(
                "trait argument count mismatch for `{name}`: expected {}, found {}",
                schema.compile_parameters.len(),
                source_arguments.len()
            ));
            return None;
        }
        if source_arguments
            .iter()
            .any(|argument| !self.source_type_is_concrete(argument))
        {
            self.error(format!(
                "generic trait implementation of `{name}` is not supported; trait arguments must be concrete"
            ));
            return None;
        }
        let mut arguments = Vec::new();
        let mut substitutions = HashMap::new();
        for (parameter, source_argument) in schema.compile_parameters.iter().zip(source_arguments) {
            let argument = self.lower_source_type(source_argument);
            if argument == Ty::Error {
                return None;
            }
            arguments.push(argument);
            substitutions.insert(parameter.name.clone(), source_argument.clone());
        }
        Some((
            TraitRefKey {
                name: name.clone(),
                arguments,
            },
            schema,
            substitutions,
        ))
    }

    fn normalize_trait_impl_associated_type(
        &mut self,
        trait_name: &str,
        type_name: &str,
        raw: &HashMap<String, Type>,
        base_substitutions: &HashMap<String, Type>,
        normalized: &mut HashMap<String, Type>,
        visiting: &mut Vec<String>,
    ) -> Option<Type> {
        if let Some(ty) = normalized.get(type_name) {
            return Some(ty.clone());
        }
        if let Some(cycle_start) = visiting.iter().position(|name| name == type_name) {
            let mut cycle = visiting[cycle_start..].to_vec();
            cycle.push(type_name.to_owned());
            self.error(format!(
                "associated type cycle in implementation of `{trait_name}`: {}",
                cycle.join(" -> ")
            ));
            return None;
        }
        let source = raw.get(type_name)?.clone();
        visiting.push(type_name.to_owned());
        let resolved = self.normalize_trait_impl_type(
            trait_name,
            &source,
            raw,
            base_substitutions,
            normalized,
            visiting,
        );
        visiting.pop();
        if let Some(resolved) = &resolved {
            normalized.insert(type_name.to_owned(), resolved.clone());
        }
        resolved
    }

    fn normalize_trait_impl_type(
        &mut self,
        trait_name: &str,
        source: &Type,
        raw: &HashMap<String, Type>,
        base_substitutions: &HashMap<String, Type>,
        normalized: &mut HashMap<String, Type>,
        visiting: &mut Vec<String>,
    ) -> Option<Type> {
        match source {
            Type::Named(name, arguments) if arguments.is_empty() => {
                if raw.contains_key(name) {
                    self.normalize_trait_impl_associated_type(
                        trait_name,
                        name,
                        raw,
                        base_substitutions,
                        normalized,
                        visiting,
                    )
                } else {
                    Some(
                        base_substitutions
                            .get(name)
                            .cloned()
                            .unwrap_or_else(|| source.clone()),
                    )
                }
            }
            Type::Array(element, length) => Some(Type::Array(
                Box::new(self.normalize_trait_impl_type(
                    trait_name,
                    element,
                    raw,
                    base_substitutions,
                    normalized,
                    visiting,
                )?),
                *length,
            )),
            Type::Named(name, arguments) => Some(Type::Named(
                name.clone(),
                arguments
                    .iter()
                    .map(|argument| {
                        self.normalize_trait_impl_type(
                            trait_name,
                            argument,
                            raw,
                            base_substitutions,
                            normalized,
                            visiting,
                        )
                    })
                    .collect::<Option<Vec<_>>>()?,
            )),
            Type::I32
            | Type::I64
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Void
            | Type::Infer => Some(source.clone()),
        }
    }

    fn function_shape(&mut self, function: &Function) -> Option<FunctionShape> {
        let groups = function
            .groups
            .iter()
            .map(|group| {
                group
                    .iter()
                    .map(|parameter| {
                        let ty = self.lower_source_type(&parameter.ty);
                        (ty != Ty::Error).then_some((parameter.mode, ty))
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;
        let result_source = function.return_type.as_ref()?;
        let result = self.lower_source_type(result_source);
        (result != Ty::Error).then_some(FunctionShape { groups, result })
    }

    fn collect_trait_extension(&mut self, extension: ExtendDef) {
        let target_source = extension.target.clone();
        let Some(target) = self.resolve_trait_impl_target(&target_source) else {
            return;
        };
        let trait_source = extension
            .trait_ref
            .as_ref()
            .expect("trait extension has a trait reference");
        let Some((trait_ref, schema, mut substitutions)) =
            self.resolve_trait_impl_ref(trait_source)
        else {
            return;
        };
        let key = TraitImplKey {
            self_ty: target.clone(),
            trait_ref,
        };
        if !self.trait_impl_headers.insert(key.clone()) {
            self.error(format!(
                "duplicate trait implementation of `{}` for `{target}`",
                key.trait_ref.name
            ));
            return;
        }
        substitutions.insert("Self".to_owned(), target_source);

        let mut raw_associated = HashMap::new();
        let mut supplied_methods = HashMap::new();
        let mut valid = true;
        for member in extension.members {
            match member {
                ExtendMember::Const(binding) => {
                    if !schema.associated_types.contains(&binding.name) {
                        self.error(format!(
                            "unknown trait member `{}.{}`",
                            key.trait_ref.name, binding.name
                        ));
                        valid = false;
                        continue;
                    }
                    if binding.annotation.is_some() {
                        self.error(format!(
                            "associated type `{}.{}` must not have a value annotation",
                            key.trait_ref.name, binding.name
                        ));
                        valid = false;
                    }
                    let Some(source) = self.type_argument_from_expr(&binding.value, &substitutions)
                    else {
                        valid = false;
                        continue;
                    };
                    if raw_associated
                        .insert(binding.name.clone(), source)
                        .is_some()
                    {
                        self.error(format!(
                            "duplicate associated type `{}.{}`",
                            key.trait_ref.name, binding.name
                        ));
                        valid = false;
                    }
                }
                ExtendMember::Function(function) => {
                    if !schema.methods.contains_key(&function.name) {
                        self.error(format!(
                            "unknown trait member `{}.{}`",
                            key.trait_ref.name, function.name
                        ));
                        valid = false;
                        continue;
                    }
                    if supplied_methods
                        .insert(function.name.clone(), function.clone())
                        .is_some()
                    {
                        self.error(format!(
                            "duplicate trait method `{}.{}`",
                            key.trait_ref.name, function.name
                        ));
                        valid = false;
                    }
                }
            }
        }

        for associated in &schema.associated_types {
            if !raw_associated.contains_key(associated) {
                self.error(format!(
                    "missing associated type `{}.{associated}` in trait implementation",
                    key.trait_ref.name
                ));
                valid = false;
            }
        }
        for method in &schema.method_order {
            if !supplied_methods.contains_key(method) {
                self.error(format!(
                    "missing trait method `{}.{method}` in implementation for `{target}`",
                    key.trait_ref.name
                ));
                valid = false;
            }
        }
        if !valid {
            return;
        }

        let mut normalized_sources = HashMap::new();
        for associated in &schema.associated_types {
            if self
                .normalize_trait_impl_associated_type(
                    &key.trait_ref.name,
                    associated,
                    &raw_associated,
                    &substitutions,
                    &mut normalized_sources,
                    &mut Vec::new(),
                )
                .is_none()
            {
                valid = false;
            }
        }
        if !valid {
            return;
        }
        let mut associated_types = HashMap::new();
        for (name, source) in &normalized_sources {
            let ty = self.lower_source_type(source);
            if ty == Ty::Error {
                valid = false;
            } else {
                associated_types.insert(name.clone(), ty);
                substitutions.insert(name.clone(), source.clone());
            }
        }
        if !valid {
            return;
        }

        let mut registered = Vec::new();
        for method_name in &schema.method_order {
            let mut expected = schema.methods[method_name].clone();
            substitute_function_types(&mut expected, &substitutions);
            let Some(expected_shape) = self.function_shape(&expected) else {
                valid = false;
                continue;
            };

            let mut function = supplied_methods[method_name].clone();
            if !function.compile_groups.is_empty() {
                self.error(format!(
                    "generic trait implementation method `{}.{method_name}` is not supported",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            if function.body.is_none() {
                self.error(format!(
                    "trait implementation method `{}.{method_name}` requires a body",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            let is_method = function
                .groups
                .first()
                .is_some_and(|group| group.len() == 1 && group[0].name == "self");
            if !is_method {
                self.error(format!(
                    "trait method `{}.{method_name}` signature mismatch: implementation requires a contextual `self` receiver",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            substitute_function_types(&mut function, &substitutions);
            let Some(actual_shape) = self.function_shape(&function) else {
                self.error(format!(
                    "trait method `{}.{method_name}` signature mismatch",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            };
            if actual_shape != expected_shape {
                self.error(format!(
                    "trait method `{}.{method_name}` signature mismatch: expected {expected_shape:?}, found {actual_shape:?}",
                    key.trait_ref.name
                ));
                valid = false;
                continue;
            }
            let canonical = trait_method_name(&key, method_name);
            function.name = canonical.clone();
            registered.push((method_name.clone(), canonical, function));
        }
        if !valid {
            return;
        }

        let mut methods = HashMap::new();
        for (method_name, canonical, function) in registered {
            self.function_order.push(canonical.clone());
            self.functions.insert(canonical.clone(), function);
            self.function_type_substitutions
                .insert(canonical.clone(), substitutions.clone());
            methods.insert(method_name.clone(), canonical);
            self.trait_methods_by_receiver
                .entry((target.clone(), method_name))
                .or_default()
                .push(key.clone());
        }
        self.trait_impls.insert(
            key.clone(),
            TraitImplInfo {
                key,
                associated_types,
                methods,
            },
        );
    }

    fn collect_extension(&mut self, extension: ExtendDef) {
        if extension.trait_ref.is_some() {
            self.collect_trait_extension(extension);
            return;
        }
        let target = match extension.target {
            Type::Named(name, arguments) if arguments.is_empty() => name,
            Type::Named(name, _) => {
                self.error(format!(
                    "generic extend target `{name}` is not supported in M1"
                ));
                return;
            }
            _ => {
                self.error("extend target must be a non-generic nominal type in M1");
                return;
            }
        };
        if self.struct_templates.contains_key(&target) || self.enum_templates.contains_key(&target)
        {
            self.error(format!(
                "generic extend target `{target}` is not supported in the first generic slice"
            ));
            return;
        }
        if !self.struct_defs.contains_key(&target) && !self.enum_defs.contains_key(&target) {
            self.error(format!("unknown extension target `{target}`"));
            return;
        }

        for member in extension.members {
            match member {
                ExtendMember::Function(mut function) => {
                    if !function.compile_groups.is_empty() {
                        self.error(
                            "generic extend functions are not supported in the first generic slice",
                        );
                        continue;
                    }
                    let short_name = function.name.clone();
                    let is_method = function
                        .groups
                        .first()
                        .is_some_and(|group| group.len() == 1 && group[0].name == "self");
                    if is_method
                        && self.struct_layouts.get(&target).is_some_and(|layout| {
                            layout.fields.iter().any(|field| field.name == short_name)
                        })
                    {
                        self.error(format!(
                            "inherent method `{target}.{short_name}` conflicts with field `{short_name}`"
                        ));
                        continue;
                    }
                    if !is_method
                        && self.enum_layouts.get(&target).is_some_and(|layout| {
                            layout
                                .variants
                                .iter()
                                .any(|variant| variant.name == short_name)
                        })
                    {
                        self.error(format!(
                            "associated function `{target}.{short_name}` conflicts with variant `{short_name}`"
                        ));
                        continue;
                    }

                    let duplicate = {
                        let members = self.inherent_members.entry(target.clone()).or_default();
                        if is_method {
                            members.methods.contains_key(&short_name)
                        } else {
                            members.functions.contains_key(&short_name)
                                || members.constants.contains_key(&short_name)
                        }
                    };
                    if duplicate {
                        self.error(if is_method {
                            format!("duplicate inherent method `{target}.{short_name}`")
                        } else {
                            format!("duplicate associated member `{target}.{short_name}`")
                        });
                        continue;
                    }

                    for group in &mut function.groups {
                        for parameter in group {
                            substitute_self_type(&mut parameter.ty, &target);
                        }
                    }
                    if let Some(result) = &mut function.return_type {
                        substitute_self_type(result, &target);
                    }
                    let canonical = if is_method {
                        inherent_method_name(&target, &short_name)
                    } else {
                        associated_function_name(&target, &short_name)
                    };
                    function.name = canonical.clone();
                    self.function_order.push(canonical.clone());
                    self.functions.insert(canonical.clone(), function);
                    let members = self.inherent_members.entry(target.clone()).or_default();
                    if is_method {
                        members.methods.insert(short_name, canonical);
                    } else {
                        members.functions.insert(short_name, canonical);
                    }
                }
                ExtendMember::Const(mut binding) => {
                    let short_name = binding.name.clone();
                    if self.enum_layouts.get(&target).is_some_and(|layout| {
                        layout
                            .variants
                            .iter()
                            .any(|variant| variant.name == short_name)
                    }) {
                        self.error(format!(
                            "associated constant `{target}.{short_name}` conflicts with variant `{short_name}`"
                        ));
                        continue;
                    }
                    let duplicate = self
                        .inherent_members
                        .entry(target.clone())
                        .or_default()
                        .constants
                        .contains_key(&short_name)
                        || self
                            .inherent_members
                            .get(&target)
                            .is_some_and(|members| members.functions.contains_key(&short_name));
                    if duplicate {
                        self.error(format!(
                            "duplicate associated member `{target}.{short_name}`"
                        ));
                        continue;
                    }
                    if let Some(annotation) = &mut binding.annotation {
                        substitute_self_type(annotation, &target);
                    }
                    let canonical = associated_constant_name(&target, &short_name);
                    binding.name = canonical.clone();
                    self.global_order.push(canonical.clone());
                    self.globals.insert(canonical.clone(), binding);
                    self.inherent_members
                        .entry(target.clone())
                        .or_default()
                        .constants
                        .insert(short_name, canonical);
                }
            }
        }
    }

    fn collect_nominal_layouts(&mut self) {
        for name in self.struct_order.clone() {
            let is_ready = self
                .nominal_instances
                .get(&name)
                .and_then(|instance| self.nominal_instance_states.get(&instance.key))
                == Some(&NominalInstanceState::Ready);
            if is_ready {
                continue;
            }
            let definition = self.struct_defs[&name].clone();
            self.build_struct_layout(&name, definition);
        }

        for name in self.enum_order.clone() {
            let is_ready = self
                .nominal_instances
                .get(&name)
                .and_then(|instance| self.nominal_instance_states.get(&instance.key))
                == Some(&NominalInstanceState::Ready);
            if is_ready {
                continue;
            }
            let definition = self.enum_defs[&name].clone();
            self.build_enum_layout(&name, definition);
        }
    }

    fn build_struct_layout(&mut self, name: &str, definition: StructDef) {
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
        self.struct_layouts.insert(
            name.to_owned(),
            StructLayout {
                name: name.to_owned(),
                fields,
            },
        );
        if let Some(info) = self.nominal_instances.get(name) {
            self.nominal_instance_states
                .insert(info.key.clone(), NominalInstanceState::Ready);
        }
    }

    fn build_enum_layout(&mut self, name: &str, definition: EnumDef) {
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
        self.enum_layouts.insert(
            name.to_owned(),
            EnumLayout {
                name: name.to_owned(),
                variants,
            },
        );
        if let Some(info) = self.nominal_instances.get(name) {
            self.nominal_instance_states
                .insert(info.key.clone(), NominalInstanceState::Ready);
        }
    }

    fn snapshot_nominals(&self) -> NominalSnapshot {
        NominalSnapshot {
            struct_defs: self.struct_defs.clone(),
            enum_defs: self.enum_defs.clone(),
            struct_layouts: self.struct_layouts.clone(),
            enum_layouts: self.enum_layouts.clone(),
            struct_order: self.struct_order.clone(),
            enum_order: self.enum_order.clone(),
            instance_names: self.nominal_instance_names.clone(),
            instances: self.nominal_instances.clone(),
            states: self.nominal_instance_states.clone(),
            invalid_recursive_nominals: self.invalid_recursive_nominals.clone(),
        }
    }

    fn restore_nominals(&mut self, snapshot: NominalSnapshot) {
        self.struct_defs = snapshot.struct_defs;
        self.enum_defs = snapshot.enum_defs;
        self.struct_layouts = snapshot.struct_layouts;
        self.enum_layouts = snapshot.enum_layouts;
        self.struct_order = snapshot.struct_order;
        self.enum_order = snapshot.enum_order;
        self.nominal_instance_names = snapshot.instance_names;
        self.nominal_instances = snapshot.instances;
        self.nominal_instance_states = snapshot.states;
        self.invalid_recursive_nominals = snapshot.invalid_recursive_nominals;
    }

    fn validate_generic_nominal_cycles(&mut self) {
        let nominal_names: HashSet<_> = self
            .struct_defs
            .keys()
            .chain(self.enum_defs.keys())
            .chain(self.struct_templates.keys())
            .chain(self.enum_templates.keys())
            .cloned()
            .collect();
        let generic_names: HashSet<_> = self
            .struct_templates
            .keys()
            .chain(self.enum_templates.keys())
            .cloned()
            .collect();
        let mut dependencies = HashMap::new();
        for (name, definition) in self.struct_defs.iter().chain(&self.struct_templates) {
            let bound: HashSet<_> = definition
                .compile_groups
                .iter()
                .flatten()
                .map(|parameter| parameter.name.as_str())
                .collect();
            let mut direct = Vec::new();
            for field in &definition.fields {
                collect_nominal_type_dependencies(&field.ty, &nominal_names, &bound, &mut direct);
            }
            dependencies.insert(name.clone(), direct);
        }
        for (name, definition) in self.enum_defs.iter().chain(&self.enum_templates) {
            let bound: HashSet<_> = definition
                .compile_groups
                .iter()
                .flatten()
                .map(|parameter| parameter.name.as_str())
                .collect();
            let mut direct = Vec::new();
            for variant in &definition.variants {
                match &variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Positional(types) => {
                        for ty in types {
                            collect_nominal_type_dependencies(
                                ty,
                                &nominal_names,
                                &bound,
                                &mut direct,
                            );
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            collect_nominal_type_dependencies(
                                &field.ty,
                                &nominal_names,
                                &bound,
                                &mut direct,
                            );
                        }
                    }
                }
            }
            dependencies.insert(name.clone(), direct);
        }

        let mut states = HashMap::new();
        let mut stack = Vec::new();
        let names: Vec<_> = nominal_names.into_iter().collect();
        for name in names {
            self.visit_generic_nominal_cycle(
                &name,
                &dependencies,
                &generic_names,
                &mut states,
                &mut stack,
            );
        }
    }

    fn visit_generic_nominal_cycle(
        &mut self,
        name: &str,
        dependencies: &HashMap<String, Vec<String>>,
        generic_names: &HashSet<String>,
        states: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
    ) {
        match states.get(name).copied() {
            Some(2) => return,
            Some(1) => {
                let start = stack.iter().position(|item| item == name).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(name.to_owned());
                if cycle.iter().any(|item| generic_names.contains(item)) {
                    for item in &cycle {
                        if generic_names.contains(item) {
                            self.invalid_recursive_nominals.insert(item.clone());
                        }
                    }
                    self.error(format!(
                        "recursive generic value layout has infinite size: {}",
                        cycle.join(" -> ")
                    ));
                }
                return;
            }
            _ => {}
        }
        states.insert(name.to_owned(), 1);
        stack.push(name.to_owned());
        if let Some(items) = dependencies.get(name) {
            for dependency in items {
                self.visit_generic_nominal_cycle(
                    dependency,
                    dependencies,
                    generic_names,
                    states,
                    stack,
                );
            }
        }
        stack.pop();
        states.insert(name.to_owned(), 2);
    }

    fn validate_nominal_templates(&mut self) {
        let templates: Vec<_> = self
            .struct_template_order
            .iter()
            .map(|name| (NominalKind::Struct, name.clone()))
            .chain(
                self.enum_template_order
                    .iter()
                    .map(|name| (NominalKind::Enum, name.clone())),
            )
            .collect();
        for (kind, template_name) in templates {
            if self.invalid_recursive_nominals.contains(&template_name) {
                continue;
            }
            let parameters = match kind {
                NominalKind::Struct => self.struct_templates[&template_name]
                    .compile_groups
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
                NominalKind::Enum => self.enum_templates[&template_name]
                    .compile_groups
                    .iter()
                    .flatten()
                    .cloned()
                    .collect::<Vec<_>>(),
            };
            let mut source_arguments = Vec::new();
            let mut arguments = Vec::new();
            for (index, parameter) in parameters.iter().enumerate() {
                let owner = format!("nominal::{template_name}");
                let marker = generic_parameter_marker(&owner, index, &parameter.name);
                self.abstract_type_parameters
                    .insert(marker.clone(), parameter.name.clone());
                source_arguments.push(Type::Named(marker.clone(), Vec::new()));
                arguments.push(Ty::Struct(marker));
            }
            let snapshot = self.snapshot_nominals();
            if let Some(canonical) =
                self.ensure_nominal_instance(kind, &template_name, source_arguments, arguments)
            {
                let mut states = HashMap::new();
                let mut stack = Vec::new();
                self.visit_nominal_layout(&canonical, &mut states, &mut stack);
            }
            let dynamically_invalid = self.invalid_recursive_nominals.contains(&template_name);
            self.restore_nominals(snapshot);
            if dynamically_invalid {
                self.invalid_recursive_nominals.insert(template_name);
            }
        }
    }

    fn ensure_nominal_instance(
        &mut self,
        kind: NominalKind,
        template_name: &str,
        source_arguments: Vec<Type>,
        arguments: Vec<Ty>,
    ) -> Option<String> {
        if self.invalid_recursive_nominals.contains(template_name) {
            return None;
        }
        let key = NominalInstanceKey {
            kind,
            template: template_name.to_owned(),
            arguments,
        };
        if let Some(canonical) = self.nominal_instance_names.get(&key) {
            let info = &self.nominal_instances[canonical];
            debug_assert_eq!(info.key, key);
            debug_assert_eq!(info.canonical, *canonical);
            match self.nominal_instance_states.get(&key) {
                Some(NominalInstanceState::Ready) => return Some(canonical.clone()),
                Some(NominalInstanceState::Building) => {
                    self.error(format!(
                        "recursive generic value layout has infinite size while instantiating `{template_name}`"
                    ));
                    self.invalid_recursive_nominals
                        .insert(template_name.to_owned());
                    return None;
                }
                None => {
                    self.error(format!(
                        "internal error: missing construction state for nominal instance `{canonical}`"
                    ));
                    return None;
                }
            }
        }
        if self.nominal_instance_states.iter().any(|(active, state)| {
            *state == NominalInstanceState::Building
                && active.kind == kind
                && active.template == template_name
                && !active.arguments.is_empty()
        }) {
            self.error(format!(
                "recursive generic value layout has infinite size while instantiating `{template_name}` with growing type arguments"
            ));
            self.invalid_recursive_nominals
                .insert(template_name.to_owned());
            return None;
        }
        let instance_count = self
            .nominal_instances
            .values()
            .filter(|instance| !instance.key.arguments.is_empty())
            .count();
        if instance_count >= MAX_NOMINAL_INSTANCES {
            self.error(format!(
                "generic nominal instance limit of {MAX_NOMINAL_INSTANCES} exceeded while instantiating `{template_name}`"
            ));
            return None;
        }

        let parameters = match kind {
            NominalKind::Struct => self.struct_templates[template_name]
                .compile_groups
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
            NominalKind::Enum => self.enum_templates[template_name]
                .compile_groups
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
        };
        if parameters.len() != source_arguments.len() {
            self.error(format!(
                "type argument count mismatch for `{template_name}`: expected {}, found {}",
                parameters.len(),
                source_arguments.len()
            ));
            return None;
        }
        let mut substitutions = HashMap::new();
        for (parameter, argument) in parameters.iter().zip(source_arguments) {
            if substitutions
                .insert(parameter.name.clone(), argument)
                .is_some()
            {
                self.error(format!(
                    "duplicate compile-time parameter `{}` in generic nominal `{template_name}`",
                    parameter.name
                ));
                return None;
            }
        }

        let canonical = nominal_instance_name(&key);
        if let Some(existing) = self.nominal_instances.get(&canonical) {
            self.error(format!(
                "internal error: nominal instance name collision between `{}` and `{template_name}`",
                existing.key.template
            ));
            return None;
        }
        self.nominal_instance_names
            .insert(key.clone(), canonical.clone());
        self.nominal_instances.insert(
            canonical.clone(),
            NominalInstanceInfo {
                key: key.clone(),
                canonical: canonical.clone(),
            },
        );
        self.nominal_instance_states
            .insert(key.clone(), NominalInstanceState::Building);

        match kind {
            NominalKind::Struct => {
                self.struct_order.push(canonical.clone());
                self.struct_layouts.insert(
                    canonical.clone(),
                    StructLayout {
                        name: canonical.clone(),
                        fields: Vec::new(),
                    },
                );
                let mut definition = self.struct_templates[template_name].clone();
                substitute_struct_types(&mut definition, &substitutions);
                definition.name = canonical.clone();
                definition.compile_groups.clear();
                self.struct_defs
                    .insert(canonical.clone(), definition.clone());
                self.build_struct_layout(&canonical, definition);
            }
            NominalKind::Enum => {
                self.enum_order.push(canonical.clone());
                self.enum_layouts.insert(
                    canonical.clone(),
                    EnumLayout {
                        name: canonical.clone(),
                        variants: Vec::new(),
                    },
                );
                let mut definition = self.enum_templates[template_name].clone();
                substitute_enum_types(&mut definition, &substitutions);
                definition.name = canonical.clone();
                definition.compile_groups.clear();
                self.enum_defs.insert(canonical.clone(), definition.clone());
                self.build_enum_layout(&canonical, definition);
            }
        }
        self.nominal_instance_states
            .insert(key, NominalInstanceState::Ready);
        Some(canonical)
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
        let mut function_index = 0;
        while function_index < self.function_order.len() {
            let name = self.function_order[function_index].clone();
            self.lower_function(&name);
            function_index += 1;
        }
        self.validate_nominal_layouts();
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

    fn validate_function_templates(&mut self) {
        for template_name in self.function_template_order.clone() {
            let template = self.function_templates[&template_name].clone();
            if template.return_type.is_none() {
                self.error(format!(
                    "generic function `{template_name}` requires an explicit return type"
                ));
                continue;
            }

            let mut substitutions = HashMap::new();
            for (index, parameter) in template.compile_groups.iter().flatten().enumerate() {
                let marker = generic_parameter_marker(&template_name, index, &parameter.name);
                self.abstract_type_parameters
                    .insert(marker.clone(), parameter.name.clone());
                if substitutions
                    .insert(parameter.name.clone(), Type::Named(marker, Vec::new()))
                    .is_some()
                {
                    self.error(format!(
                        "duplicate compile-time parameter `{}` in generic function `{template_name}`",
                        parameter.name
                    ));
                }
            }

            let functions_before = self.functions.clone();
            let function_order_before = self.function_order.clone();
            let signatures_before = self.signatures.clone();
            let function_states_before = self.function_states.clone();
            let hir_functions_before = self.hir_functions.clone();
            let global_states_before = self.global_states.clone();
            let hir_globals_before = self.hir_globals.clone();
            let nominals_before = self.snapshot_nominals();
            let instance_names_before = self.function_instance_names.clone();
            let instances_before = self.function_instances.clone();
            let type_substitutions_before = self.function_type_substitutions.clone();
            let lifted_functions_before = self.lifted_functions.clone();
            let next_closure = self.next_closure;

            let mut function = template;
            substitute_function_types(&mut function, &substitutions);
            let validation_name = generic_validation_name(&template_name);
            function.name = validation_name.clone();
            function.compile_groups.clear();
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
            self.functions.insert(validation_name.clone(), function);
            self.signatures
                .insert(validation_name.clone(), FunctionSig { groups, result });
            self.function_type_substitutions
                .insert(validation_name.clone(), substitutions);
            self.lower_function(&validation_name);
            self.functions = functions_before;
            self.function_order = function_order_before;
            self.signatures = signatures_before;
            self.function_states = function_states_before;
            self.hir_functions = hir_functions_before;
            self.global_states = global_states_before;
            self.hir_globals = hir_globals_before;
            self.restore_nominals(nominals_before);
            self.function_instance_names = instance_names_before;
            self.function_instances = instances_before;
            self.function_type_substitutions = type_substitutions_before;
            self.lifted_functions = lifted_functions_before;
            self.next_closure = next_closure;
        }
    }

    fn lower_source_type(&mut self, source: &Type) -> Ty {
        match source {
            Type::I32 => Ty::I32,
            Type::I64 => Ty::I64,
            Type::U32 => Ty::U32,
            Type::U64 => Ty::U64,
            Type::Bool => Ty::Bool,
            Type::Void => Ty::Unit,
            Type::Infer => {
                self.error("`_` type inference is not supported in the first generic slice");
                Ty::Error
            }
            Type::Array(element, length) => {
                let element = self.lower_source_type(element);
                if *length > i32::MAX as u64 {
                    self.error(format!(
                        "array length {length} exceeds the first-version limit of {}",
                        i32::MAX
                    ));
                    Ty::Error
                } else if element == Ty::Unit {
                    self.error("array element type `()` is not supported in the first version");
                    Ty::Error
                } else if !is_copy_type(&element) {
                    self.error(format!(
                        "array element type `{element}` must implement Copy in the first version"
                    ));
                    Ty::Error
                } else {
                    Ty::Array(Box::new(element), *length)
                }
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Ty::Unit,
            Type::Named(name, arguments)
                if arguments.is_empty() && self.abstract_type_parameters.contains_key(name) =>
            {
                Ty::Struct(name.clone())
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) {
                    Ty::Struct(name.clone())
                } else if self.enum_defs.contains_key(name) {
                    Ty::Enum(name.clone())
                } else if self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
                {
                    self.error(format!(
                        "generic type `{name}` requires explicit type arguments"
                    ));
                    Ty::Error
                } else {
                    self.error(format!("unknown type `{name}`"));
                    Ty::Error
                }
            }
            Type::Named(name, source_arguments) => {
                let kind = if self.struct_templates.contains_key(name) {
                    NominalKind::Struct
                } else if self.enum_templates.contains_key(name) {
                    NominalKind::Enum
                } else if self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name) {
                    self.error(format!(
                        "non-generic type `{name}` does not accept type arguments"
                    ));
                    return Ty::Error;
                } else {
                    self.error(format!("unknown generic type `{name}`"));
                    return Ty::Error;
                };
                let expected = match kind {
                    NominalKind::Struct => self.struct_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .count(),
                    NominalKind::Enum => self.enum_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .count(),
                };
                if source_arguments.len() != expected {
                    self.error(format!(
                        "type argument count mismatch for `{name}`: expected {expected}, found {}",
                        source_arguments.len()
                    ));
                    return Ty::Error;
                }
                let mut arguments = Vec::new();
                for argument in source_arguments {
                    let argument = self.lower_source_type(argument);
                    if argument == Ty::Error {
                        return Ty::Error;
                    }
                    arguments.push(argument);
                }
                let Some(canonical) =
                    self.ensure_nominal_instance(kind, name, source_arguments.clone(), arguments)
                else {
                    return Ty::Error;
                };
                match kind {
                    NominalKind::Struct => Ty::Struct(canonical),
                    NominalKind::Enum => Ty::Enum(canonical),
                }
            }
        }
    }

    fn struct_layout_or_diagnostic(&mut self, name: &str) -> Option<StructLayout> {
        if let Some(layout) = self.struct_layouts.get(name) {
            return Some(layout.clone());
        }
        if let Some(parameter) = self.abstract_type_parameters.get(name).cloned() {
            self.error(format!(
                "generic parameter `{parameter}` has no known fields or struct layout"
            ));
        } else {
            self.error(format!(
                "internal error: struct type `{name}` has no registered layout"
            ));
        }
        None
    }

    fn enum_layout_or_diagnostic(&mut self, name: &str) -> Option<EnumLayout> {
        if let Some(layout) = self.enum_layouts.get(name) {
            return Some(layout.clone());
        }
        if let Some(parameter) = self.abstract_type_parameters.get(name).cloned() {
            self.error(format!(
                "generic parameter `{parameter}` has no known variants or enum layout"
            ));
        } else {
            self.error(format!(
                "internal error: enum type `{name}` has no registered layout"
            ));
        }
        None
    }

    fn type_argument_from_expr(
        &mut self,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Infer => {
                self.error(
                    "nested `_` type argument inference is not supported; use `_` as a complete compile-time argument",
                );
                None
            }
            Expr::Unit => Some(Type::Void),
            Expr::Name(name) => {
                if let Some(replacement) = substitutions.get(name) {
                    return Some(replacement.clone());
                }
                Some(match name.as_str() {
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "bool" => Type::Bool,
                    "void" => Type::Void,
                    _ => Type::Named(name.clone(), Vec::new()),
                })
            }
            Expr::Call(callee, call_arguments) => {
                let Expr::Name(name) = callee.as_ref() else {
                    self.error("generic type arguments require a named type constructor");
                    return None;
                };
                if call_arguments
                    .iter()
                    .any(|argument| argument.label.is_some())
                {
                    self.error("generic type arguments cannot contain labeled arguments");
                    return None;
                }
                if name == "Array" {
                    if call_arguments.len() != 2 {
                        self.error("`Array` type arguments require an element type and length");
                        return None;
                    }
                    let element =
                        self.type_argument_from_expr(&call_arguments[0].value, substitutions)?;
                    let Expr::Integer(length) = call_arguments[1].value else {
                        self.error("array type argument length must be a non-negative integer");
                        return None;
                    };
                    let Ok(length) = u64::try_from(length) else {
                        self.error("array type argument length must fit in `u64`");
                        return None;
                    };
                    Some(Type::Array(Box::new(element), length))
                } else {
                    let mut arguments = Vec::new();
                    for argument in call_arguments {
                        arguments
                            .push(self.type_argument_from_expr(&argument.value, substitutions)?);
                    }
                    Some(Type::Named(name.clone(), arguments))
                }
            }
            _ => {
                self.error("generic type arguments must be type names, type applications, or `_`");
                None
            }
        }
    }

    fn source_type_for_ty(&self, ty: &Ty) -> Option<Type> {
        match ty {
            Ty::I32 => Some(Type::I32),
            Ty::I64 => Some(Type::I64),
            Ty::U32 => Some(Type::U32),
            Ty::U64 => Some(Type::U64),
            Ty::Bool => Some(Type::Bool),
            Ty::Unit => Some(Type::Void),
            Ty::Array(element, length) => Some(Type::Array(
                Box::new(self.source_type_for_ty(element)?),
                *length,
            )),
            Ty::Struct(name) | Ty::Enum(name) => {
                if let Some(instance) = self.nominal_instances.get(name) {
                    let arguments = instance
                        .key
                        .arguments
                        .iter()
                        .map(|argument| self.source_type_for_ty(argument))
                        .collect::<Option<Vec<_>>>()?;
                    Some(Type::Named(instance.key.template.clone(), arguments))
                } else if self.abstract_type_parameters.contains_key(name)
                    || self.struct_defs.contains_key(name)
                    || self.enum_defs.contains_key(name)
                {
                    Some(Type::Named(name.clone(), Vec::new()))
                } else {
                    None
                }
            }
            Ty::Never | Ty::Function(_) | Ty::Error => None,
        }
    }

    fn probe_type_argument_source(
        &self,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Infer => None,
            Expr::Unit => Some(Type::Void),
            Expr::Name(name) => substitutions.get(name).cloned().or_else(|| {
                Some(match name.as_str() {
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "bool" => Type::Bool,
                    "void" => Type::Void,
                    _ => Type::Named(name.clone(), Vec::new()),
                })
            }),
            Expr::Call(callee, arguments) => {
                let Expr::Name(name) = callee.as_ref() else {
                    return None;
                };
                if arguments.iter().any(|argument| argument.label.is_some()) {
                    return None;
                }
                if name == "Array" {
                    if arguments.len() != 2 {
                        return None;
                    }
                    let element =
                        self.probe_type_argument_source(&arguments[0].value, substitutions)?;
                    let Expr::Integer(length) = arguments[1].value else {
                        return None;
                    };
                    Some(Type::Array(Box::new(element), u64::try_from(length).ok()?))
                } else {
                    let arguments = arguments
                        .iter()
                        .map(|argument| {
                            self.probe_type_argument_source(&argument.value, substitutions)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(Type::Named(name.clone(), arguments))
                }
            }
            _ => None,
        }
    }

    fn probe_source_ty(&self, source: &Type) -> Option<Ty> {
        match source {
            Type::I32 => Some(Ty::I32),
            Type::I64 => Some(Ty::I64),
            Type::U32 => Some(Ty::U32),
            Type::U64 => Some(Ty::U64),
            Type::Bool => Some(Ty::Bool),
            Type::Void => Some(Ty::Unit),
            Type::Infer => None,
            Type::Array(element, length) => {
                Some(Ty::Array(Box::new(self.probe_source_ty(element)?), *length))
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Some(Ty::Unit),
            Type::Named(name, arguments)
                if arguments.is_empty() && self.abstract_type_parameters.contains_key(name) =>
            {
                Some(Ty::Struct(name.clone()))
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else if self.enum_defs.contains_key(name) {
                    Some(Ty::Enum(name.clone()))
                } else {
                    None
                }
            }
            Type::Named(name, source_arguments) => {
                let (kind, expected) = if let Some(template) = self.struct_templates.get(name) {
                    (
                        NominalKind::Struct,
                        template.compile_groups.iter().flatten().count(),
                    )
                } else if let Some(template) = self.enum_templates.get(name) {
                    (
                        NominalKind::Enum,
                        template.compile_groups.iter().flatten().count(),
                    )
                } else {
                    return None;
                };
                if source_arguments.len() != expected {
                    return None;
                }
                let arguments = source_arguments
                    .iter()
                    .map(|argument| self.probe_source_ty(argument))
                    .collect::<Option<Vec<_>>>()?;
                let key = NominalInstanceKey {
                    kind,
                    template: name.clone(),
                    arguments,
                };
                let canonical = self
                    .nominal_instance_names
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| nominal_instance_name(&key));
                Some(match kind {
                    NominalKind::Struct => Ty::Struct(canonical),
                    NominalKind::Enum => Ty::Enum(canonical),
                })
            }
        }
    }

    fn probe_generic_nominal_type_head(
        &self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> Option<(NominalKind, Ty, Type)> {
        let (kind, compile_groups) = if let Some(template) = self.struct_templates.get(name) {
            (NominalKind::Struct, &template.compile_groups)
        } else if let Some(template) = self.enum_templates.get(name) {
            (NominalKind::Enum, &template.compile_groups)
        } else {
            return None;
        };
        if groups.len() != compile_groups.len() {
            return None;
        }
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for (parameters, supplied) in compile_groups.iter().zip(groups) {
            if parameters.len() != supplied.len()
                || supplied.iter().any(|argument| argument.label.is_some())
            {
                return None;
            }
            for argument in *supplied {
                let source =
                    self.probe_type_argument_source(&argument.value, &context.type_substitutions)?;
                let ty = self.probe_source_ty(&source)?;
                source_arguments.push(source);
                arguments.push(ty);
            }
        }
        let key = NominalInstanceKey {
            kind,
            template: name.to_owned(),
            arguments,
        };
        let canonical = self
            .nominal_instance_names
            .get(&key)
            .cloned()
            .unwrap_or_else(|| nominal_instance_name(&key));
        let ty = match kind {
            NominalKind::Struct => Ty::Struct(canonical),
            NominalKind::Enum => Ty::Enum(canonical),
        };
        Some((kind, ty, Type::Named(name.to_owned(), source_arguments)))
    }

    fn probe_nominal_type_head(
        &self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Option<(NominalKind, Ty, Type)> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(name) = root else {
            return None;
        };
        if context.lookup(name).is_some() {
            return None;
        }
        if groups.is_empty() {
            if self.struct_defs.contains_key(name) {
                return Some((
                    NominalKind::Struct,
                    Ty::Struct(name.clone()),
                    Type::Named(name.clone(), Vec::new()),
                ));
            }
            if self.enum_defs.contains_key(name) {
                return Some((
                    NominalKind::Enum,
                    Ty::Enum(name.clone()),
                    Type::Named(name.clone(), Vec::new()),
                ));
            }
        }
        self.probe_generic_nominal_type_head(name, &groups, context)
    }

    fn probe_enum_variant_fields(&self, source: &Type, variant: &str) -> Option<VariantFields> {
        let Type::Named(template, _) = source else {
            return None;
        };
        self.enum_templates
            .get(template)
            .or_else(|| self.enum_defs.get(template))?
            .variants
            .iter()
            .find(|candidate| candidate.name == variant)
            .map(|candidate| candidate.fields.clone())
    }

    fn unify_template_ty(
        &self,
        template: &Type,
        actual: &Ty,
        actual_source: Option<&Type>,
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        origin: &str,
    ) -> Result<bool, String> {
        if let Type::Named(name, arguments) = template {
            if arguments.is_empty() && compile_parameters.contains(name) {
                if let Some(previous) = inferred.get_mut(name) {
                    if previous.ty == *actual {
                        if previous.source.is_none() {
                            previous.source = actual_source
                                .cloned()
                                .or_else(|| self.source_type_for_ty(actual));
                        }
                        return Ok(false);
                    }
                    return Err(format!(
                        "conflicting inference for type parameter `{name}`: `{}` from {} conflicts with `{actual}` from {origin}",
                        previous.ty, previous.origin
                    ));
                }
                if matches!(actual, Ty::Never | Ty::Error) {
                    return Err(format!(
                        "cannot infer type parameter `{name}` from `{actual}` in {origin}"
                    ));
                }
                inferred.insert(
                    name.clone(),
                    InferredTypeArgument {
                        ty: actual.clone(),
                        source: actual_source
                            .cloned()
                            .or_else(|| self.source_type_for_ty(actual)),
                        origin: origin.to_owned(),
                    },
                );
                return Ok(true);
            }
        }

        let mismatch = || {
            format!("type inference constraint from {origin} does not match actual type `{actual}`")
        };
        match template {
            Type::I32 => (*actual == Ty::I32).then_some(false).ok_or_else(mismatch),
            Type::I64 => (*actual == Ty::I64).then_some(false).ok_or_else(mismatch),
            Type::U32 => (*actual == Ty::U32).then_some(false).ok_or_else(mismatch),
            Type::U64 => (*actual == Ty::U64).then_some(false).ok_or_else(mismatch),
            Type::Bool => (*actual == Ty::Bool).then_some(false).ok_or_else(mismatch),
            Type::Void => (*actual == Ty::Unit).then_some(false).ok_or_else(mismatch),
            Type::Array(element, length) => {
                let Ty::Array(actual_element, actual_length) = actual else {
                    return Err(mismatch());
                };
                if length != actual_length {
                    return Err(mismatch());
                }
                self.unify_template_ty(
                    element,
                    actual_element,
                    match actual_source {
                        Some(Type::Array(element, _)) => Some(element),
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => {
                if *actual == Ty::Unit {
                    Ok(false)
                } else {
                    Err(mismatch())
                }
            }
            Type::Named(name, arguments) => {
                let (actual_kind, actual_name) = match actual {
                    Ty::Struct(name) => (NominalKind::Struct, name),
                    Ty::Enum(name) => (NominalKind::Enum, name),
                    _ => return Err(mismatch()),
                };
                if let Some(instance) = self.nominal_instances.get(actual_name) {
                    if instance.key.kind != actual_kind
                        || instance.key.template != *name
                        || instance.key.arguments.len() != arguments.len()
                    {
                        return Err(mismatch());
                    }
                    let actual_arguments = instance.key.arguments.clone();
                    let mut changed = false;
                    for (template, actual) in arguments.iter().zip(&actual_arguments) {
                        changed |= self.unify_template_ty(
                            template,
                            actual,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                    Ok(changed)
                } else if let Some(Type::Named(actual_template, source_arguments)) = actual_source {
                    if actual_template != name || source_arguments.len() != arguments.len() {
                        return Err(mismatch());
                    }
                    let mut changed = false;
                    for (template, source) in arguments.iter().zip(source_arguments) {
                        let Some(actual) = self.probe_source_ty(source) else {
                            return Err(mismatch());
                        };
                        changed |= self.unify_template_ty(
                            template,
                            &actual,
                            Some(source),
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                    Ok(changed)
                } else if arguments.is_empty() && name == actual_name {
                    Ok(false)
                } else {
                    Err(mismatch())
                }
            }
            Type::Infer => Err(format!(
                "nested `_` type inference is not supported in {origin}; use `_` as a complete compile-time argument"
            )),
        }
    }

    fn resolved_template_ty(
        &self,
        template: &Type,
        compile_parameters: &HashSet<String>,
        inferred: &HashMap<String, InferredTypeArgument>,
    ) -> Option<Ty> {
        match template {
            Type::I32 => Some(Ty::I32),
            Type::I64 => Some(Ty::I64),
            Type::U32 => Some(Ty::U32),
            Type::U64 => Some(Ty::U64),
            Type::Bool => Some(Ty::Bool),
            Type::Void => Some(Ty::Unit),
            Type::Infer => None,
            Type::Array(element, length) => Some(Ty::Array(
                Box::new(self.resolved_template_ty(element, compile_parameters, inferred)?),
                *length,
            )),
            Type::Named(name, arguments)
                if arguments.is_empty() && compile_parameters.contains(name) =>
            {
                inferred.get(name).map(|argument| argument.ty.clone())
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Some(Ty::Unit),
            Type::Named(name, arguments) => {
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        self.resolved_template_ty(argument, compile_parameters, inferred)
                    })
                    .collect::<Option<Vec<_>>>()?;
                if self.struct_templates.contains_key(name) {
                    let key = NominalInstanceKey {
                        kind: NominalKind::Struct,
                        template: name.clone(),
                        arguments,
                    };
                    Some(Ty::Struct(
                        self.nominal_instance_names
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| nominal_instance_name(&key)),
                    ))
                } else if self.enum_templates.contains_key(name) {
                    let key = NominalInstanceKey {
                        kind: NominalKind::Enum,
                        template: name.clone(),
                        arguments,
                    };
                    Some(Ty::Enum(
                        self.nominal_instance_names
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| nominal_instance_name(&key)),
                    ))
                } else if arguments.is_empty() && self.struct_defs.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else if arguments.is_empty() && self.enum_defs.contains_key(name) {
                    Some(Ty::Enum(name.clone()))
                } else if arguments.is_empty() && self.abstract_type_parameters.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else {
                    None
                }
            }
        }
    }

    fn nominal_ty_from_probe(probe: &TypeProbe) -> Option<Ty> {
        match probe {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _)
                if matches!(ty, Ty::Struct(_) | Ty::Enum(_)) =>
            {
                Some(ty.clone())
            }
            TypeProbe::Known(_)
            | TypeProbe::KnownSource(_, _)
            | TypeProbe::Defaultable(_)
            | TypeProbe::Unsupported => None,
        }
    }

    fn probe_matches_type(probe: &TypeProbe, expected: &Ty) -> bool {
        match probe {
            TypeProbe::Known(actual) | TypeProbe::KnownSource(actual, _) => actual == expected,
            TypeProbe::Defaultable(default) => default.is_integer() && expected.is_integer(),
            TypeProbe::Unsupported => true,
        }
    }

    fn add_candidates(
        &self,
        receiver: &Ty,
        right: &Expr,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> Vec<AddCandidate> {
        if !self.traits.get("Add").is_some_and(|schema| schema.valid) {
            return Vec::new();
        }

        let mut candidates = self
            .trait_impls
            .values()
            .filter_map(|implementation| {
                if implementation.key.self_ty != *receiver
                    || implementation.key.trait_ref.name != "Add"
                {
                    return None;
                }
                let [rhs] = implementation.key.trait_ref.arguments.as_slice() else {
                    return None;
                };
                let method = implementation.methods.get("add")?;
                let output = implementation.associated_types.get("Output")?;
                if integer_literal_value(right).is_some_and(|value| !integer_fits(value, rhs)) {
                    return None;
                }
                let right_probe = self.probe_expr_ty(right, Some(rhs), context);
                Self::probe_matches_type(&right_probe, rhs).then(|| AddCandidate {
                    method: method.clone(),
                    rhs: rhs.clone(),
                    output: output.clone(),
                })
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            canonical_type_encoding(&left.rhs)
                .cmp(&canonical_type_encoding(&right.rhs))
                .then_with(|| {
                    canonical_type_encoding(&left.output)
                        .cmp(&canonical_type_encoding(&right.output))
                })
                .then_with(|| left.method.cmp(&right.method))
        });

        if candidates.len() > 1 {
            if let Some(expected) = expected.filter(|ty| **ty != Ty::Error) {
                candidates.retain(|candidate| candidate.output == *expected);
            }
        }
        candidates
    }

    fn probe_numeric_binary_ty(
        &self,
        left: &Expr,
        right: &Expr,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let numeric_hint = hint.filter(|ty| ty.is_integer());
        match self.probe_expr_ty(left, numeric_hint, context) {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) if ty.is_integer() => {
                TypeProbe::Known(ty)
            }
            _ => match self.probe_expr_ty(right, numeric_hint, context) {
                TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) if ty.is_integer() => {
                    TypeProbe::Known(ty)
                }
                TypeProbe::Defaultable(ty) if ty.is_integer() => TypeProbe::Defaultable(ty),
                _ => TypeProbe::Unsupported,
            },
        }
    }

    fn probe_add_ty(
        &self,
        left: &Expr,
        right: &Expr,
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let left_probe = self.probe_expr_ty(left, None, context);
        if let Some(receiver) = Self::nominal_ty_from_probe(&left_probe) {
            let candidates = self.add_candidates(&receiver, right, expected, context);
            return match candidates.as_slice() {
                [candidate] => TypeProbe::Known(candidate.output.clone()),
                [] | [_, _, ..] => TypeProbe::Unsupported,
            };
        }
        self.probe_numeric_binary_ty(left, right, expected, context)
    }

    fn builtin_fallible_info_for_ty(&self, ty: &Ty) -> Option<BuiltinFallibleInfo> {
        let Ty::Enum(canonical) = ty else {
            return None;
        };
        let instance = self.nominal_instances.get(canonical)?;
        if instance.key.kind != NominalKind::Enum {
            return None;
        }
        match (
            instance.key.template.as_str(),
            instance.key.arguments.as_slice(),
        ) {
            ("Option", [payload]) => Some(BuiltinFallibleInfo {
                kind: BuiltinFallibleKind::Option,
                payload: payload.clone(),
                payload_source: self.source_type_for_ty(payload),
                error: None,
            }),
            ("Result", [payload, error]) => Some(BuiltinFallibleInfo {
                kind: BuiltinFallibleKind::Result,
                payload: payload.clone(),
                payload_source: self.source_type_for_ty(payload),
                error: Some(error.clone()),
            }),
            _ => None,
        }
    }

    fn builtin_fallible_info_for_probe(&self, probe: &TypeProbe) -> Option<BuiltinFallibleInfo> {
        let ty = match probe {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => ty,
            TypeProbe::Defaultable(_) | TypeProbe::Unsupported => return None,
        };
        if let Some(info) = self.builtin_fallible_info_for_ty(ty) {
            return Some(info);
        }
        let TypeProbe::KnownSource(Ty::Enum(_), Type::Named(template, arguments)) = probe else {
            return None;
        };
        match (template.as_str(), arguments.as_slice()) {
            ("Option", [payload]) => Some(BuiltinFallibleInfo {
                kind: BuiltinFallibleKind::Option,
                payload: self.probe_source_ty(payload)?,
                payload_source: Some(payload.clone()),
                error: None,
            }),
            ("Result", [payload, error]) => Some(BuiltinFallibleInfo {
                kind: BuiltinFallibleKind::Result,
                payload: self.probe_source_ty(payload)?,
                payload_source: Some(payload.clone()),
                error: Some(self.probe_source_ty(error)?),
            }),
            _ => None,
        }
    }

    fn return_boundary_for_ty(&self, ty: &Ty) -> Option<ReturnBoundary> {
        let info = self.builtin_fallible_info_for_ty(ty)?;
        Some(ReturnBoundary {
            kind: info.kind,
            container: ty.clone(),
            success: info.payload,
            error: info.error,
        })
    }

    fn inferred_try_operand_expected(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        boundary: &ReturnBoundary,
        context: &LowerCtx,
    ) -> Option<Ty> {
        let inferred = self.inferred_builtin_coalesce_lhs(expression, context)?;
        if inferred.kind != boundary.kind {
            return None;
        }

        let payload = expected
            .filter(|ty| !matches!(ty, Ty::Never | Ty::Error))
            .cloned()
            .or_else(|| self.inferred_try_payload(&inferred, context))?;
        let mut arguments = vec![payload];
        if inferred.kind == BuiltinFallibleKind::Result {
            arguments.push(boundary.error.clone()?);
        }
        let source_arguments = arguments
            .iter()
            .map(|argument| self.source_type_for_ty(argument))
            .collect::<Option<Vec<_>>>()?;
        let canonical = self.ensure_nominal_instance(
            NominalKind::Enum,
            &inferred.name,
            source_arguments,
            arguments,
        )?;
        Some(Ty::Enum(canonical))
    }

    fn inferred_try_payload(
        &self,
        inferred: &InferredCoalesceLhs<'_>,
        context: &LowerCtx,
    ) -> Option<Ty> {
        let [arguments] = inferred.type_groups.as_slice() else {
            return None;
        };
        let first = arguments.first()?;
        if first.label.is_some() {
            return None;
        }
        if !matches!(first.value, Expr::Infer) {
            let source =
                self.probe_type_argument_source(&first.value, &context.type_substitutions)?;
            return self.probe_source_ty(&source);
        }

        let success_variant = match inferred.kind {
            BuiltinFallibleKind::Option => "Some",
            BuiltinFallibleKind::Result => "Ok",
        };
        if inferred.variant != success_variant {
            return None;
        }
        let [values] = inferred.value_groups.as_slice() else {
            return None;
        };
        let [value] = *values else {
            return None;
        };
        if value.label.is_some() {
            return None;
        }
        match self.probe_expr_ty(&value.value, None, context) {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) | TypeProbe::Defaultable(ty) => {
                Some(ty)
            }
            TypeProbe::Unsupported => None,
        }
    }

    fn return_value_candidate(
        &self,
        expression: &Expr,
        boundary: &ReturnBoundary,
        context: &LowerCtx,
    ) -> ReturnValueCandidate {
        self.return_value_candidate_with_shadowing(expression, boundary, context, &HashSet::new())
    }

    fn return_value_candidate_with_shadowing(
        &self,
        expression: &Expr,
        boundary: &ReturnBoundary,
        context: &LowerCtx,
        shadowed_short_variants: &HashSet<String>,
    ) -> ReturnValueCandidate {
        match expression {
            Expr::Block(statements, Some(tail)) => {
                let mut nested_shadowing = shadowed_short_variants.clone();
                for statement in statements {
                    if let Stmt::Let(binding) = statement {
                        nested_shadowing.insert(binding.name.clone());
                    }
                }
                return self.return_value_candidate_with_shadowing(
                    tail,
                    boundary,
                    context,
                    &nested_shadowing,
                );
            }
            Expr::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                let then_candidate = self.return_value_candidate_with_shadowing(
                    then_branch,
                    boundary,
                    context,
                    shadowed_short_variants,
                );
                let else_candidate = self.return_value_candidate_with_shadowing(
                    else_branch,
                    boundary,
                    context,
                    shadowed_short_variants,
                );
                if then_candidate == else_candidate {
                    return then_candidate;
                }
            }
            Expr::Match { arms, .. } if !arms.is_empty() => {
                let first = self.return_value_candidate_with_shadowing(
                    &arms[0].body,
                    boundary,
                    context,
                    shadowed_short_variants,
                );
                if arms.iter().skip(1).all(|arm| {
                    self.return_value_candidate_with_shadowing(
                        &arm.body,
                        boundary,
                        context,
                        shadowed_short_variants,
                    ) == first
                }) {
                    return first;
                }
            }
            _ => {}
        }

        let container_probe = self.probe_expr_ty(expression, Some(&boundary.container), context);
        if Self::probe_matches_type(&container_probe, &boundary.container)
            && matches!(
                container_probe,
                TypeProbe::Known(_) | TypeProbe::KnownSource(_, _)
            )
        {
            return ReturnValueCandidate::Container;
        }
        if let Some(inferred) = self.inferred_builtin_coalesce_lhs(expression, context) {
            if inferred.kind == boundary.kind {
                return ReturnValueCandidate::Container;
            }
            if self
                .builtin_fallible_info_for_ty(&boundary.success)
                .is_some_and(|success| success.kind == inferred.kind)
            {
                return ReturnValueCandidate::Success;
            }
        }
        if self.is_short_boundary_variant(expression, boundary, context, shadowed_short_variants) {
            return ReturnValueCandidate::Container;
        }

        let success_probe = self.probe_expr_ty(expression, Some(&boundary.success), context);
        if Self::probe_matches_type(&success_probe, &boundary.success)
            && !matches!(success_probe, TypeProbe::Unsupported)
        {
            ReturnValueCandidate::Success
        } else {
            ReturnValueCandidate::Unknown
        }
    }

    fn is_short_boundary_variant(
        &self,
        expression: &Expr,
        boundary: &ReturnBoundary,
        context: &LowerCtx,
        shadowed_short_variants: &HashSet<String>,
    ) -> bool {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return false;
        };
        if context.lookup(name).is_some() || shadowed_short_variants.contains(name) {
            return false;
        }
        if self.functions.contains_key(name)
            || self.function_templates.contains_key(name)
            || self.globals.contains_key(name)
            || self.struct_defs.contains_key(name)
            || self.struct_templates.contains_key(name)
        {
            return false;
        }
        match boundary.kind {
            BuiltinFallibleKind::Option => matches!(name.as_str(), "Some" | "None"),
            BuiltinFallibleKind::Result => matches!(name.as_str(), "Ok" | "Err"),
        }
    }

    fn unknown_return_value_prefers_success(expression: &Expr) -> bool {
        match expression {
            Expr::Block(_, Some(tail)) => Self::unknown_return_value_prefers_success(tail),
            Expr::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                Self::unknown_return_value_prefers_success(then_branch)
                    && Self::unknown_return_value_prefers_success(else_branch)
            }
            Expr::Match { arms, .. } => {
                !arms.is_empty()
                    && arms
                        .iter()
                        .all(|arm| Self::unknown_return_value_prefers_success(&arm.body))
            }
            Expr::Unit
            | Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::Unary(_, _)
            | Expr::Borrow { .. }
            | Expr::Binary(_, _, _)
            | Expr::Coalesce(_, _)
            | Expr::Try(_)
            | Expr::Throw(_)
            | Expr::Array(_)
            | Expr::Index { .. } => true,
            Expr::Name(_)
            | Expr::Infer
            | Expr::Assign(_, _)
            | Expr::Call(_, _)
            | Expr::Member(_, _)
            | Expr::Block(_, _)
            | Expr::Closure(_, _)
            | Expr::If { .. }
            | Expr::Return(_)
            | Expr::While { .. }
            | Expr::Loop { .. }
            | Expr::Break(_) => false,
        }
    }

    fn inferred_builtin_coalesce_lhs<'a>(
        &self,
        expression: &'a Expr,
        context: &LowerCtx,
    ) -> Option<InferredCoalesceLhs<'a>> {
        let mut value_groups = Vec::new();
        let Expr::Member(base, variant) = flatten_call(expression, &mut value_groups) else {
            return None;
        };
        let (name, type_groups) = self.inferred_generic_enum_type_head(base, context)?;
        let kind = match name.as_str() {
            "Option" => BuiltinFallibleKind::Option,
            "Result" => BuiltinFallibleKind::Result,
            _ => return None,
        };
        Some(InferredCoalesceLhs {
            kind,
            name,
            type_groups,
            variant: variant.as_str(),
            value_groups,
        })
    }

    fn inferred_coalesce_payload_probe(
        &self,
        kind: BuiltinFallibleKind,
        type_groups: &[&[CallArg]],
        hint: Option<&Ty>,
        right: &Expr,
        context: &LowerCtx,
    ) -> TypeProbe {
        let expected_arguments = match kind {
            BuiltinFallibleKind::Option => 1,
            BuiltinFallibleKind::Result => 2,
        };
        let [arguments] = type_groups else {
            return TypeProbe::Unsupported;
        };
        if arguments.len() != expected_arguments || arguments[0].label.is_some() {
            return TypeProbe::Unsupported;
        }
        if !matches!(arguments[0].value, Expr::Infer) {
            let Some(source) =
                self.probe_type_argument_source(&arguments[0].value, &context.type_substitutions)
            else {
                return TypeProbe::Unsupported;
            };
            let Some(ty) = self.probe_source_ty(&source) else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::KnownSource(ty, source);
        }
        if let Some(hint) = hint.filter(|ty| **ty != Ty::Error) {
            return self.source_type_for_ty(hint).map_or_else(
                || TypeProbe::Known(hint.clone()),
                |source| TypeProbe::KnownSource(hint.clone(), source),
            );
        }
        self.probe_expr_ty(right, None, context)
    }

    fn probe_coalesce_ty(
        &self,
        left: &Expr,
        right: &Expr,
        hint: Option<&Ty>,
        context: &LowerCtx,
    ) -> TypeProbe {
        let left_probe = self.probe_expr_ty(left, None, context);
        let payload = if let Some(info) = self.builtin_fallible_info_for_probe(&left_probe) {
            match info.payload_source {
                Some(source) => TypeProbe::KnownSource(info.payload, source),
                None => TypeProbe::Known(info.payload),
            }
        } else {
            let Some(inferred) = self.inferred_builtin_coalesce_lhs(left, context) else {
                return TypeProbe::Unsupported;
            };
            self.inferred_coalesce_payload_probe(
                inferred.kind,
                &inferred.type_groups,
                hint,
                right,
                context,
            )
        };
        let payload_ty = match &payload {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) | TypeProbe::Defaultable(ty) => ty,
            TypeProbe::Unsupported => return TypeProbe::Unsupported,
        };
        let right = self.probe_expr_ty(right, Some(payload_ty), context);
        if Self::probe_matches_type(&right, payload_ty) {
            payload
        } else {
            TypeProbe::Unsupported
        }
    }

    fn probe_expr_ty(&self, expression: &Expr, hint: Option<&Ty>, context: &LowerCtx) -> TypeProbe {
        match expression {
            Expr::Integer(_) => hint
                .filter(|ty| ty.is_integer())
                .cloned()
                .map_or(TypeProbe::Defaultable(Ty::I32), TypeProbe::Known),
            Expr::Bool(_) => TypeProbe::Known(Ty::Bool),
            Expr::Unit => TypeProbe::Known(Ty::Unit),
            Expr::Name(name) => {
                if let Some(local) = context.lookup(name) {
                    TypeProbe::Known(local.ty.clone())
                } else if let Some(Some(annotation)) = self.global_annotations.get(name) {
                    TypeProbe::Known(annotation.clone())
                } else if let Some(global) = self.hir_globals.get(name) {
                    TypeProbe::Known(global.ty.clone())
                } else if let Some(signature) = self.signatures.get(name) {
                    signature
                        .function_ty()
                        .map_or(TypeProbe::Unsupported, TypeProbe::Known)
                } else {
                    TypeProbe::Unsupported
                }
            }
            Expr::Borrow { value, .. } => self.probe_expr_ty(value, hint, context),
            Expr::Unary(UnaryOp::Neg, operand) => {
                self.probe_expr_ty(operand, hint.filter(|ty| ty.is_signed()), context)
            }
            Expr::Unary(UnaryOp::Not, _) => TypeProbe::Known(Ty::Bool),
            Expr::Binary(left, operator, right) => match operator {
                BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge => TypeProbe::Known(Ty::Bool),
                BinaryOp::Add => self.probe_add_ty(left, right, hint, context),
                BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                    self.probe_numeric_binary_ty(left, right, hint, context)
                }
            },
            Expr::Coalesce(left, right) => self.probe_coalesce_ty(left, right, hint, context),
            Expr::Try(value) => {
                let probe = self.probe_expr_ty(value, None, context);
                let Some(info) = self.builtin_fallible_info_for_probe(&probe) else {
                    return TypeProbe::Unsupported;
                };
                match info.payload_source {
                    Some(source) => TypeProbe::KnownSource(info.payload, source),
                    None => TypeProbe::Known(info.payload),
                }
            }
            Expr::Throw(_) => TypeProbe::Unsupported,
            Expr::Array(elements) => {
                if let Some(Ty::Array(element, length)) = hint {
                    if *length != elements.len() as u64 {
                        return TypeProbe::Unsupported;
                    }
                    return TypeProbe::Known(Ty::Array(element.clone(), *length));
                }
                let Some(first) = elements.first() else {
                    return TypeProbe::Unsupported;
                };
                let first = self.probe_expr_ty(first, None, context);
                let mut exact = match &first {
                    TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => Some(ty.clone()),
                    TypeProbe::Defaultable(_) => None,
                    TypeProbe::Unsupported => return TypeProbe::Unsupported,
                };
                let mut probes = vec![first];
                for item in elements.iter().skip(1) {
                    let probe = self.probe_expr_ty(item, exact.as_ref(), context);
                    match &probe {
                        TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => {
                            if exact.as_ref().is_some_and(|exact| exact != ty) {
                                return TypeProbe::Unsupported;
                            }
                            exact.get_or_insert_with(|| ty.clone());
                        }
                        TypeProbe::Defaultable(_) => {}
                        TypeProbe::Unsupported => return TypeProbe::Unsupported,
                    }
                    probes.push(probe);
                }
                if let Some(element) = exact {
                    if probes.iter().all(|probe| match probe {
                        TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => ty == &element,
                        TypeProbe::Defaultable(ty) => ty.is_integer() && element.is_integer(),
                        TypeProbe::Unsupported => false,
                    }) {
                        TypeProbe::Known(Ty::Array(Box::new(element), elements.len() as u64))
                    } else {
                        TypeProbe::Unsupported
                    }
                } else if probes
                    .iter()
                    .all(|probe| matches!(probe, TypeProbe::Defaultable(ty) if ty == &Ty::I32))
                {
                    TypeProbe::Defaultable(Ty::Array(Box::new(Ty::I32), elements.len() as u64))
                } else {
                    TypeProbe::Unsupported
                }
            }
            Expr::Index { base, .. } => match self.probe_expr_ty(base, None, context) {
                TypeProbe::Known(Ty::Array(element, _))
                | TypeProbe::KnownSource(Ty::Array(element, _), _) => TypeProbe::Known(*element),
                _ => TypeProbe::Unsupported,
            },
            Expr::Member(base, member) => {
                if let Some((NominalKind::Enum, ty, source)) =
                    self.probe_nominal_type_head(base, context)
                {
                    if matches!(
                        self.probe_enum_variant_fields(&source, member),
                        Some(VariantFields::Unit)
                    ) {
                        return TypeProbe::KnownSource(ty, source);
                    }
                }
                match self.probe_expr_ty(base, None, context) {
                    TypeProbe::Known(Ty::Struct(name))
                    | TypeProbe::KnownSource(Ty::Struct(name), _) => self
                        .struct_layouts
                        .get(&name)
                        .and_then(|layout| layout.fields.iter().find(|field| field.name == *member))
                        .map(|field| TypeProbe::Known(field.ty.clone()))
                        .unwrap_or(TypeProbe::Unsupported),
                    _ => TypeProbe::Unsupported,
                }
            }
            Expr::Call(_, _) => self.probe_call_ty(expression, context),
            Expr::Block(statements, tail) if statements.is_empty() => {
                tail.as_ref().map_or(TypeProbe::Known(Ty::Unit), |tail| {
                    self.probe_expr_ty(tail, hint, context)
                })
            }
            Expr::If {
                then_branch,
                else_branch: Some(else_branch),
                ..
            } => {
                let then_ty = self.probe_expr_ty(then_branch, hint, context);
                let else_ty = self.probe_expr_ty(else_branch, hint, context);
                if then_ty == else_ty {
                    then_ty
                } else {
                    match (then_ty, else_ty) {
                        (TypeProbe::Defaultable(default), exact)
                        | (exact, TypeProbe::Defaultable(default)) => match exact {
                            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _)
                                if default.is_integer() && ty.is_integer() =>
                            {
                                TypeProbe::Known(ty)
                            }
                            _ => TypeProbe::Unsupported,
                        },
                        (TypeProbe::Known(left), TypeProbe::KnownSource(right, source))
                        | (TypeProbe::KnownSource(right, source), TypeProbe::Known(left))
                            if left == right =>
                        {
                            TypeProbe::KnownSource(left, source)
                        }
                        _ => TypeProbe::Unsupported,
                    }
                }
            }
            Expr::Infer
            | Expr::Assign(_, _)
            | Expr::Block(_, _)
            | Expr::Closure(_, _)
            | Expr::If { .. }
            | Expr::Return(_)
            | Expr::While { .. }
            | Expr::Loop { .. }
            | Expr::Break(_)
            | Expr::Match { .. } => TypeProbe::Unsupported,
        }
    }

    fn probe_call_ty(&self, expression: &Expr, context: &LowerCtx) -> TypeProbe {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        if let Expr::Member(base, variant) = root {
            let Some((NominalKind::Enum, ty, source)) = self.probe_nominal_type_head(base, context)
            else {
                return TypeProbe::Unsupported;
            };
            let Some(fields) = self.probe_enum_variant_fields(&source, variant) else {
                return TypeProbe::Unsupported;
            };
            let valid = match fields {
                VariantFields::Unit => false,
                VariantFields::Positional(fields) => {
                    groups.len() == 1
                        && groups[0].len() == fields.len()
                        && groups[0].iter().all(|argument| argument.label.is_none())
                }
                VariantFields::Named(fields) => {
                    groups.len() == 1
                        && groups[0].len() == fields.len()
                        && groups[0].iter().all(|argument| {
                            argument.label.as_ref().is_some_and(|label| {
                                fields.iter().any(|field| field.name == *label)
                            })
                        })
                }
            };
            return if valid {
                TypeProbe::KnownSource(ty, source)
            } else {
                TypeProbe::Unsupported
            };
        }
        let Expr::Name(name) = root else {
            return TypeProbe::Unsupported;
        };
        if context.lookup(name).is_some() {
            return TypeProbe::Unsupported;
        }
        if let Some(template) = self.struct_templates.get(name) {
            let compile_group_count = template.compile_groups.len();
            if groups.len() == compile_group_count + 1 {
                let value_arguments = groups[compile_group_count];
                let labeled = value_arguments
                    .iter()
                    .filter(|argument| argument.label.is_some())
                    .count();
                let valid_fields = if labeled == 0 {
                    value_arguments.len() == template.fields.len()
                } else if labeled == value_arguments.len() {
                    value_arguments.len() == template.fields.len()
                        && value_arguments.iter().all(|argument| {
                            argument.label.as_ref().is_some_and(|label| {
                                template.fields.iter().any(|field| field.name == *label)
                            })
                        })
                } else {
                    false
                };
                if valid_fields {
                    if let Some((NominalKind::Struct, ty, source)) = self
                        .probe_generic_nominal_type_head(
                            name,
                            &groups[..compile_group_count],
                            context,
                        )
                    {
                        return TypeProbe::KnownSource(ty, source);
                    }
                }
            }
        }
        if let Some(template) = self.function_templates.get(name) {
            let compile_group_count = template.compile_groups.len();
            if groups.len() >= compile_group_count
                && groups.len() <= compile_group_count + template.groups.len()
            {
                let mut substitutions = HashMap::new();
                let mut valid = true;
                for (parameters, supplied) in template
                    .compile_groups
                    .iter()
                    .zip(groups.iter().take(compile_group_count))
                {
                    if parameters.len() != supplied.len()
                        || supplied.iter().any(|argument| argument.label.is_some())
                    {
                        valid = false;
                        break;
                    }
                    for (parameter, argument) in parameters.iter().zip(*supplied) {
                        let Some(source) = self.probe_type_argument_source(
                            &argument.value,
                            &context.type_substitutions,
                        ) else {
                            valid = false;
                            break;
                        };
                        if self.probe_source_ty(&source).is_none() {
                            valid = false;
                            break;
                        }
                        substitutions.insert(parameter.name.clone(), source);
                    }
                }
                let runtime_groups = &groups[compile_group_count..];
                valid &= runtime_groups
                    .iter()
                    .zip(&template.groups)
                    .all(|(arguments, parameters)| arguments.len() == parameters.len());
                if valid {
                    let Some(mut result_source) = template.return_type.clone() else {
                        return TypeProbe::Unsupported;
                    };
                    substitute_type_parameters(&mut result_source, &substitutions);
                    let Some(result) = self.probe_source_ty(&result_source) else {
                        return TypeProbe::Unsupported;
                    };
                    if runtime_groups.len() == template.groups.len() {
                        return TypeProbe::KnownSource(result, result_source);
                    }
                    let remaining = template.groups[runtime_groups.len()..]
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| {
                                    let mut source = parameter.ty.clone();
                                    substitute_type_parameters(&mut source, &substitutions);
                                    self.probe_source_ty(&source)
                                })
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>();
                    if let Some(groups) = remaining {
                        return TypeProbe::Known(Ty::Function(FunctionTy {
                            groups,
                            result: Box::new(result),
                        }));
                    }
                }
            }
        }
        if let Some(signature) = self.signatures.get(name) {
            if groups.len() > signature.groups.len()
                || groups
                    .iter()
                    .zip(&signature.groups)
                    .any(|(arguments, parameters)| arguments.len() != parameters.len())
            {
                return TypeProbe::Unsupported;
            }
            if groups.len() == signature.groups.len() {
                return signature
                    .result
                    .clone()
                    .map_or(TypeProbe::Unsupported, TypeProbe::Known);
            }
            let Some(result) = signature.result.clone() else {
                return TypeProbe::Unsupported;
            };
            return TypeProbe::Known(Ty::Function(FunctionTy {
                groups: signature.groups[groups.len()..]
                    .iter()
                    .map(|group| group.iter().map(|parameter| parameter.ty.clone()).collect())
                    .collect(),
                result: Box::new(result),
            }));
        }
        if self.struct_layouts.contains_key(name) && groups.len() == 1 {
            return TypeProbe::Known(Ty::Struct(name.clone()));
        }
        TypeProbe::Unsupported
    }

    fn seed_type_argument_inference(
        &mut self,
        owner: &str,
        compile_groups: &[Vec<CompileParam>],
        groups: &[&[CallArg]],
        type_substitutions: &HashMap<String, Type>,
    ) -> Option<(HashSet<String>, HashMap<String, InferredTypeArgument>)> {
        if groups.len() < compile_groups.len() {
            self.error(format!(
                "generic declaration `{owner}` requires all {} type argument groups",
                compile_groups.len()
            ));
            return None;
        }
        let compile_parameters: HashSet<_> = compile_groups
            .iter()
            .flatten()
            .map(|parameter| parameter.name.clone())
            .collect();
        let mut inferred = HashMap::new();
        for (group_index, (parameters, arguments)) in compile_groups
            .iter()
            .zip(groups.iter().take(compile_groups.len()))
            .enumerate()
        {
            if parameters.len() != arguments.len() {
                self.error(format!(
                    "type argument count mismatch in group {} of `{owner}`: expected {}, found {}",
                    group_index + 1,
                    parameters.len(),
                    arguments.len()
                ));
                return None;
            }
            for (parameter, argument) in parameters.iter().zip(*arguments) {
                if argument.label.is_some() {
                    self.error(format!(
                        "labeled arguments are not allowed in type argument groups of `{owner}`"
                    ));
                    return None;
                }
                if matches!(argument.value, Expr::Infer) {
                    continue;
                }
                let source = self.type_argument_from_expr(&argument.value, type_substitutions)?;
                let Some(ty) = self.probe_source_ty(&source) else {
                    self.error(format!(
                        "invalid explicit type argument for `{}` in `{owner}`",
                        parameter.name
                    ));
                    return None;
                };
                inferred.insert(
                    parameter.name.clone(),
                    InferredTypeArgument {
                        ty,
                        source: Some(source),
                        origin: "explicit type argument".to_owned(),
                    },
                );
            }
        }
        Some((compile_parameters, inferred))
    }

    fn finish_type_argument_inference(
        &mut self,
        owner: &str,
        ordered_parameters: &[CompileParam],
        inferred: &HashMap<String, InferredTypeArgument>,
        unsupported_argument: bool,
    ) -> Option<(Vec<Type>, Vec<Ty>)> {
        let unresolved: Vec<_> = ordered_parameters
            .iter()
            .filter(|parameter| !inferred.contains_key(&parameter.name))
            .map(|parameter| parameter.name.clone())
            .collect();
        if !unresolved.is_empty() {
            if unsupported_argument {
                self.error(format!(
                    "cannot infer type argument{} {} for `{owner}` from this argument expression; write explicit type arguments",
                    if unresolved.len() == 1 { "" } else { "s" },
                    unresolved
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else {
                self.error(format!(
                    "cannot infer type argument{} {} for `{owner}`; write explicit type arguments",
                    if unresolved.len() == 1 { "" } else { "s" },
                    unresolved
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            return None;
        }
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for parameter in ordered_parameters {
            let inferred = &inferred[&parameter.name];
            let Some(source) = inferred
                .source
                .clone()
                .or_else(|| self.source_type_for_ty(&inferred.ty))
            else {
                self.error(format!(
                    "cannot use inferred type `{}` for type parameter `{}` in `{owner}`",
                    inferred.ty, parameter.name
                ));
                return None;
            };
            source_arguments.push(source);
            arguments.push(inferred.ty.clone());
        }
        Some((source_arguments, arguments))
    }

    fn infer_from_expression_constraints(
        &mut self,
        constraints: &[(Type, Expr, String)],
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        context: &LowerCtx,
    ) -> Option<bool> {
        let mut pending: Vec<_> = (0..constraints.len()).collect();
        let unsupported = loop {
            let mut progress = false;
            let mut next = Vec::new();
            let mut defaultable = Vec::new();
            for index in pending {
                let (template, expression, origin) = &constraints[index];
                let hint = self.resolved_template_ty(template, compile_parameters, inferred);
                match self.probe_expr_ty(expression, hint.as_ref(), context) {
                    TypeProbe::Known(actual) => {
                        match self.unify_template_ty(
                            template,
                            &actual,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        ) {
                            Ok(changed) => progress |= changed,
                            Err(message) => {
                                self.error(message);
                                return None;
                            }
                        }
                    }
                    TypeProbe::KnownSource(actual, source) => {
                        match self.unify_template_ty(
                            template,
                            &actual,
                            Some(&source),
                            compile_parameters,
                            inferred,
                            origin,
                        ) {
                            Ok(changed) => progress |= changed,
                            Err(message) => {
                                self.error(message);
                                return None;
                            }
                        }
                    }
                    TypeProbe::Defaultable(actual) => defaultable.push((index, actual)),
                    TypeProbe::Unsupported => next.push(index),
                }
            }
            if progress {
                next.extend(defaultable.into_iter().map(|(index, _)| index));
                pending = next;
                continue;
            }
            let mut default_progress = false;
            for (index, actual) in defaultable {
                let (template, _, origin) = &constraints[index];
                match self.unify_template_ty(
                    template,
                    &actual,
                    None,
                    compile_parameters,
                    inferred,
                    origin,
                ) {
                    Ok(changed) => default_progress |= changed,
                    Err(message) => {
                        self.error(message);
                        return None;
                    }
                }
            }
            if next.is_empty() {
                break false;
            }
            if !default_progress {
                break true;
            }
            pending = next;
        };
        Some(unsupported)
    }

    fn ensure_function_instance(
        &mut self,
        template_name: &str,
        source_arguments: Vec<Type>,
        arguments: Vec<Ty>,
    ) -> Option<String> {
        let key = FunctionInstanceKey {
            template: template_name.to_owned(),
            arguments,
        };
        if let Some(canonical) = self.function_instance_names.get(&key) {
            let info = &self.function_instances[canonical];
            debug_assert_eq!(info.key, key);
            debug_assert_eq!(info.canonical, *canonical);
            return Some(canonical.clone());
        }
        if self.function_instances.len() >= MAX_FUNCTION_INSTANCES {
            self.error(format!(
                "generic function instance limit of {MAX_FUNCTION_INSTANCES} exceeded while instantiating `{template_name}`"
            ));
            return None;
        }

        let template = self.function_templates[template_name].clone();
        let compile_parameters: Vec<_> = template.compile_groups.iter().flatten().collect();
        if compile_parameters.len() != source_arguments.len() {
            self.error(format!(
                "internal error: invalid type argument count while instantiating `{template_name}`"
            ));
            return None;
        }
        let mut substitutions = HashMap::new();
        for (parameter, argument) in compile_parameters.iter().zip(source_arguments) {
            if substitutions
                .insert(parameter.name.clone(), argument)
                .is_some()
            {
                self.error(format!(
                    "duplicate compile-time parameter `{}` in generic function `{template_name}`",
                    parameter.name
                ));
                return None;
            }
        }

        let canonical = function_instance_name(&key);
        if let Some(existing) = self.function_instances.get(&canonical) {
            self.error(format!(
                "internal error: generic function instance name collision between `{}` and `{template_name}`",
                existing.key.template
            ));
            return None;
        }
        self.function_instance_names
            .insert(key.clone(), canonical.clone());
        self.function_instances.insert(
            canonical.clone(),
            FunctionInstanceInfo {
                key,
                canonical: canonical.clone(),
            },
        );
        self.function_type_substitutions
            .insert(canonical.clone(), substitutions.clone());

        let mut function = template;
        substitute_function_types(&mut function, &substitutions);
        function.name = canonical.clone();
        function.compile_groups.clear();
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
            .insert(canonical.clone(), FunctionSig { groups, result });
        self.functions.insert(canonical.clone(), function);
        self.function_order.push(canonical.clone());
        Some(canonical)
    }

    fn lower_return_value(
        &mut self,
        expression: &Expr,
        boundary: &ReturnBoundary,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let candidate = self.return_value_candidate(expression, boundary, context);
        let expected = match candidate {
            ReturnValueCandidate::Container => Some(&boundary.container),
            ReturnValueCandidate::Success => Some(&boundary.success),
            ReturnValueCandidate::Unknown
                if Self::unknown_return_value_prefers_success(expression) =>
            {
                Some(&boundary.success)
            }
            ReturnValueCandidate::Unknown => None,
        };
        let value = self.lower_expr(expression, expected, context);
        self.finish_return_value(value, boundary)
    }

    fn finish_return_value(&mut self, value: HirExpr, boundary: &ReturnBoundary) -> HirExpr {
        if matches!(value.ty, Ty::Never | Ty::Error) || value.ty == boundary.container {
            return value;
        }
        if value.ty == boundary.success {
            return self.construct_boundary_variant(boundary, true, Some(value));
        }
        self.error(format!(
            "return value must be `{}` or its success value `{}`, found `{}`",
            boundary.container, boundary.success, value.ty
        ));
        error_expr()
    }

    fn construct_boundary_variant(
        &mut self,
        boundary: &ReturnBoundary,
        success: bool,
        value: Option<HirExpr>,
    ) -> HirExpr {
        let Ty::Enum(enum_name) = &boundary.container else {
            self.error("internal error: non-enum return boundary");
            return error_expr();
        };
        let variant_name = match (boundary.kind, success) {
            (BuiltinFallibleKind::Option, true) => "Some",
            (BuiltinFallibleKind::Option, false) => "None",
            (BuiltinFallibleKind::Result, true) => "Ok",
            (BuiltinFallibleKind::Result, false) => "Err",
        };
        let Some(layout) = self.enum_layout_or_diagnostic(enum_name) else {
            return error_expr();
        };
        let Some(variant) = layout
            .variants
            .iter()
            .position(|variant| variant.name == variant_name)
        else {
            self.error(format!(
                "internal error: `{enum_name}` has no `{variant_name}` variant"
            ));
            return error_expr();
        };
        let fields = match value {
            Some(value) => vec![(0, value)],
            None => Vec::new(),
        };
        HirExpr {
            ty: boundary.container.clone(),
            kind: HirExprKind::ConstructEnum {
                name: enum_name.clone(),
                variant,
                fields,
            },
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
        if function.return_type.is_some() {
            context.return_boundary = signature
                .result
                .as_ref()
                .and_then(|result| self.return_boundary_for_ty(result));
        }
        context.type_substitutions = self
            .function_type_substitutions
            .get(name)
            .cloned()
            .unwrap_or_default();
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

        let boundary = context.return_boundary.clone();
        let lowered_body = if let Some(boundary) = &boundary {
            self.lower_return_value(body, boundary, &mut context)
        } else {
            self.lower_expr(body, signature.result.as_ref(), &mut context)
        };
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
            Expr::Infer => {
                self.error("`_` may only be used as a generic type argument");
                error_expr()
            }
            Expr::Array(elements) => self.lower_array_literal(elements, expected, context),
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
                } else if self.function_templates.contains_key(name) {
                    self.error(format!(
                        "generic function `{name}` requires explicit type argument groups"
                    ));
                    error_expr()
                } else if let Some((enum_name, variant)) =
                    self.resolve_short_variant(name, expected)
                {
                    if self
                        .enum_layouts
                        .get(&enum_name)
                        .and_then(|layout| layout.variants.get(variant))
                        .is_some_and(|variant| variant.fields.is_empty())
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
                if matches!(value.as_ref(), Expr::Index { .. }) {
                    self.error("borrowing an indexed array element is not supported yet");
                    return error_expr();
                }
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
            Expr::Coalesce(left, right) => self.lower_coalesce(left, right, expected, context),
            Expr::Try(value) => self.lower_try(value, expected, context),
            Expr::Throw(value) => self.lower_throw(value, context),
            Expr::Assign(place, value) => {
                if matches!(place.as_ref(), Expr::Index { .. }) {
                    self.error("indexed array assignment is not supported yet");
                    let _ = self.lower_expr(value, None, context);
                    return error_expr();
                }
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
            Expr::Member(base, field) => self.lower_member(base, field, expected, context),
            Expr::Index { base, index } => self.lower_index(base, index, context),
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
                let boundary = context.return_boundary.clone();
                let declared_result = context.declared_result.clone();
                let value = if let Some(boundary) = &boundary {
                    Some(Box::new(match value {
                        Some(value) => self.lower_return_value(value, boundary, context),
                        None => self.finish_return_value(
                            HirExpr {
                                ty: Ty::Unit,
                                kind: HirExprKind::Unit,
                            },
                            boundary,
                        ),
                    }))
                } else {
                    value.as_ref().map(|value| {
                        Box::new(self.lower_expr(value, declared_result.as_ref(), context))
                    })
                };
                let returned_ty = value.as_ref().map_or(Ty::Unit, |value| value.ty.clone());
                context.returned_types.push(returned_ty);
                context.flow.reachable = false;
                HirExpr {
                    ty: Ty::Never,
                    kind: HirExprKind::Return(value),
                }
            }
            Expr::While { condition, body } => self.lower_while(condition, body, context),
            Expr::Loop { body } => self.lower_loop(body, expected, context),
            Expr::Break(value) => self.lower_break(value.as_deref(), context),
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

    fn lower_array_literal(
        &mut self,
        elements: &[Expr],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let expected_array = match expected {
            Some(Ty::Array(element, length)) => Some((element.as_ref().clone(), *length)),
            Some(Ty::Error) => None,
            Some(other) => {
                self.error(format!(
                    "array literal cannot be used where `{other}` is expected"
                ));
                None
            }
            None => None,
        };

        if let Some((element_ty, length)) = expected_array {
            if elements.len() as u64 != length {
                self.error(format!(
                    "array literal length mismatch: expected {length}, found {}",
                    elements.len()
                ));
            }
            let elements = elements
                .iter()
                .map(|element| self.lower_expr(element, Some(&element_ty), context))
                .collect();
            return HirExpr {
                ty: Ty::Array(Box::new(element_ty), length),
                kind: HirExprKind::Array(elements),
            };
        }

        let Some((first, rest)) = elements.split_first() else {
            self.error("empty array literal requires an expected array type");
            return error_expr();
        };
        let first = self.lower_expr(first, None, context);
        let element_ty = first.ty.clone();
        if element_ty == Ty::Unit {
            self.error("array element type `()` is not supported in the first version");
        } else if !is_copy_type(&element_ty) {
            self.error(format!(
                "array element type `{element_ty}` must implement Copy in the first version"
            ));
        }
        let mut lowered = vec![first];
        lowered.extend(
            rest.iter()
                .map(|element| self.lower_expr(element, Some(&element_ty), context)),
        );
        HirExpr {
            ty: Ty::Array(Box::new(element_ty), elements.len() as u64),
            kind: HirExprKind::Array(lowered),
        }
    }

    fn lower_index(&mut self, base: &Expr, index: &Expr, context: &mut LowerCtx) -> HirExpr {
        let base = self.lower_expr(base, None, context);
        let Ty::Array(element, length) = &base.ty else {
            self.error(format!(
                "array index requires an array value, found `{}`",
                base.ty
            ));
            let _ = self.lower_expr(index, None, context);
            return error_expr();
        };
        let element_ty = element.as_ref().clone();
        let length = *length;
        let lowered_index = self.lower_expr(index, None, context);
        self.require_same_type(&lowered_index.ty, &Ty::I32, "array index");

        let index = match integer_literal_value(index) {
            Some(value) => {
                if value < 0 || u64::try_from(value).map_or(true, |value| value >= length) {
                    self.error(format!(
                        "array index {value} is out of bounds for length {length}"
                    ));
                    HirIndex::Static(0)
                } else {
                    HirIndex::Static(value as u64)
                }
            }
            None => HirIndex::Dynamic(Box::new(lowered_index)),
        };
        HirExpr {
            ty: element_ty,
            kind: HirExprKind::Index {
                base: Box::new(base),
                index,
                length,
            },
        }
    }

    fn lower_while(&mut self, condition: &Expr, body: &Expr, context: &mut LowerCtx) -> HirExpr {
        let entry_flow = context.flow.clone();
        let outer_locals = context.outer_local_ids();
        context.loops.push(LoopFrame {
            result_ty: Some(Ty::Unit),
            unit_only: true,
            scope_depth: context.scopes.len(),
            break_flows: Vec::new(),
        });

        let condition = self.lower_expr(condition, Some(&Ty::Bool), context);
        let condition_flow = context.flow.clone();
        let body = self.lower_expr(body, Some(&Ty::Unit), context);
        let backedge_flow = context.flow.clone();
        let frame = context.loops.pop().expect("while frame");

        if backedge_flow.reachable {
            self.reject_loop_carried_moves(&entry_flow, &backedge_flow, &outer_locals);
        }
        let mut exit_flows = frame.break_flows;
        exit_flows.push(condition_flow);
        if backedge_flow.reachable {
            exit_flows.push(backedge_flow);
        }
        context.flow = FlowState::join(&exit_flows);
        HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::While {
                condition: Box::new(condition),
                body: Box::new(body),
            },
        }
    }

    fn lower_loop(
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
        });
        let body = self.lower_expr(body, Some(&Ty::Unit), context);
        let backedge_flow = context.flow.clone();
        let frame = context.loops.pop().expect("loop frame");

        if backedge_flow.reachable {
            self.reject_loop_carried_moves(&entry_flow, &backedge_flow, &outer_locals);
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

    fn lower_break(&mut self, value: Option<&Expr>, context: &mut LowerCtx) -> HirExpr {
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

    fn reject_loop_carried_moves(
        &mut self,
        entry: &FlowState,
        backedge: &FlowState,
        outer_locals: &HashSet<LocalId>,
    ) {
        let mut reported = HashSet::new();
        for (place, status) in &backedge.moves {
            if !outer_locals.contains(&place.local)
                || entry.moves.get(place) == Some(status)
                || !reported.insert(place.local)
            {
                continue;
            }
            self.error(
                "move of an outer value may cross a loop backedge; reinitialize it before the next iteration or move it only on a break/return path",
            );
        }
    }

    fn lower_local_closure(
        &mut self,
        source_params: &[crate::ast::Param],
        body: &Expr,
        outer: &mut LowerCtx,
    ) -> HirExpr {
        let mut source_groups = vec![source_params];
        let mut body = body;
        while let Expr::Closure(params, nested_body) = body {
            source_groups.push(params);
            body = nested_body;
        }

        let mut bound: HashSet<String> = source_groups
            .iter()
            .flat_map(|group| group.iter().map(|param| param.name.clone()))
            .collect();
        let mut capture_uses = Vec::new();
        if !self.scan_simple_closure_captures(body, &mut bound, outer, &mut capture_uses) {
            return error_expr();
        }

        let function = format!("__closure.{}", self.next_closure);
        self.next_closure += 1;
        let mut context = LowerCtx::for_function(&function, None);
        context.type_substitutions = outer.type_substitutions.clone();
        let mut hir_params = Vec::new();
        let mut captures = Vec::new();

        let is_fn_once = capture_uses
            .iter()
            .any(|capture| capture.mode == ClosureCaptureMode::Move);
        let is_fn_mut = !is_fn_once
            && capture_uses
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
            match capture.mode {
                ClosureCaptureMode::Shared | ClosureCaptureMode::Mutable
                    if !matches!(local.ty, Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 | Ty::Bool) =>
                {
                    self.error(format!(
                        "closure capture `{name}` must be a Copy scalar for this capture mode"
                    ));
                    continue;
                }
                ClosureCaptureMode::Move if !matches!(local.ty, Ty::Struct(_) | Ty::Enum(_)) => {
                    self.error(format!(
                        "FnOnce move capture `{name}` must be a nominal root local for now"
                    ));
                    continue;
                }
                _ => {}
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
            let (parameter_mode, capability, mutable, value) = match capture.mode {
                ClosureCaptureMode::Shared => {
                    place.loan = self.acquire_loan(&place, LoanKind::Shared, true, outer);
                    (PassMode::Borrow, LocalCapability::SharedParam, false, None)
                }
                ClosureCaptureMode::Mutable => {
                    self.ensure_writable(&place);
                    place.loan = self.acquire_loan(&place, LoanKind::Mutable, true, outer);
                    (PassMode::MutBorrow, LocalCapability::MutParam, true, None)
                }
                ClosureCaptureMode::Move => {
                    let value = self.access_place(place.clone(), AccessKind::Move, outer);
                    (
                        PassMode::Move,
                        LocalCapability::Owned,
                        false,
                        Some(Box::new(value)),
                    )
                }
            };
            captures.push(ClosureCapture {
                place,
                mode: capture.mode,
                value,
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

        let mut groups = Vec::new();
        for source_group in source_groups {
            let mut group = Vec::new();
            for param in source_group {
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
                group.push(ParamSig {
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
            groups.push(group);
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
            groups: groups.clone(),
            result: result.clone(),
            captures,
            is_fn_mut,
            is_fn_once,
        };
        HirExpr {
            ty: Ty::Function(FunctionTy {
                groups: groups
                    .into_iter()
                    .map(|group| group.into_iter().map(|param| param.ty).collect())
                    .collect(),
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
            Expr::Unary(_, operand) | Expr::Try(operand) | Expr::Throw(operand) => {
                self.scan_simple_closure_captures(operand, bound, outer, captures)
            }
            Expr::Binary(left, _, right) | Expr::Coalesce(left, right) => {
                self.scan_simple_closure_captures(left, bound, outer, captures)
                    & self.scan_simple_closure_captures(right, bound, outer, captures)
            }
            Expr::Array(elements) => elements.iter().fold(true, |valid, element| {
                self.scan_simple_closure_captures(element, bound, outer, captures) & valid
            }),
            Expr::Index { base, index } => {
                self.scan_simple_closure_captures(base, bound, outer, captures)
                    & self.scan_simple_closure_captures(index, bound, outer, captures)
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
            Expr::Call(_, _) => {
                self.scan_direct_move_closure_call(expression, bound, outer, captures)
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
            Expr::While { condition, body } => {
                self.scan_simple_closure_captures(condition, bound, outer, captures)
                    & self.scan_simple_closure_captures(body, bound, outer, captures)
            }
            Expr::Loop { body } => self.scan_simple_closure_captures(body, bound, outer, captures),
            Expr::Break(value) => value.as_ref().is_none_or(|value| {
                self.scan_simple_closure_captures(value, bound, outer, captures)
            }),
            _ => {
                self.error(
                    "closure body form requires mutable or consuming capture analysis, which is not supported yet",
                );
                false
            }
        }
    }

    fn scan_direct_move_closure_call(
        &mut self,
        expression: &Expr,
        bound: &HashSet<String>,
        outer: &LowerCtx,
        captures: &mut Vec<ClosureCaptureUse>,
    ) -> bool {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(function) = root else {
            self.error("FnOnce capture requires a direct named-function call");
            return false;
        };
        let (signature, runtime_groups) = if let Some(signature) = self.signatures.get(function) {
            (signature.clone(), groups.as_slice())
        } else if self.function_templates.contains_key(function) {
            let Some((canonical, compile_group_count)) = self.resolve_generic_function_instance(
                function,
                &groups,
                &outer.type_substitutions,
            ) else {
                return false;
            };
            (
                self.signatures[&canonical].clone(),
                &groups[compile_group_count..],
            )
        } else {
            self.error(format!(
                "closure call `{function}` is not a top-level named function"
            ));
            return false;
        };
        if runtime_groups.len() != signature.groups.len() {
            self.error(format!(
                "named function `{function}` must be fully applied inside a closure"
            ));
            return false;
        }

        let mut valid = true;
        for (arguments, parameters) in runtime_groups.iter().zip(&signature.groups) {
            if arguments.len() != parameters.len() {
                self.error(format!(
                    "argument count mismatch in closure call to `{function}`"
                ));
                valid = false;
            }
            for (argument, parameter) in arguments.iter().zip(parameters) {
                match &argument.value {
                    Expr::Name(name) if bound.contains(name) => {}
                    Expr::Name(name) => {
                        if let Some(local) = outer.lookup(name) {
                            let mode = effective_pass_mode(parameter.mode, &parameter.ty);
                            if mode == PassMode::Move
                                && matches!(local.ty, Ty::Struct(_) | Ty::Enum(_))
                                && local.ty == parameter.ty
                            {
                                record_closure_capture(captures, name, ClosureCaptureMode::Move);
                            } else {
                                self.error(format!(
                                    "closure call capture `{name}` must be a nominal root passed to a move parameter"
                                ));
                                valid = false;
                            }
                        }
                    }
                    Expr::Unit | Expr::Integer(_) | Expr::Bool(_) => {}
                    _ => {
                        self.error(
                            "closure call arguments only support literals, closure parameters, or a nominal root move capture",
                        );
                        valid = false;
                    }
                }
            }
        }
        valid
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

    fn lower_member(
        &mut self,
        base: &Expr,
        member: &str,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if let Some((name, type_groups)) = self.inferred_generic_enum_type_head(base, context) {
            let Some(canonical) = self.resolve_inferred_generic_enum_instance(
                &name,
                &type_groups,
                member,
                &[],
                InferredEnumHints {
                    payload: None,
                    result: expected,
                },
                context,
            ) else {
                return error_expr();
            };
            return self.lower_nominal_type_member_value(&canonical, NominalKind::Enum, member);
        }
        match self.resolve_nominal_type_head(base, context) {
            Ok(Some((target, kind))) => {
                return self.lower_nominal_type_member_value(&target, kind, member);
            }
            Err(()) => return error_expr(),
            Ok(None) => {}
        }
        if let Expr::Name(target) = base {
            if context.lookup(target).is_none()
                && (self.struct_layouts.contains_key(target)
                    || self.enum_layouts.contains_key(target))
            {
                if let Some(canonical) = self
                    .inherent_members
                    .get(target)
                    .and_then(|members| members.constants.get(member))
                    .cloned()
                {
                    return HirExpr {
                        ty: self.global_type(&canonical),
                        kind: HirExprKind::Global(canonical),
                    };
                }
                if self
                    .inherent_members
                    .get(target)
                    .is_some_and(|members| members.functions.contains_key(member))
                {
                    self.error(format!(
                        "associated function `{target}.{member}` must be called"
                    ));
                    return error_expr();
                }
            }
            if context.lookup(target).is_none() {
                if let Some(layout) = self.enum_layouts.get(target).cloned() {
                    if let Some((variant, variant_layout)) = layout
                        .variants
                        .iter()
                        .enumerate()
                        .find(|(_, variant)| variant.name == member)
                    {
                        if !variant_layout.fields.is_empty() {
                            self.error(format!(
                                "variant `{target}.{member}` requires constructor arguments"
                            ));
                            return error_expr();
                        }
                        return HirExpr {
                            ty: Ty::Enum(target.clone()),
                            kind: HirExprKind::ConstructEnum {
                                name: target.clone(),
                                variant,
                                fields: Vec::new(),
                            },
                        };
                    }
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.methods.contains_key(member))
                    {
                        self.error(format!(
                            "inherent method `{target}.{member}` requires an instance receiver and must be called"
                        ));
                        return error_expr();
                    }
                    self.error(format!(
                        "unknown associated member or variant `{member}` on `{target}`"
                    ));
                    return error_expr();
                }
                if self.struct_layouts.contains_key(target) {
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.methods.contains_key(member))
                    {
                        self.error(format!(
                            "inherent method `{target}.{member}` requires an instance receiver and must be called"
                        ));
                        return error_expr();
                    }
                    self.error(format!(
                        "unknown associated member `{member}` on `{target}`"
                    ));
                    return error_expr();
                }
            }
        }

        if let Some(place) = self.lower_place_without_diagnostic(base, context) {
            if let Ty::Struct(target) | Ty::Enum(target) = &place.ty {
                if self
                    .inherent_members
                    .get(target)
                    .is_some_and(|members| members.methods.contains_key(member))
                {
                    self.error(format!(
                        "inherent method `{target}.{member}` must be called"
                    ));
                    return error_expr();
                }
                let has_field = self
                    .struct_layouts
                    .get(target)
                    .is_some_and(|layout| layout.fields.iter().any(|field| field.name == member));
                if !has_field {
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.functions.contains_key(member))
                    {
                        self.error(format!(
                            "associated function `{target}.{member}` must be called on the type"
                        ));
                        return error_expr();
                    }
                    if self
                        .inherent_members
                        .get(target)
                        .is_some_and(|members| members.constants.contains_key(member))
                    {
                        self.error(format!(
                            "associated constant `{target}.{member}` must be accessed on the type"
                        ));
                        return error_expr();
                    }
                }
            }
            let Ty::Struct(struct_name) = &place.ty else {
                self.error(format!(
                    "member access requires a struct value, found `{}`",
                    place.ty
                ));
                return error_expr();
            };
            let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
                return error_expr();
            };
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
        let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
            return error_expr();
        };
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

    fn lower_nominal_type_member_value(
        &mut self,
        target: &str,
        kind: NominalKind,
        member: &str,
    ) -> HirExpr {
        if let Some(canonical) = self
            .inherent_members
            .get(target)
            .and_then(|members| members.constants.get(member))
            .cloned()
        {
            return HirExpr {
                ty: self.global_type(&canonical),
                kind: HirExprKind::Global(canonical),
            };
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.functions.contains_key(member))
        {
            self.error(format!(
                "associated function `{target}.{member}` must be called"
            ));
            return error_expr();
        }
        if kind == NominalKind::Enum {
            let Some(layout) = self.enum_layout_or_diagnostic(target) else {
                return error_expr();
            };
            if let Some((variant, variant_layout)) = layout
                .variants
                .iter()
                .enumerate()
                .find(|(_, variant)| variant.name == member)
            {
                if !variant_layout.fields.is_empty() {
                    self.error(format!(
                        "variant `{target}.{member}` requires constructor arguments"
                    ));
                    return error_expr();
                }
                return HirExpr {
                    ty: Ty::Enum(target.to_owned()),
                    kind: HirExprKind::ConstructEnum {
                        name: target.to_owned(),
                        variant,
                        fields: Vec::new(),
                    },
                };
            }
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.methods.contains_key(member))
        {
            self.error(format!(
                "inherent method `{target}.{member}` requires an instance receiver and must be called"
            ));
        } else if kind == NominalKind::Enum {
            self.error(format!(
                "unknown associated member or variant `{member}` on `{target}`"
            ));
        } else {
            self.error(format!(
                "unknown associated member `{member}` on `{target}`"
            ));
        }
        error_expr()
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
        self.lower_match_with_scrutinee(scrutinee, arms, expected, context)
    }

    fn lower_coalesce(
        &mut self,
        left: &Expr,
        right: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let inferred_left = self.inferred_builtin_coalesce_lhs(left, context);
        let payload_hint = inferred_left.as_ref().and_then(|inferred| {
            match self.inferred_coalesce_payload_probe(
                inferred.kind,
                &inferred.type_groups,
                expected.filter(|ty| !matches!(ty, Ty::Never | Ty::Error)),
                right,
                context,
            ) {
                TypeProbe::Known(ty) | TypeProbe::Defaultable(ty) => Some(CoalescePayloadHint {
                    source: self.source_type_for_ty(&ty),
                    ty,
                }),
                TypeProbe::KnownSource(ty, source) => Some(CoalescePayloadHint {
                    ty,
                    source: Some(source),
                }),
                TypeProbe::Unsupported => None,
            }
        });
        let scrutinee = if let (Some(inferred), Some(hint)) = (inferred_left, payload_hint.as_ref())
        {
            let Some(canonical) = self.resolve_inferred_generic_enum_instance(
                &inferred.name,
                &inferred.type_groups,
                inferred.variant,
                &inferred.value_groups,
                InferredEnumHints {
                    payload: Some(hint),
                    result: None,
                },
                context,
            ) else {
                return error_expr();
            };
            if inferred.value_groups.is_empty() {
                self.lower_nominal_type_member_value(
                    &canonical,
                    NominalKind::Enum,
                    inferred.variant,
                )
            } else {
                self.lower_nominal_type_member_call(
                    &canonical,
                    NominalKind::Enum,
                    inferred.variant,
                    &inferred.value_groups,
                    context,
                )
            }
        } else {
            self.lower_expr(left, None, context)
        };
        if scrutinee.ty == Ty::Error {
            return error_expr();
        }
        let Some(info) = self.builtin_fallible_info_for_ty(&scrutinee.ty) else {
            self.error(format!(
                "operator `??` requires `Option(T)` or `Result(T, E)` on the left, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };

        const PAYLOAD_BINDING: &str = "$coalesce$payload";
        let payload_arm = |variant: &str| MatchArm {
            pattern: Pattern::Constructor {
                path: vec![variant.to_owned()],
                fields: PatternFields::Positional(vec![Pattern::Binding(
                    PAYLOAD_BINDING.to_owned(),
                )]),
            },
            guard: None,
            body: Expr::Name(PAYLOAD_BINDING.to_owned()),
        };
        let fallback_arm = |variant: &str, fields: PatternFields| MatchArm {
            pattern: Pattern::Constructor {
                path: vec![variant.to_owned()],
                fields,
            },
            guard: None,
            body: right.clone(),
        };
        let arms = match info.kind {
            BuiltinFallibleKind::Option => vec![
                payload_arm("Some"),
                fallback_arm("None", PatternFields::Unit),
            ],
            BuiltinFallibleKind::Result => vec![
                payload_arm("Ok"),
                fallback_arm("Err", PatternFields::Positional(vec![Pattern::Wildcard])),
            ],
        };
        self.lower_match_with_scrutinee(scrutinee, &arms, Some(&info.payload), context)
    }

    fn lower_try(
        &mut self,
        value: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let boundary = context.return_boundary.clone();
        let operand_expected = boundary.as_ref().and_then(|boundary| {
            self.inferred_try_operand_expected(value, expected, boundary, context)
        });
        let operand = self.lower_expr(value, operand_expected.as_ref(), context);
        let Some(boundary) = boundary else {
            self.error(
                "postfix `.try` requires a named function with an explicit `Option` or `Result` return type",
            );
            return error_expr();
        };
        if operand.ty == Ty::Error {
            return error_expr();
        }
        let Some(info) = self.builtin_fallible_info_for_ty(&operand.ty) else {
            self.error(format!(
                "postfix `.try` requires an `Option(T)` or `Result(T, E)` operand, found `{}`",
                operand.ty
            ));
            return error_expr();
        };
        if info.kind != boundary.kind {
            self.error(format!(
                "postfix `.try` cannot propagate `{}` through `{}`",
                operand.ty, boundary.container
            ));
            return error_expr();
        }
        if info.kind == BuiltinFallibleKind::Result && info.error != boundary.error {
            self.error(format!(
                "postfix `.try` requires the same `Result` error type as `{}`",
                boundary.container
            ));
            return error_expr();
        }

        let Ty::Enum(boundary_name) = &boundary.container else {
            self.error("internal error: non-enum `.try` return boundary");
            return error_expr();
        };
        const PAYLOAD_BINDING: &str = "$try$payload";
        const ERROR_BINDING: &str = "$try$error";
        let success_variant = match info.kind {
            BuiltinFallibleKind::Option => "Some",
            BuiltinFallibleKind::Result => "Ok",
        };
        let residual_arm = match info.kind {
            BuiltinFallibleKind::Option => MatchArm {
                pattern: Pattern::Constructor {
                    path: vec!["None".to_owned()],
                    fields: PatternFields::Unit,
                },
                guard: None,
                body: Expr::Return(Some(Box::new(Expr::Member(
                    Box::new(Expr::Name(boundary_name.clone())),
                    "None".to_owned(),
                )))),
            },
            BuiltinFallibleKind::Result => MatchArm {
                pattern: Pattern::Constructor {
                    path: vec!["Err".to_owned()],
                    fields: PatternFields::Positional(vec![Pattern::Binding(
                        ERROR_BINDING.to_owned(),
                    )]),
                },
                guard: None,
                body: Expr::Return(Some(Box::new(Expr::Call(
                    Box::new(Expr::Member(
                        Box::new(Expr::Name(boundary_name.clone())),
                        "Err".to_owned(),
                    )),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(ERROR_BINDING.to_owned()),
                    }],
                )))),
            },
        };
        let arms = vec![
            MatchArm {
                pattern: Pattern::Constructor {
                    path: vec![success_variant.to_owned()],
                    fields: PatternFields::Positional(vec![Pattern::Binding(
                        PAYLOAD_BINDING.to_owned(),
                    )]),
                },
                guard: None,
                body: Expr::Name(PAYLOAD_BINDING.to_owned()),
            },
            residual_arm,
        ];
        self.lower_match_with_scrutinee(operand, &arms, expected, context)
    }

    fn lower_throw(&mut self, value: &Expr, context: &mut LowerCtx) -> HirExpr {
        let boundary = context.return_boundary.clone();
        let Some(boundary) = boundary else {
            let _ = self.lower_expr(value, None, context);
            self.error("`throw` requires a named function with an explicit `Result` return type");
            return error_expr();
        };
        if boundary.kind != BuiltinFallibleKind::Result {
            let _ = self.lower_expr(value, None, context);
            self.error("`throw` may only propagate through a `Result` return type");
            return error_expr();
        }
        let error_ty = boundary
            .error
            .clone()
            .expect("Result return boundary has an error type");
        let error = self.lower_expr(value, Some(&error_ty), context);
        if error.ty == Ty::Never {
            return error;
        }
        let result = self.construct_boundary_variant(&boundary, false, Some(error));
        context.returned_types.push(boundary.container);
        context.flow.reachable = false;
        HirExpr {
            ty: Ty::Never,
            kind: HirExprKind::Return(Some(Box::new(result))),
        }
    }

    fn lower_match_with_scrutinee(
        &mut self,
        scrutinee: HirExpr,
        arms: &[MatchArm],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Ty::Enum(enum_name) = &scrutinee.ty else {
            self.error(format!(
                "match currently requires an enum value, found `{}`",
                scrutinee.ty
            ));
            return error_expr();
        };
        let Some(layout) = self.enum_layout_or_diagnostic(enum_name) else {
            return error_expr();
        };
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
                let source_name = self
                    .nominal_instances
                    .get(&layout.name)
                    .map(|instance| instance.key.template.as_str())
                    .unwrap_or(&layout.name);
                if path.len() > 2
                    || (path.len() == 2 && path[0] != layout.name && path[0] != source_name)
                {
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
                let source_name = self
                    .nominal_instances
                    .get(struct_name)
                    .map(|instance| instance.key.template.as_str())
                    .unwrap_or(struct_name);
                if constructor
                    .last()
                    .is_none_or(|name| name != struct_name && name != source_name)
                {
                    self.error(format!(
                        "pattern type mismatch: expected struct `{struct_name}`, found `{}`",
                        constructor.join(".")
                    ));
                    return;
                }
                let Some(layout) = self.struct_layout_or_diagnostic(struct_name) else {
                    return;
                };
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

    fn lower_trait_add(
        &mut self,
        left: &Expr,
        lowered_left: Option<HirExpr>,
        right: &Expr,
        receiver: &Ty,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some(schema) = self.traits.get("Add") else {
            self.error(format!(
                "operator `+` for `{receiver}` requires the top-level `Add` trait"
            ));
            return error_expr();
        };
        if !schema.valid {
            return error_expr();
        }

        let candidates = self.add_candidates(receiver, right, expected, context);
        let candidate = match candidates.as_slice() {
            [candidate] => candidate.clone(),
            [] => {
                let right_probe = self.probe_expr_ty(right, None, context);
                let right_ty = match right_probe {
                    TypeProbe::Known(ty)
                    | TypeProbe::KnownSource(ty, _)
                    | TypeProbe::Defaultable(ty) => Some(ty),
                    TypeProbe::Unsupported => None,
                };
                let expected_output = expected
                    .filter(|ty| **ty != Ty::Error)
                    .map(|ty| format!(" producing `{ty}`"))
                    .unwrap_or_default();
                if let Some(right_ty) = right_ty {
                    self.error(format!(
                        "no matching `Add` implementation for `{receiver}` with right operand `{right_ty}`{expected_output}"
                    ));
                } else {
                    self.error(format!(
                        "no matching `Add` implementation for `{receiver}` with an unresolved right operand{expected_output}"
                    ));
                }
                return error_expr();
            }
            [_, _, ..] => {
                let descriptions = candidates
                    .iter()
                    .map(|candidate| {
                        format!("`Add({}, Output = {})`", candidate.rhs, candidate.output)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                self.error(format!(
                    "ambiguous `+` for `{receiver}`; matching implementations: {descriptions}"
                ));
                return error_expr();
            }
        };

        let Some(signature) = self.signatures.get(&candidate.method).cloned() else {
            self.error("internal error: `Add.add` implementation has no function signature");
            return error_expr();
        };
        let valid_signature = signature.groups.len() == 2
            && signature.groups[0].len() == 1
            && signature.groups[1].len() == 1
            && signature.groups[0][0].ty == *receiver
            && signature.groups[0][0].mode == PassMode::Move
            && signature.groups[1][0].ty == candidate.rhs
            && signature.groups[1][0].mode == PassMode::Move
            && signature.result.as_ref() == Some(&candidate.output);
        if !valid_signature {
            self.error("internal error: invalid registered `Add.add` signature");
            return error_expr();
        }
        let receiver_parameter = signature.groups[0][0].clone();
        let rhs_parameter = signature.groups[1][0].clone();
        let mut temporary_loans = Vec::new();
        let left = if let Some(left) = lowered_left {
            self.require_same_type(
                &left.ty,
                &receiver_parameter.ty,
                "left operand of overloaded `+`",
            );
            HirArgument::Move(left)
        } else {
            self.lower_call_argument(left, &receiver_parameter, context, &mut temporary_loans)
        };
        let right = self.lower_call_argument(right, &rhs_parameter, context, &mut temporary_loans);
        self.release_loans(&temporary_loans, context);
        HirExpr {
            ty: candidate.output,
            kind: HirExprKind::Call {
                function: candidate.method,
                arguments: vec![left, right],
            },
        }
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
        if operator == Add {
            let left_probe = self.probe_expr_ty(left, None, context);
            if let Some(receiver) = Self::nominal_ty_from_probe(&left_probe) {
                return self.lower_trait_add(left, None, right, &receiver, expected, context);
            }
            if left_probe == TypeProbe::Unsupported {
                let lowered_left = self.lower_expr(left, None, context);
                if matches!(lowered_left.ty, Ty::Struct(_) | Ty::Enum(_)) {
                    let receiver = lowered_left.ty.clone();
                    return self.lower_trait_add(
                        left,
                        Some(lowered_left),
                        right,
                        &receiver,
                        expected,
                        context,
                    );
                }

                let right_hint = lowered_left.ty.is_integer().then_some(&lowered_left.ty);
                let lowered_right = self.lower_expr(right, right_hint, context);
                if !lowered_left.ty.is_integer() {
                    self.error(format!(
                        "operator `+` requires integer operands, found `{}`",
                        lowered_left.ty
                    ));
                }
                self.require_same_type(&lowered_left.ty, &lowered_right.ty, "operands of `+`");
                let ty = lowered_left.ty.clone();
                return HirExpr {
                    ty,
                    kind: HirExprKind::Binary(Box::new(lowered_left), Add, Box::new(lowered_right)),
                };
            }
        }
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
            if let Some(local) = context.lookup(name).cloned() {
                if local.closure.is_some() {
                    return self.lower_local_closure_call(name, &local, &groups, context);
                }
                if local.partial.is_some() {
                    return self.lower_local_partial_call(name, &local, &groups, context);
                }
                self.error(format!("local value `{name}` is not callable"));
                return error_expr();
            }
            if self.struct_layouts.contains_key(name) {
                return self.lower_struct_constructor(name, &groups, context);
            }
            if self.struct_templates.contains_key(name) {
                let compile_group_count = self.struct_templates[name].compile_groups.len();
                let has_inference = groups
                    .iter()
                    .take(compile_group_count)
                    .flat_map(|group| group.iter())
                    .any(|argument| matches!(argument.value, Expr::Infer));
                let resolved = if has_inference {
                    self.resolve_inferred_generic_struct_instance(name, &groups, expected, context)
                        .map(|canonical| (canonical, compile_group_count, NominalKind::Struct))
                } else {
                    self.resolve_generic_nominal_instance(
                        name,
                        &groups,
                        &context.type_substitutions,
                    )
                };
                let Some((canonical, compile_group_count, NominalKind::Struct)) = resolved else {
                    return error_expr();
                };
                return self.lower_struct_constructor(
                    &canonical,
                    &groups[compile_group_count..],
                    context,
                );
            }
            if self.enum_templates.contains_key(name) {
                self.error(format!(
                    "generic enum type `{name}` is not directly callable; select a variant"
                ));
                return error_expr();
            }
            if self.function_templates.contains_key(name) {
                return self.lower_generic_function_call(name, &groups, expected, context);
            }
            if self.functions.contains_key(name) {
                return self.lower_named_function_call(name, &groups, context);
            }
            if let Some((enum_name, variant)) = self.resolve_short_variant(name, expected) {
                return self.lower_enum_constructor(&enum_name, variant, &groups, context);
            }
            self.error(format!("`{name}` is not a function or constructor"));
            return error_expr();
        }
        if let Expr::Member(base, variant_name) = root {
            if let Some((name, type_groups)) = self.inferred_generic_enum_type_head(base, context) {
                let Some(canonical) = self.resolve_inferred_generic_enum_instance(
                    &name,
                    &type_groups,
                    variant_name,
                    &groups,
                    InferredEnumHints {
                        payload: None,
                        result: expected,
                    },
                    context,
                ) else {
                    return error_expr();
                };
                return self.lower_nominal_type_member_call(
                    &canonical,
                    NominalKind::Enum,
                    variant_name,
                    &groups,
                    context,
                );
            }
            match self.resolve_nominal_type_head(base, context) {
                Ok(Some((target, kind))) => {
                    return self.lower_nominal_type_member_call(
                        &target,
                        kind,
                        variant_name,
                        &groups,
                        context,
                    );
                }
                Err(()) => return error_expr(),
                Ok(None) => {}
            }
            if let Expr::Name(enum_name) = base.as_ref() {
                if context.lookup(enum_name).is_none()
                    && (self.struct_layouts.contains_key(enum_name)
                        || self.enum_layouts.contains_key(enum_name))
                {
                    if let Some(canonical) = self
                        .inherent_members
                        .get(enum_name)
                        .and_then(|members| members.functions.get(variant_name))
                        .cloned()
                    {
                        return self.lower_named_function_call(&canonical, &groups, context);
                    }
                    if self
                        .inherent_members
                        .get(enum_name)
                        .is_some_and(|members| members.constants.contains_key(variant_name))
                    {
                        self.error(format!(
                            "associated constant `{enum_name}.{variant_name}` is not callable"
                        ));
                        return error_expr();
                    }
                }
                if context.lookup(enum_name).is_none() {
                    if let Some(layout) = self.enum_layouts.get(enum_name) {
                        if let Some(variant) = layout
                            .variants
                            .iter()
                            .position(|variant| variant.name == *variant_name)
                        {
                            return self
                                .lower_enum_constructor(enum_name, variant, &groups, context);
                        }
                        if self
                            .inherent_members
                            .get(enum_name)
                            .is_some_and(|members| members.methods.contains_key(variant_name))
                        {
                            self.error(format!(
                                "inherent method `{enum_name}.{variant_name}` requires an instance receiver"
                            ));
                            return error_expr();
                        }
                        self.error(format!(
                            "unknown associated member or variant `{variant_name}` on `{enum_name}`"
                        ));
                        return error_expr();
                    }
                    if self.struct_layouts.contains_key(enum_name) {
                        if self
                            .inherent_members
                            .get(enum_name)
                            .is_some_and(|members| members.methods.contains_key(variant_name))
                        {
                            self.error(format!(
                                "inherent method `{enum_name}.{variant_name}` requires an instance receiver"
                            ));
                            return error_expr();
                        }
                        self.error(format!(
                            "unknown associated member `{variant_name}` on `{enum_name}`"
                        ));
                        return error_expr();
                    }
                }
            }
            return self.lower_bound_method_call(base, variant_name, &groups, context);
        }
        self.error("calls require a named function, constructor, associated function, or method");
        error_expr()
    }

    fn lower_nominal_type_member_call(
        &mut self,
        target: &str,
        kind: NominalKind,
        member: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if let Some(canonical) = self
            .inherent_members
            .get(target)
            .and_then(|members| members.functions.get(member))
            .cloned()
        {
            return self.lower_named_function_call(&canonical, groups, context);
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.constants.contains_key(member))
        {
            self.error(format!(
                "associated constant `{target}.{member}` is not callable"
            ));
            return error_expr();
        }
        if kind == NominalKind::Enum {
            let Some(layout) = self.enum_layout_or_diagnostic(target) else {
                return error_expr();
            };
            if let Some(variant) = layout
                .variants
                .iter()
                .position(|variant| variant.name == member)
            {
                return self.lower_enum_constructor(target, variant, groups, context);
            }
        }
        if self
            .inherent_members
            .get(target)
            .is_some_and(|members| members.methods.contains_key(member))
        {
            self.error(format!(
                "inherent method `{target}.{member}` requires an instance receiver"
            ));
        } else if kind == NominalKind::Enum {
            self.error(format!(
                "unknown associated member or variant `{member}` on `{target}`"
            ));
        } else {
            self.error(format!(
                "unknown associated member `{member}` on `{target}`"
            ));
        }
        error_expr()
    }

    fn lower_generic_function_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let compile_group_count = self.function_templates[name].compile_groups.len();
        let has_inference = groups
            .iter()
            .take(compile_group_count)
            .flat_map(|group| group.iter())
            .any(|argument| matches!(argument.value, Expr::Infer));
        let resolved = if has_inference {
            self.resolve_inferred_generic_function_instance(name, groups, expected, context)
        } else {
            self.resolve_generic_function_instance(name, groups, &context.type_substitutions)
        };
        let Some((canonical, compile_group_count)) = resolved else {
            return error_expr();
        };
        self.lower_named_function_call(&canonical, &groups[compile_group_count..], context)
    }

    fn inferred_generic_enum_type_head<'a>(
        &self,
        expression: &'a Expr,
        context: &LowerCtx,
    ) -> Option<(String, Vec<&'a [CallArg]>)> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(name) = root else {
            return None;
        };
        if context.lookup(name).is_some() || !self.enum_templates.contains_key(name) {
            return None;
        }
        let compile_group_count = self.enum_templates[name].compile_groups.len();
        if groups.len() > compile_group_count
            || !groups
                .iter()
                .flat_map(|group| group.iter())
                .any(|argument| matches!(argument.value, Expr::Infer))
        {
            return None;
        }
        Some((name.clone(), groups))
    }

    fn resolve_inferred_generic_enum_instance(
        &mut self,
        name: &str,
        type_groups: &[&[CallArg]],
        variant_name: &str,
        value_groups: &[&[CallArg]],
        hints: InferredEnumHints<'_>,
        context: &LowerCtx,
    ) -> Option<String> {
        let template = self.enum_templates[name].clone();
        let (compile_parameters, mut inferred) = self.seed_type_argument_inference(
            name,
            &template.compile_groups,
            type_groups,
            &context.type_substitutions,
        )?;
        if let Some(payload_hint) = hints.payload.filter(|hint| hint.ty != Ty::Error) {
            let Some(payload_parameter) = template.compile_groups.iter().flatten().next() else {
                self.error(format!(
                    "internal error: coalescing enum `{name}` has no payload type parameter"
                ));
                return None;
            };
            let payload_template = Type::Named(payload_parameter.name.clone(), Vec::new());
            if let Err(message) = self.unify_template_ty(
                &payload_template,
                &payload_hint.ty,
                payload_hint.source.as_ref(),
                &compile_parameters,
                &mut inferred,
                "payload type of `??`",
            ) {
                self.error(message);
                return None;
            }
        }
        if let Some(expected) = hints.result.filter(|ty| **ty != Ty::Error) {
            let result_template = Type::Named(
                name.to_owned(),
                template
                    .compile_groups
                    .iter()
                    .flatten()
                    .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
                    .collect(),
            );
            if let Err(message) = self.unify_template_ty(
                &result_template,
                expected,
                None,
                &compile_parameters,
                &mut inferred,
                "expected result type",
            ) {
                self.error(message);
                return None;
            }
        }

        let Some(variant) = template
            .variants
            .iter()
            .find(|variant| variant.name == variant_name)
            .cloned()
        else {
            self.error(format!(
                "unknown associated member or variant `{variant_name}` on `{name}`"
            ));
            return None;
        };
        let (fields, named) = match variant.fields {
            VariantFields::Unit => (Vec::new(), false),
            VariantFields::Positional(types) => (
                types
                    .into_iter()
                    .enumerate()
                    .map(|(index, ty)| (index.to_string(), ty))
                    .collect::<Vec<_>>(),
                false,
            ),
            VariantFields::Named(fields) => (
                fields
                    .into_iter()
                    .map(|field| (field.name, field.ty))
                    .collect::<Vec<_>>(),
                true,
            ),
        };
        if fields.is_empty() {
            if !value_groups.is_empty() {
                self.error(format!(
                    "unit variant `{name}.{variant_name}` is a value and must not be called"
                ));
                return None;
            }
        } else if value_groups.len() != 1 {
            self.error(format!(
                "enum variant constructor `{name}` expects exactly one argument group"
            ));
            return None;
        }

        let mut constraints = Vec::new();
        if let Some(arguments) = value_groups.first().copied() {
            let labeled = arguments
                .iter()
                .filter(|argument| argument.label.is_some())
                .count();
            if labeled != 0 && labeled != arguments.len() {
                self.error(format!(
                    "cannot mix labeled and positional arguments in variant `{name}.{variant_name}`"
                ));
                return None;
            }
            if labeled == 0 {
                if arguments.len() != fields.len() {
                    self.error(format!(
                        "argument count mismatch for variant `{name}.{variant_name}`: expected {}, found {}",
                        fields.len(),
                        arguments.len()
                    ));
                    return None;
                }
                for (argument, (field_name, field_ty)) in arguments.iter().zip(&fields) {
                    constraints.push((
                        field_ty.clone(),
                        argument.value.clone(),
                        format!("argument for variant field `{field_name}`"),
                    ));
                }
            } else {
                if !named {
                    self.error(format!(
                        "variant `{name}.{variant_name}` does not accept labeled arguments"
                    ));
                    return None;
                }
                let mut initialized = HashSet::new();
                for argument in arguments {
                    let label = argument
                        .label
                        .as_deref()
                        .expect("all arguments are labeled");
                    let Some((index, (_, field_ty))) = fields
                        .iter()
                        .enumerate()
                        .find(|(_, (field_name, _))| field_name == label)
                    else {
                        self.error(format!(
                            "unknown field `{label}` in variant `{name}.{variant_name}`"
                        ));
                        return None;
                    };
                    if !initialized.insert(index) {
                        self.error(format!(
                            "duplicate field `{label}` in variant `{name}.{variant_name}`"
                        ));
                        return None;
                    }
                    constraints.push((
                        field_ty.clone(),
                        argument.value.clone(),
                        format!("argument for variant field `{label}`"),
                    ));
                }
                if initialized.len() != fields.len() {
                    let missing = fields
                        .iter()
                        .enumerate()
                        .find(|(index, _)| !initialized.contains(index))
                        .map(|(_, (name, _))| name.as_str())
                        .unwrap_or("<unknown>");
                    self.error(format!(
                        "missing field `{missing}` in variant `{name}.{variant_name}`"
                    ));
                    return None;
                }
            }
        }

        let unsupported = self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            context,
        )?;
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) =
            self.finish_type_argument_inference(name, &ordered_parameters, &inferred, unsupported)?;
        self.ensure_nominal_instance(NominalKind::Enum, name, source_arguments, arguments)
    }

    fn resolve_inferred_generic_struct_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> Option<String> {
        let template = self.struct_templates[name].clone();
        let compile_group_count = template.compile_groups.len();
        let (compile_parameters, mut inferred) = self.seed_type_argument_inference(
            name,
            &template.compile_groups,
            groups,
            &context.type_substitutions,
        )?;
        let value_groups = &groups[compile_group_count..];
        if value_groups.len() != 1 {
            self.error(format!(
                "struct constructor `{name}` expects exactly one argument group"
            ));
            return None;
        }

        if let Some(expected) = expected.filter(|ty| **ty != Ty::Error) {
            let result_template = Type::Named(
                name.to_owned(),
                template
                    .compile_groups
                    .iter()
                    .flatten()
                    .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
                    .collect(),
            );
            if let Err(message) = self.unify_template_ty(
                &result_template,
                expected,
                None,
                &compile_parameters,
                &mut inferred,
                "expected result type",
            ) {
                self.error(message);
                return None;
            }
        }

        let arguments = value_groups[0];
        let labeled = arguments
            .iter()
            .filter(|argument| argument.label.is_some())
            .count();
        if labeled != 0 && labeled != arguments.len() {
            self.error(format!(
                "cannot mix labeled and positional arguments in struct `{name}`"
            ));
            return None;
        }
        let mut constraints = Vec::new();
        if labeled == 0 {
            if arguments.len() != template.fields.len() {
                self.error(format!(
                    "argument count mismatch for struct `{name}`: expected {}, found {}",
                    template.fields.len(),
                    arguments.len()
                ));
                return None;
            }
            for (argument, field) in arguments.iter().zip(&template.fields) {
                constraints.push((
                    field.ty.clone(),
                    argument.value.clone(),
                    format!("argument for field `{}`", field.name),
                ));
            }
        } else {
            let mut initialized = HashSet::new();
            for argument in arguments {
                let label = argument
                    .label
                    .as_deref()
                    .expect("all arguments are labeled");
                let Some((index, field)) = template
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(_, field)| field.name == label)
                else {
                    self.error(format!("unknown field `{label}` in struct `{name}`"));
                    return None;
                };
                if !initialized.insert(index) {
                    self.error(format!("duplicate field `{label}` in struct `{name}`"));
                    return None;
                }
                constraints.push((
                    field.ty.clone(),
                    argument.value.clone(),
                    format!("argument for field `{label}`"),
                ));
            }
            if initialized.len() != template.fields.len() {
                let missing = template
                    .fields
                    .iter()
                    .enumerate()
                    .find(|(index, _)| !initialized.contains(index))
                    .map(|(_, field)| field.name.as_str())
                    .unwrap_or("<unknown>");
                self.error(format!("missing field `{missing}` in struct `{name}`"));
                return None;
            }
        }
        let unsupported = self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            context,
        )?;
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) =
            self.finish_type_argument_inference(name, &ordered_parameters, &inferred, unsupported)?;
        self.ensure_nominal_instance(NominalKind::Struct, name, source_arguments, arguments)
    }

    fn resolve_generic_nominal_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        substitutions: &HashMap<String, Type>,
    ) -> Option<(String, usize, NominalKind)> {
        let (kind, compile_groups) = if let Some(template) = self.struct_templates.get(name) {
            (NominalKind::Struct, template.compile_groups.clone())
        } else if let Some(template) = self.enum_templates.get(name) {
            (NominalKind::Enum, template.compile_groups.clone())
        } else {
            self.error(format!("unknown generic nominal type `{name}`"));
            return None;
        };
        let compile_group_count = compile_groups.len();
        if groups.len() < compile_group_count {
            self.error(format!(
                "generic type `{name}` requires all {compile_group_count} type argument groups"
            ));
            return None;
        }

        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for (group_index, (source_group, argument_group)) in compile_groups
            .iter()
            .zip(groups.iter().take(compile_group_count))
            .enumerate()
        {
            if source_group.len() != argument_group.len() {
                self.error(format!(
                    "type argument count mismatch in group {} of `{name}`: expected {}, found {}",
                    group_index + 1,
                    source_group.len(),
                    argument_group.len()
                ));
                return None;
            }
            for argument in *argument_group {
                if argument.label.is_some() {
                    self.error(format!(
                        "labeled arguments are not allowed in type argument groups of `{name}`"
                    ));
                    return None;
                }
                let source_ty = self.type_argument_from_expr(&argument.value, substitutions)?;
                let ty = self.lower_source_type(&source_ty);
                if ty == Ty::Error {
                    return None;
                }
                source_arguments.push(source_ty);
                arguments.push(ty);
            }
        }
        let canonical = self.ensure_nominal_instance(kind, name, source_arguments, arguments)?;
        Some((canonical, compile_group_count, kind))
    }

    fn resolve_nominal_type_head(
        &mut self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Result<Option<(String, NominalKind)>, ()> {
        match expression {
            Expr::Name(name) if context.lookup(name).is_none() => {
                if self.struct_layouts.contains_key(name) {
                    Ok(Some((name.clone(), NominalKind::Struct)))
                } else if self.enum_layouts.contains_key(name) {
                    Ok(Some((name.clone(), NominalKind::Enum)))
                } else if self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
                {
                    self.error(format!(
                        "generic type `{name}` requires explicit type arguments"
                    ));
                    Err(())
                } else {
                    Ok(None)
                }
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    return Ok(None);
                };
                if context.lookup(name).is_some()
                    || (!self.struct_templates.contains_key(name)
                        && !self.enum_templates.contains_key(name))
                {
                    return Ok(None);
                }
                let expected_groups = if let Some(template) = self.struct_templates.get(name) {
                    template.compile_groups.len()
                } else {
                    self.enum_templates[name].compile_groups.len()
                };
                if groups.len() > expected_groups {
                    return Ok(None);
                }
                let Some((canonical, consumed, kind)) = self.resolve_generic_nominal_instance(
                    name,
                    &groups,
                    &context.type_substitutions,
                ) else {
                    return Err(());
                };
                if consumed != groups.len() {
                    self.error(format!(
                        "generic type head `{name}` is missing type argument groups"
                    ));
                    return Err(());
                }
                Ok(Some((canonical, kind)))
            }
            _ => Ok(None),
        }
    }

    fn resolve_inferred_generic_function_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> Option<(String, usize)> {
        let template = self.function_templates[name].clone();
        let compile_group_count = template.compile_groups.len();
        let (compile_parameters, mut inferred) = self.seed_type_argument_inference(
            name,
            &template.compile_groups,
            groups,
            &context.type_substitutions,
        )?;
        let runtime_groups = &groups[compile_group_count..];
        if runtime_groups.len() > template.groups.len() {
            self.error(format!(
                "too many parameter groups in call to `{name}`: expected at most {}, found {}",
                template.groups.len(),
                runtime_groups.len()
            ));
            return None;
        }
        for (group_index, (arguments, parameters)) in
            runtime_groups.iter().zip(&template.groups).enumerate()
        {
            if arguments.len() != parameters.len() {
                self.error(format!(
                    "argument count mismatch in group {} of `{name}`: expected {}, found {}",
                    group_index + 1,
                    parameters.len(),
                    arguments.len()
                ));
                return None;
            }
        }

        if runtime_groups.len() == template.groups.len() {
            if let (Some(expected), Some(result)) = (expected, template.return_type.as_ref()) {
                if *expected != Ty::Error {
                    if let Err(message) = self.unify_template_ty(
                        result,
                        expected,
                        None,
                        &compile_parameters,
                        &mut inferred,
                        "expected result type",
                    ) {
                        self.error(message);
                        return None;
                    }
                }
            }
        }

        let constraints: Vec<_> = runtime_groups
            .iter()
            .zip(&template.groups)
            .enumerate()
            .flat_map(|(group_index, (arguments, parameters))| {
                arguments
                    .iter()
                    .zip(parameters)
                    .map(move |(argument, parameter)| {
                        (
                            parameter.ty.clone(),
                            argument.value.clone(),
                            format!(
                                "argument for parameter `{}` in group {}",
                                parameter.name,
                                group_index + 1
                            ),
                        )
                    })
            })
            .collect();
        let unsupported_argument = self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            context,
        )?;

        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) = self.finish_type_argument_inference(
            name,
            &ordered_parameters,
            &inferred,
            unsupported_argument,
        )?;
        let canonical = self.ensure_function_instance(name, source_arguments, arguments)?;
        Some((canonical, compile_group_count))
    }

    fn resolve_generic_function_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        substitutions: &HashMap<String, Type>,
    ) -> Option<(String, usize)> {
        let template = self.function_templates[name].clone();
        let compile_group_count = template.compile_groups.len();
        if groups.len() < compile_group_count {
            self.error(format!(
                "generic function `{name}` requires all {compile_group_count} type argument groups"
            ));
            return None;
        }

        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for (group_index, (source_group, argument_group)) in template
            .compile_groups
            .iter()
            .zip(groups.iter().take(compile_group_count))
            .enumerate()
        {
            if source_group.len() != argument_group.len() {
                self.error(format!(
                    "type argument count mismatch in group {} of `{name}`: expected {}, found {}",
                    group_index + 1,
                    source_group.len(),
                    argument_group.len()
                ));
                return None;
            }
            for argument in *argument_group {
                if argument.label.is_some() {
                    self.error(format!(
                        "labeled arguments are not allowed in type argument groups of `{name}`"
                    ));
                    return None;
                }
                let source_ty = self.type_argument_from_expr(&argument.value, substitutions)?;
                let ty = self.lower_source_type(&source_ty);
                if ty == Ty::Error {
                    return None;
                }
                source_arguments.push(source_ty);
                arguments.push(ty);
            }
        }

        let canonical = self.ensure_function_instance(name, source_arguments, arguments)?;
        Some((canonical, compile_group_count))
    }

    fn lower_bound_method_call(
        &mut self,
        receiver: &Expr,
        member: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some(receiver_place) = self.lower_place_without_diagnostic(receiver, context) else {
            self.error(format!(
                "temporary receiver for method `{member}` is not supported yet; bind it to a local first"
            ));
            return error_expr();
        };
        let target = match &receiver_place.ty {
            Ty::Struct(name) | Ty::Enum(name) => name.clone(),
            ty => {
                self.error(format!(
                    "method call requires a nominal receiver, found `{ty}`"
                ));
                return error_expr();
            }
        };
        let inherent = self
            .inherent_members
            .get(&target)
            .and_then(|members| members.methods.get(member))
            .cloned();
        let canonical = if let Some(canonical) = inherent {
            canonical
        } else {
            let candidates = self
                .trait_methods_by_receiver
                .get(&(receiver_place.ty.clone(), member.to_owned()))
                .cloned()
                .unwrap_or_default();
            if candidates.len() == 1 {
                let implementation = &self.trait_impls[&candidates[0]];
                debug_assert_eq!(implementation.key, candidates[0]);
                debug_assert!(implementation
                    .associated_types
                    .values()
                    .all(|ty| *ty != Ty::Error));
                implementation.methods[member].clone()
            } else if candidates.len() > 1 {
                let mut traits = candidates
                    .iter()
                    .map(|candidate| candidate.trait_ref.name.clone())
                    .collect::<Vec<_>>();
                traits.sort();
                traits.dedup();
                self.error(format!(
                    "ambiguous trait method `{member}` on `{target}`; candidates: {}",
                    traits
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                return error_expr();
            } else {
                if self
                    .inherent_members
                    .get(&target)
                    .is_some_and(|members| members.functions.contains_key(member))
                {
                    self.error(format!(
                        "associated function `{target}.{member}` must be called on the type"
                    ));
                } else if self
                    .inherent_members
                    .get(&target)
                    .is_some_and(|members| members.constants.contains_key(member))
                {
                    self.error(format!(
                        "associated constant `{target}.{member}` must be accessed on the type"
                    ));
                } else {
                    self.error(format!("unknown method `{member}` on `{target}`"));
                }
                return error_expr();
            }
        };

        let function_ty = self.function_type(&canonical);
        let Ty::Function(function_ty) = function_ty else {
            return error_expr();
        };
        let signature = self.signatures[&canonical].clone();
        let Some(receiver_parameter) = signature.groups.first().and_then(|group| group.first())
        else {
            self.error(format!(
                "internal error: method `{target}.{member}` has no receiver parameter"
            ));
            return error_expr();
        };
        let consumed_groups = groups.len() + 1;
        if consumed_groups > signature.groups.len() {
            self.error(format!(
                "too many parameter groups in method call `{target}.{member}`: expected {}, found {}",
                signature.groups.len() - 1,
                groups.len()
            ));
            return error_expr();
        }

        let mut temporary_loans = Vec::new();
        let mut arguments = vec![self.lower_call_argument(
            receiver,
            receiver_parameter,
            context,
            &mut temporary_loans,
        )];
        for (relative_group, arguments_ast) in groups.iter().enumerate() {
            let group_index = relative_group + 1;
            let params = &signature.groups[group_index];
            if arguments_ast.len() != params.len() {
                self.error(format!(
                    "argument count mismatch in group {} of method `{target}.{member}`: expected {}, found {}",
                    relative_group + 1,
                    params.len(),
                    arguments_ast.len()
                ));
            }
            for (argument, parameter) in arguments_ast.iter().zip(params) {
                if argument.label.is_some() {
                    self.error(format!(
                        "named arguments are not allowed in call to method `{target}.{member}`"
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

        let complete = consumed_groups == signature.groups.len();
        if !complete
            && arguments
                .iter()
                .any(|argument| !matches!(argument, HirArgument::Copy(_)))
        {
            self.error(format!(
                "partial application of bound method `{target}.{member}` may only capture Copy arguments"
            ));
        }
        self.release_loans(&temporary_loans, context);
        if complete {
            HirExpr {
                ty: (*function_ty.result).clone(),
                kind: HirExprKind::Call {
                    function: canonical,
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
                    function: canonical,
                    consumed_groups,
                    captures: arguments,
                },
            }
        }
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
        if groups.len() < closure.groups.len() {
            self.error(format!(
                "curried closures require all {} parameter groups in one call; partial application of closure `{local_name}` is not supported",
                closure.groups.len()
            ));
            return error_expr();
        }
        if groups.len() > closure.groups.len() {
            self.error(format!(
                "too many parameter groups in call to closure `{local_name}`: expected {}, found {}",
                closure.groups.len(),
                groups.len()
            ));
            return error_expr();
        }
        for (index, (arguments, parameters)) in groups.iter().zip(&closure.groups).enumerate() {
            if arguments.len() != parameters.len() {
                self.error(format!(
                    "argument count mismatch in group {} of closure `{local_name}`: expected {}, found {}",
                    index + 1,
                    parameters.len(),
                    arguments.len()
                ));
            }
        }

        if closure.is_fn_once {
            let callable = HirPlace {
                local: local.id,
                root_ty: local.ty.clone(),
                projections: Vec::new(),
                ty: local.ty.clone(),
                capability: LocalCapability::Owned,
                root_mutable: local.mutable,
                loan: None,
            };
            let key = PlaceKey::from(&callable);
            if let Some(status) = context
                .flow
                .moves
                .iter()
                .find_map(|(moved, status)| places_overlap(&key, moved).then_some(*status))
            {
                self.error(match status {
                    MoveStatus::Moved => {
                        format!("FnOnce closure `{local_name}` was already consumed")
                    }
                    MoveStatus::MaybeMoved => {
                        format!("FnOnce closure `{local_name}` may already be consumed")
                    }
                });
            } else {
                self.mark_moved(&callable, context);
            }
        }

        let mut lowered_arguments: Vec<_> = closure
            .captures
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, capture)| match capture.mode {
                ClosureCaptureMode::Shared => HirArgument::SharedBorrow(capture.place),
                ClosureCaptureMode::Mutable => HirArgument::MutBorrow(capture.place),
                ClosureCaptureMode::Move => HirArgument::Move(HirExpr {
                    ty: capture.place.ty,
                    kind: HirExprKind::PartialCapture {
                        binding: local.id,
                        index,
                    },
                }),
            })
            .collect();
        let mut temporary_loans = Vec::new();
        for (argument_group, parameters) in groups.iter().zip(&closure.groups) {
            for (argument, parameter) in argument_group.iter().zip(parameters) {
                if argument.label.is_some() {
                    self.error(format!(
                        "named arguments are not allowed in call to closure `{local_name}`"
                    ));
                }
                lowered_arguments.push(self.lower_call_argument(
                    &argument.value,
                    parameter,
                    context,
                    &mut temporary_loans,
                ));
            }
        }
        self.release_loans(&temporary_loans, context);
        HirExpr {
            ty: closure.result.clone(),
            kind: HirExprKind::Call {
                function: closure.function.clone(),
                arguments: lowered_arguments,
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
        let Some(layout) = self.struct_layout_or_diagnostic(name) else {
            return error_expr();
        };
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
        let Some(layout) = self.enum_layout_or_diagnostic(enum_name) else {
            return error_expr();
        };
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
            let layout = self.enum_layout_or_diagnostic(enum_name)?;
            if let Some(index) = layout
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
                let is_non_generic = self
                    .nominal_instances
                    .get(enum_name)
                    .is_some_and(|instance| instance.key.arguments.is_empty());
                if !is_non_generic {
                    return None;
                }
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
        match mode {
            ClosureCaptureMode::Move => capture.mode = mode,
            ClosureCaptureMode::Mutable if capture.mode == ClosureCaptureMode::Shared => {
                capture.mode = mode;
            }
            ClosureCaptureMode::Shared | ClosureCaptureMode::Mutable => {}
        }
    } else {
        captures.push(ClosureCaptureUse {
            name: name.to_owned(),
            mode,
        });
    }
}

fn is_copy_type(ty: &Ty) -> bool {
    match ty {
        Ty::I32 | Ty::I64 | Ty::U32 | Ty::U64 | Ty::Bool | Ty::Unit | Ty::Never | Ty::Error => true,
        Ty::Array(element, _) => is_copy_type(element),
        Ty::Struct(_) | Ty::Enum(_) | Ty::Function(_) => false,
    }
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
        Ty::Array(element, _) => nominal_name(element),
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

fn integer_literal_value(expression: &Expr) -> Option<i128> {
    match expression {
        Expr::Integer(value) => Some(*value),
        Expr::Unary(UnaryOp::Neg, operand) => {
            let Expr::Integer(value) = operand.as_ref() else {
                return None;
            };
            value.checked_neg()
        }
        Expr::Block(statements, Some(tail)) if statements.is_empty() => integer_literal_value(tail),
        _ => None,
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
            HirExprKind::Array(elements) => Some(ConstValue::Aggregate(
                elements
                    .iter()
                    .map(|element| self.evaluate_expr(element, locals))
                    .collect::<Option<Vec<_>>>()?,
            )),
            HirExprKind::Index { base, index, .. } => {
                let ConstValue::Aggregate(elements) = self.evaluate_expr(base, locals)? else {
                    self.error("invalid array value in constant expression");
                    return None;
                };
                let index = match index {
                    HirIndex::Static(index) => i128::from(*index),
                    HirIndex::Dynamic(index) => {
                        let ConstValue::Integer(index) = self.evaluate_expr(index, locals)? else {
                            self.error("invalid array index in constant expression");
                            return None;
                        };
                        index
                    }
                };
                let Ok(index) = usize::try_from(index) else {
                    self.error("array index is out of bounds in constant expression");
                    return None;
                };
                elements.get(index).cloned().or_else(|| {
                    self.error("array index is out of bounds in constant expression");
                    None
                })
            }
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
            | HirExprKind::While { .. }
            | HirExprKind::Loop { .. }
            | HirExprKind::Break(_)
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
        output.push_str(
            "; ModuleID = 'salicin'\nsource_filename = \"salicin\"\n\ndeclare void @llvm.trap()\n\n",
        );

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

#[derive(Clone)]
struct EmitLoopTarget {
    break_label: String,
    result: Option<(Ty, String)>,
}

struct FunctionEmitter<'a> {
    function: &'a HirFunction,
    program: &'a HirProgram,
    output: String,
    next_register: usize,
    next_label: usize,
    locals: HashMap<LocalId, String>,
    partial_captures: HashMap<LocalId, Vec<Option<(Ty, String)>>>,
    entry_allocas: String,
    loops: Vec<EmitLoopTarget>,
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
            entry_allocas: String::new(),
            loops: Vec::new(),
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
        let entry_alloca_offset = self.output.len();

        for (index, parameter) in self.function.params.iter().enumerate() {
            if parameter.ty == Ty::Unit {
                continue;
            }
            if matches!(parameter.mode, PassMode::Borrow | PassMode::MutBorrow) {
                self.locals.insert(parameter.id, format!("%arg.{index}"));
                continue;
            }
            let ty = llvm_value_type(&parameter.ty)?;
            let pointer = self.entry_alloca(&ty, &llvm_comment(&parameter.name));
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
        self.output
            .insert_str(entry_alloca_offset, &self.entry_allocas);
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
            HirExprKind::Array(elements) => {
                let aggregate_ty = llvm_value_type(&expression.ty)?;
                let mut aggregate = "zeroinitializer".to_owned();
                for (index, element) in elements.iter().enumerate() {
                    let element = self.emit_expr(element)?;
                    if self.terminated {
                        return Ok(Operand::never());
                    }
                    let register = self.fresh_register();
                    self.instruction(format!(
                        "{register} = insertvalue {aggregate_ty} {aggregate}, {} {}, {index}",
                        llvm_value_type(&element.ty)?,
                        element.value()?
                    ));
                    aggregate = register;
                }
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(aggregate),
                })
            }
            HirExprKind::Index {
                base,
                index,
                length,
            } => self.emit_index(expression, base, index, *length),
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
                            if let HirExprKind::LocalClosure(closure) = &binding.value.kind {
                                let mut stored = Vec::new();
                                for capture in &closure.captures {
                                    if capture.mode != ClosureCaptureMode::Move {
                                        stored.push(None);
                                        continue;
                                    }
                                    let value = self.emit_expr(
                                        capture.value.as_deref().ok_or_else(|| {
                                            Diagnostic::new(
                                                "internal error: move closure capture has no value",
                                            )
                                        })?,
                                    )?;
                                    if self.terminated {
                                        break;
                                    }
                                    let ty = llvm_value_type(&value.ty)?;
                                    let pointer = self.entry_alloca(&ty, "closure capture");
                                    self.instruction(format!(
                                        "store {ty} {}, ptr {pointer}",
                                        value.value()?
                                    ));
                                    stored.push(Some((value.ty, pointer)));
                                }
                                if !self.terminated {
                                    self.partial_captures.insert(binding.id, stored);
                                }
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
                                    let ty = llvm_value_type(&capture.ty)?;
                                    let pointer = self.entry_alloca(&ty, "partial capture");
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
                            let ty = llvm_value_type(&binding.ty)?;
                            let pointer = self.entry_alloca(&ty, &llvm_comment(&binding.name));
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
            HirExprKind::While { condition, body } => self.emit_while(condition, body),
            HirExprKind::Loop { body } => self.emit_loop(expression, body),
            HirExprKind::Break(value) => self.emit_break(value.as_deref()),
            HirExprKind::Match { scrutinee, arms } => self.emit_match(expression, scrutinee, arms),
        }
    }

    fn emit_index(
        &mut self,
        expression: &HirExpr,
        base: &HirExpr,
        index: &HirIndex,
        length: u64,
    ) -> Result<Operand, Diagnostic> {
        let base = self.emit_expr(base)?;
        if self.terminated {
            return Ok(Operand::never());
        }
        let array_ty = llvm_value_type(&base.ty)?;
        match index {
            HirIndex::Static(index) => {
                let register = self.fresh_register();
                self.instruction(format!(
                    "{register} = extractvalue {array_ty} {}, {index}",
                    base.value()?
                ));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
            HirIndex::Dynamic(index) => {
                let index = self.emit_expr(index)?;
                if self.terminated {
                    return Ok(Operand::never());
                }
                let wide_index = self.fresh_register();
                self.instruction(format!("{wide_index} = sext i32 {} to i64", index.value()?));
                let in_bounds = self.fresh_register();
                self.instruction(format!("{in_bounds} = icmp ult i64 {wide_index}, {length}"));
                let ok_label = self.fresh_label("index.ok");
                let trap_label = self.fresh_label("index.trap");
                self.terminate(format!(
                    "br i1 {in_bounds}, label %{ok_label}, label %{trap_label}"
                ));

                self.start_block(&trap_label);
                self.instruction("call void @llvm.trap()");
                self.terminate("unreachable");

                self.start_block(&ok_label);
                let spill = self.entry_alloca(&array_ty, "array index spill");
                self.instruction(format!("store {array_ty} {}, ptr {spill}", base.value()?));
                let pointer = self.fresh_register();
                self.instruction(format!(
                    "{pointer} = getelementptr inbounds {array_ty}, ptr {spill}, i32 0, i64 {wide_index}"
                ));
                let register = self.fresh_register();
                let element_ty = llvm_value_type(&expression.ty)?;
                self.instruction(format!("{register} = load {element_ty}, ptr {pointer}"));
                Ok(Operand {
                    ty: expression.ty.clone(),
                    value: Some(register),
                })
            }
        }
    }

    fn emit_while(&mut self, condition: &HirExpr, body: &HirExpr) -> Result<Operand, Diagnostic> {
        let condition_label = self.fresh_label("while.condition");
        let body_label = self.fresh_label("while.body");
        let end_label = self.fresh_label("while.end");
        self.terminate(format!("br label %{condition_label}"));
        self.loops.push(EmitLoopTarget {
            break_label: end_label.clone(),
            result: None,
        });

        self.start_block(&condition_label);
        let condition = self.emit_expr(condition)?;
        if !self.terminated {
            self.terminate(format!(
                "br i1 {}, label %{body_label}, label %{end_label}",
                condition.value()?
            ));
            self.start_block(&body_label);
            self.emit_expr(body)?;
            if !self.terminated {
                self.terminate(format!("br label %{condition_label}"));
            }
        }

        self.loops.pop().expect("while emission frame");
        self.start_block(&end_label);
        Ok(Operand::unit())
    }

    fn emit_loop(&mut self, expression: &HirExpr, body: &HirExpr) -> Result<Operand, Diagnostic> {
        let body_label = self.fresh_label("loop.body");
        let end_label = self.fresh_label("loop.end");
        let result = if matches!(expression.ty, Ty::Unit | Ty::Never) {
            None
        } else {
            let ty = llvm_value_type(&expression.ty)?;
            Some((expression.ty.clone(), self.entry_alloca(&ty, "loop result")))
        };
        self.terminate(format!("br label %{body_label}"));
        self.loops.push(EmitLoopTarget {
            break_label: end_label.clone(),
            result: result.clone(),
        });
        self.start_block(&body_label);
        self.emit_expr(body)?;
        if !self.terminated {
            self.terminate(format!("br label %{body_label}"));
        }
        self.loops.pop().expect("loop emission frame");

        if expression.ty == Ty::Never {
            return Ok(Operand::never());
        }
        self.start_block(&end_label);
        let Some((ty, pointer)) = result else {
            return Ok(Operand::unit());
        };
        let register = self.fresh_register();
        let llvm_ty = llvm_value_type(&ty)?;
        self.instruction(format!("{register} = load {llvm_ty}, ptr {pointer}"));
        Ok(Operand {
            ty,
            value: Some(register),
        })
    }

    fn emit_break(&mut self, value: Option<&HirExpr>) -> Result<Operand, Diagnostic> {
        let target = self.loops.last().cloned().ok_or_else(|| {
            Diagnostic::new("internal error: break reached emission outside a loop")
        })?;
        let value = match value {
            Some(value) => Some(self.emit_expr(value)?),
            None => None,
        };
        if self.terminated {
            return Ok(Operand::never());
        }
        match (&target.result, value) {
            (Some((ty, pointer)), Some(value)) => {
                let llvm_ty = llvm_value_type(ty)?;
                self.instruction(format!("store {llvm_ty} {}, ptr {pointer}", value.value()?));
            }
            (Some(_), None) => {
                return Err(Diagnostic::new(
                    "internal error: value-producing loop break has no value",
                ));
            }
            (None, Some(value)) if value.ty != Ty::Unit => {
                return Err(Diagnostic::new(
                    "internal error: unit loop break carries a value",
                ));
            }
            (None, None) | (None, Some(_)) => {}
        }
        self.terminate(format!("br label %{}", target.break_label));
        Ok(Operand::never())
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
            let ty = llvm_value_type(&binding.ty)?;
            let pointer = self.entry_alloca(&ty, &llvm_comment(&binding.name));
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

    fn entry_alloca(&mut self, ty: &str, comment: &str) -> String {
        let pointer = self.fresh_register();
        self.entry_allocas.push_str("  ");
        self.entry_allocas.push_str(&pointer);
        self.entry_allocas.push_str(" = alloca ");
        self.entry_allocas.push_str(ty);
        if !comment.is_empty() {
            self.entry_allocas.push_str(" ; ");
            self.entry_allocas.push_str(comment);
        }
        self.entry_allocas.push('\n');
        pointer
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
        Ty::Array(element, length) => Ok(format!("[{length} x {}]", llvm_value_type(element)?)),
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
        Ty::Array(element, length) => {
            let length = usize::try_from(*length).ok()?;
            Some(ConstValue::Aggregate(
                (0..length)
                    .map(|_| zero_const(element, program))
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
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
        (ConstValue::Aggregate(values), Ty::Array(element, length)) => {
            if values.len() as u64 != *length {
                return Err(Diagnostic::new(
                    "internal error: constant array length does not match its type",
                ));
            }
            let element_ty = llvm_value_type(element)?;
            let elements = values
                .iter()
                .map(|value| {
                    Ok(format!(
                        "{element_ty} {}",
                        const_ir(value, element, program)?
                    ))
                })
                .collect::<Result<Vec<_>, Diagnostic>>()?;
            Ok(format!("[{}]", elements.join(", ")))
        }
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

fn collect_nominal_type_dependencies(
    ty: &Type,
    nominal_names: &HashSet<String>,
    bound: &HashSet<&str>,
    output: &mut Vec<String>,
) {
    match ty {
        Type::Array(element, _) => {
            collect_nominal_type_dependencies(element, nominal_names, bound, output)
        }
        Type::Named(name, _) if !bound.contains(name.as_str()) && nominal_names.contains(name) => {
            if !output.contains(name) {
                output.push(name.clone());
            }
        }
        Type::I32
        | Type::I64
        | Type::U32
        | Type::U64
        | Type::Bool
        | Type::Void
        | Type::Infer
        | Type::Named(_, _) => {}
    }
}

fn substitute_struct_types(definition: &mut StructDef, substitutions: &HashMap<String, Type>) {
    for field in &mut definition.fields {
        substitute_type_parameters(&mut field.ty, substitutions);
    }
}

fn substitute_enum_types(definition: &mut EnumDef, substitutions: &HashMap<String, Type>) {
    for variant in &mut definition.variants {
        match &mut variant.fields {
            VariantFields::Unit => {}
            VariantFields::Positional(types) => {
                for ty in types {
                    substitute_type_parameters(ty, substitutions);
                }
            }
            VariantFields::Named(fields) => {
                for field in fields {
                    substitute_type_parameters(&mut field.ty, substitutions);
                }
            }
        }
    }
}

fn substitute_function_types(function: &mut Function, substitutions: &HashMap<String, Type>) {
    for group in &mut function.groups {
        for parameter in group {
            substitute_type_parameters(&mut parameter.ty, substitutions);
        }
    }
    if let Some(result) = &mut function.return_type {
        substitute_type_parameters(result, substitutions);
    }
    if let Some(body) = &mut function.body {
        substitute_expr_types(body, substitutions);
    }
}

fn substitute_expr_types(expression: &mut Expr, substitutions: &HashMap<String, Type>) {
    match expression {
        Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Name(_) | Expr::Infer => {}
        Expr::Unary(_, operand) | Expr::Try(operand) | Expr::Throw(operand) => {
            substitute_expr_types(operand, substitutions)
        }
        Expr::Borrow { value, .. } => substitute_expr_types(value, substitutions),
        Expr::Binary(left, _, right) | Expr::Coalesce(left, right) | Expr::Assign(left, right) => {
            substitute_expr_types(left, substitutions);
            substitute_expr_types(right, substitutions);
        }
        Expr::Call(callee, arguments) => {
            substitute_expr_types(callee, substitutions);
            for argument in arguments {
                substitute_expr_types(&mut argument.value, substitutions);
            }
        }
        Expr::Member(base, _) => substitute_expr_types(base, substitutions),
        Expr::Array(elements) => {
            for element in elements {
                substitute_expr_types(element, substitutions);
            }
        }
        Expr::Index { base, index } => {
            substitute_expr_types(base, substitutions);
            substitute_expr_types(index, substitutions);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &mut binding.annotation {
                            substitute_type_parameters(annotation, substitutions);
                        }
                        substitute_expr_types(&mut binding.value, substitutions);
                    }
                    Stmt::Expr(expression) => substitute_expr_types(expression, substitutions),
                }
            }
            if let Some(tail) = tail {
                substitute_expr_types(tail, substitutions);
            }
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                substitute_type_parameters(&mut parameter.ty, substitutions);
            }
            substitute_expr_types(body, substitutions);
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_expr_types(condition, substitutions);
            substitute_expr_types(then_branch, substitutions);
            if let Some(else_branch) = else_branch {
                substitute_expr_types(else_branch, substitutions);
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                substitute_expr_types(value, substitutions);
            }
        }
        Expr::While { condition, body } => {
            substitute_expr_types(condition, substitutions);
            substitute_expr_types(body, substitutions);
        }
        Expr::Loop { body } => substitute_expr_types(body, substitutions),
        Expr::Match { scrutinee, arms } => {
            substitute_expr_types(scrutinee, substitutions);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    substitute_expr_types(guard, substitutions);
                }
                substitute_expr_types(&mut arm.body, substitutions);
            }
        }
    }
}

fn substitute_type_parameters(ty: &mut Type, substitutions: &HashMap<String, Type>) {
    match ty {
        Type::Named(name, arguments) if arguments.is_empty() => {
            if let Some(replacement) = substitutions.get(name) {
                *ty = replacement.clone();
            }
        }
        Type::Array(element, _) => substitute_type_parameters(element, substitutions),
        Type::Named(_, arguments) => {
            for argument in arguments {
                substitute_type_parameters(argument, substitutions);
            }
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Void | Type::Infer => {}
    }
}

fn function_instance_name(key: &FunctionInstanceKey) -> String {
    let mut canonical = String::from("$mono$fn$");
    push_canonical_component(&mut canonical, &key.template);
    canonical.push_str(&key.arguments.len().to_string());
    canonical.push(':');
    for argument in &key.arguments {
        let encoded = canonical_type_encoding(argument);
        push_canonical_component(&mut canonical, &encoded);
    }
    canonical
}

fn nominal_instance_name(key: &NominalInstanceKey) -> String {
    let mut canonical = String::from("$mono$type$");
    push_canonical_component(
        &mut canonical,
        match key.kind {
            NominalKind::Struct => "struct",
            NominalKind::Enum => "enum",
        },
    );
    push_canonical_component(&mut canonical, &key.template);
    canonical.push_str(&key.arguments.len().to_string());
    canonical.push(':');
    for argument in &key.arguments {
        let encoded = canonical_type_encoding(argument);
        push_canonical_component(&mut canonical, &encoded);
    }
    canonical
}

fn generic_validation_name(template: &str) -> String {
    let mut name = String::from("$generic$check$");
    push_canonical_component(&mut name, template);
    name
}

fn generic_parameter_marker(template: &str, index: usize, parameter: &str) -> String {
    let mut name = String::from("$generic$param$");
    push_canonical_component(&mut name, template);
    name.push_str(&index.to_string());
    name.push(':');
    push_canonical_component(&mut name, parameter);
    name
}

fn push_canonical_component(output: &mut String, component: &str) {
    output.push_str(&component.len().to_string());
    output.push(':');
    output.push_str(component);
}

fn canonical_type_encoding(ty: &Ty) -> String {
    match ty {
        Ty::I32 => "i32".to_owned(),
        Ty::I64 => "i64".to_owned(),
        Ty::U32 => "u32".to_owned(),
        Ty::U64 => "u64".to_owned(),
        Ty::Bool => "bool".to_owned(),
        Ty::Unit => "unit".to_owned(),
        Ty::Array(element, length) => {
            let element = canonical_type_encoding(element);
            let mut encoded = format!("array{length}:");
            push_canonical_component(&mut encoded, &element);
            encoded
        }
        Ty::Struct(name) => {
            let mut encoded = String::from("struct");
            push_canonical_component(&mut encoded, name);
            encoded
        }
        Ty::Enum(name) => {
            let mut encoded = String::from("enum");
            push_canonical_component(&mut encoded, name);
            encoded
        }
        Ty::Never => "never".to_owned(),
        Ty::Function(function) => {
            let mut encoded = String::from("function");
            encoded.push_str(&function.groups.len().to_string());
            encoded.push(':');
            for group in &function.groups {
                encoded.push_str(&group.len().to_string());
                encoded.push(':');
                for parameter in group {
                    push_canonical_component(&mut encoded, &canonical_type_encoding(parameter));
                }
            }
            push_canonical_component(&mut encoded, &canonical_type_encoding(&function.result));
            encoded
        }
        Ty::Error => "error".to_owned(),
    }
}

fn substitute_self_type(ty: &mut Type, target: &str) {
    match ty {
        Type::Array(element, _) => substitute_self_type(element, target),
        Type::Named(name, arguments) if name == "Self" && arguments.is_empty() => {
            *ty = Type::Named(target.to_owned(), Vec::new());
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                substitute_self_type(argument, target);
            }
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Void | Type::Infer => {}
    }
}

fn inherent_method_name(target: &str, member: &str) -> String {
    format!("{target}::method::{member}")
}

fn associated_function_name(target: &str, member: &str) -> String {
    format!("{target}::function::{member}")
}

fn associated_constant_name(target: &str, member: &str) -> String {
    format!("{target}::constant::{member}")
}

fn trait_method_name(key: &TraitImplKey, member: &str) -> String {
    let mut canonical = String::from("$trait$impl$");
    push_canonical_component(&mut canonical, &key.trait_ref.name);
    push_canonical_component(&mut canonical, &canonical_type_encoding(&key.self_ty));
    canonical.push_str(&key.trait_ref.arguments.len().to_string());
    canonical.push(':');
    for argument in &key.trait_ref.arguments {
        push_canonical_component(&mut canonical, &canonical_type_encoding(argument));
    }
    push_canonical_component(&mut canonical, member);
    canonical
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
            compile_groups: Vec::new(),
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
    fn monomorphizes_and_deduplicates_explicit_generic_function_calls() {
        let program = crate::parser::parse(
            "let identity(T: type)(move value: T): T = value\n\
             let main(): i32 = identity(i32)(40) + identity(i32)(2)\n",
        )
        .expect("generic source must parse");
        let mut analyzer = Analyzer::new(&program);
        let hir = analyzer.analyze();
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert!(hir.is_some());
        assert_eq!(analyzer.function_instances.len(), 1);
        let instance = analyzer
            .function_instances
            .values()
            .next()
            .expect("identity instance");
        assert_eq!(instance.key.arguments, vec![Ty::I32]);
        assert!(instance.canonical.starts_with("$mono$fn$"));
    }

    #[test]
    fn inferred_and_explicit_type_arguments_share_instance_cache_keys() {
        let program = crate::parser::parse(
            "let identity(T: type)(move value: T): T = value\n\
             let Cell(T: type) = struct(value: T)\n\
             let main(): i32 = {\n\
               let explicit = Cell(i32)(identity(i32)(20))\n\
               let inferred_value = identity(_)(22)\n\
               let inferred = Cell(_)(inferred_value)\n\
               explicit.value + inferred.value\n\
             }\n",
        )
        .expect("mixed explicit and inferred source must parse");
        let mut analyzer = Analyzer::new(&program);
        let hir = analyzer.analyze();
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert!(hir.is_some(), "mixed generic program HIR");

        let function_instances: Vec<_> = analyzer
            .function_instances
            .values()
            .filter(|instance| instance.key.template == "identity")
            .collect();
        assert_eq!(function_instances.len(), 1);
        assert_eq!(function_instances[0].key.arguments, vec![Ty::I32]);

        let nominal_instances: Vec<_> = analyzer
            .nominal_instances
            .values()
            .filter(|instance| instance.key.template == "Cell")
            .collect();
        assert_eq!(nominal_instances.len(), 1);
        assert_eq!(nominal_instances[0].key.arguments, vec![Ty::I32]);
    }

    #[test]
    fn inference_reifies_and_decomposes_generic_nominal_types() {
        let program = crate::parser::parse(
            "let Cell(T: type) = struct(value: T)\n\
             let unwrap(T: type)(move value: Cell(T)): T = value.value\n\
             let main(): i32 = {\n\
               let inner = Cell(i32)(42)\n\
               let outer = Cell(_)(inner)\n\
               let nested = unwrap(_)(outer.value)\n\
               let direct = unwrap(_)(Cell(i32)(0))\n\
               nested + direct\n\
             }\n",
        )
        .expect("nested inferred nominal source must parse");
        let mut analyzer = Analyzer::new(&program);
        analyzer.analyze().expect("nested inferred nominal HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );

        let inner_key = NominalInstanceKey {
            kind: NominalKind::Struct,
            template: "Cell".into(),
            arguments: vec![Ty::I32],
        };
        let inner = analyzer.nominal_instance_names[&inner_key].clone();
        let outer_key = NominalInstanceKey {
            kind: NominalKind::Struct,
            template: "Cell".into(),
            arguments: vec![Ty::Struct(inner)],
        };
        assert!(analyzer.nominal_instance_names.contains_key(&outer_key));

        let unwrap_instances: Vec<_> = analyzer
            .function_instances
            .values()
            .filter(|instance| instance.key.template == "unwrap")
            .collect();
        assert_eq!(unwrap_instances.len(), 1);
        assert_eq!(unwrap_instances[0].key.arguments, vec![Ty::I32]);
    }

    #[test]
    fn integer_constraints_precede_defaulting_independent_of_source_order() {
        let program = crate::parser::parse(
            "let same(T: type)(left: T, right: T): i32 = 14\n\
             let accept(T: type)(value: T): i32 = 14\n\
             let main(): i32 = {\n\
               let wide: i64 = 7\n\
               same(_)(0, wide) + same(_)(wide, 0) + accept(_)(0 + wide)\n\
             }\n",
        )
        .expect("ordered inference source must parse");
        let mut analyzer = Analyzer::new(&program);
        analyzer.analyze().expect("ordered inference HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );
        for template in ["same", "accept"] {
            let instances: Vec<_> = analyzer
                .function_instances
                .values()
                .filter(|instance| instance.key.template == template)
                .collect();
            assert_eq!(instances.len(), 1);
            assert_eq!(instances[0].key.arguments, vec![Ty::I64]);
        }
    }

    #[test]
    fn explicit_generic_enum_values_are_available_to_outer_inference() {
        let source = "let Maybe(T: type) = enum { Some(T), None }\n\
                      let identity(T: type)(move value: T): T = value\n\
                      let main(): i32 = {\n\
                        let some = identity(_)(Maybe(i32).Some(42))\n\
                        let none: Maybe(i32) = identity(_)(Maybe(i32).None)\n\
                        some match { Some(value) => value, None => 0 }\n\
                      }\n";
        compile_text(source).expect("outer inference over enum constructors must compile");
    }

    #[test]
    fn inference_conflicts_do_not_materialize_instances() {
        let program = crate::parser::parse(
            "let identity(T: type)(move value: T): T = value\n\
             let Cell(T: type) = struct(value: T)\n\
             let main(): bool = identity(_)(Cell(i32)(42))\n",
        )
        .expect("conflicting inference source must parse");
        let mut analyzer = Analyzer::new(&program);
        assert!(analyzer.analyze().is_none());
        assert!(analyzer.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("conflicting inference for type parameter `T`")
        }));
        assert!(analyzer.function_instances.is_empty());
        assert!(analyzer.function_instance_names.is_empty());
        assert!(analyzer.nominal_instances.is_empty());
        assert!(analyzer.nominal_instance_names.is_empty());
    }

    #[test]
    fn template_validation_rolls_back_temporary_instances_and_emits_closed_ir() {
        let program = crate::parser::parse(
            "let identity(T: type)(move value: T): T = value\n\
             let wrap(T: type)(move value: T): T = identity(T)(value)\n\
             let main(): i32 = wrap(i32)(42)\n",
        )
        .expect("generic composition must parse");
        let mut analyzer = Analyzer::new(&program);
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected validation diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert!(analyzer.function_instances.is_empty());
        assert!(analyzer.function_instance_names.is_empty());
        assert!(analyzer.function_type_substitutions.is_empty());
        assert!(analyzer
            .function_order
            .iter()
            .all(|name| !name.contains("$generic$")));

        let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
        let hir = analyzer.analyze().expect("closed program HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected lowering diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert_eq!(analyzer.function_instances.len(), 2);
        assert!(hir
            .functions
            .iter()
            .all(|function| !function.name.contains("$generic$")));
        assert!(analyzer.function_instances.values().all(|instance| {
            instance
                .key
                .arguments
                .iter()
                .all(|argument| match argument {
                    Ty::Struct(name) | Ty::Enum(name) => !markers.contains(name),
                    _ => true,
                })
        }));

        let ir = compile(&program).expect("closed generic composition must compile");
        for marker in markers {
            assert!(!ir.contains(&marker));
            assert!(!ir.contains(&hex_name(&marker)));
        }
    }

    #[test]
    fn inferred_template_calls_roll_back_abstract_instances() {
        let program = crate::parser::parse(
            "let identity(U: type)(move value: U): U = value\n\
             let wrap(T: type)(move value: T): T = identity(_)(value)\n\
             let main(): i32 = wrap(i32)(42)\n",
        )
        .expect("inferred generic composition must parse");
        let mut analyzer = Analyzer::new(&program);
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected validation diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert!(analyzer.function_instances.is_empty());
        assert!(analyzer.function_instance_names.is_empty());
        assert!(analyzer.function_type_substitutions.is_empty());

        let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
        let hir = analyzer.analyze().expect("closed inferred generic HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected lowering diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert_eq!(analyzer.function_instances.len(), 2);
        assert!(hir
            .functions
            .iter()
            .all(|function| !function.name.contains("$generic$")));
        assert!(analyzer.function_instances.values().all(|instance| {
            instance
                .key
                .arguments
                .iter()
                .all(|argument| match argument {
                    Ty::Struct(name) | Ty::Enum(name) => !markers.contains(name),
                    _ => true,
                })
        }));

        let ir = compile(&program).expect("closed inferred composition must compile");
        for marker in markers {
            assert!(!ir.contains(&marker));
            assert!(!ir.contains(&hex_name(&marker)));
        }
    }

    #[test]
    fn registers_plain_nominals_and_deduplicates_generic_nominal_instances() {
        let program = crate::parser::parse(
            "let Plain = struct(value: i32)\n\
             let Cell(T: type) = struct(value: T)\n\
             let main(): i32 = Cell(i32)(Plain(40).value).value + Cell(i32)(2).value\n",
        )
        .expect("generic nominal source must parse");
        let mut analyzer = Analyzer::new(&program);
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected validation diagnostics: {:?}",
            analyzer.diagnostics
        );

        let plain = analyzer
            .nominal_instances
            .get("Plain")
            .expect("plain nominal metadata");
        assert_eq!(plain.canonical, "Plain");
        assert_eq!(plain.key.kind, NominalKind::Struct);
        assert_eq!(plain.key.template, "Plain");
        assert!(plain.key.arguments.is_empty());
        assert!(analyzer
            .nominal_instances
            .values()
            .all(|instance| instance.key.arguments.is_empty()));

        let hir = analyzer.analyze().expect("generic nominal HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected lowering diagnostics: {:?}",
            analyzer.diagnostics
        );
        let key = NominalInstanceKey {
            kind: NominalKind::Struct,
            template: "Cell".into(),
            arguments: vec![Ty::I32],
        };
        let canonical = analyzer
            .nominal_instance_names
            .get(&key)
            .expect("Cell(i32) canonical name");
        let instances: Vec<_> = analyzer
            .nominal_instances
            .values()
            .filter(|instance| instance.key.template == "Cell")
            .collect();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].key, key);
        assert_eq!(instances[0].canonical, *canonical);
        assert!(canonical.starts_with("$mono$type$"));
        assert!(hir.structs.iter().any(|layout| layout.name == *canonical));
    }

    #[test]
    fn materializes_nested_generic_struct_layouts_in_dependency_order() {
        let program = crate::parser::parse(
            "let Cell(T: type) = struct(value: T)\n\
             let main(): i32 = Cell(Cell(i32))(Cell(i32)(42)).value.value\n",
        )
        .expect("nested generic nominal source must parse");
        let mut analyzer = Analyzer::new(&program);
        let hir = analyzer.analyze().expect("nested generic nominal HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );

        let inner_key = NominalInstanceKey {
            kind: NominalKind::Struct,
            template: "Cell".into(),
            arguments: vec![Ty::I32],
        };
        let inner = analyzer.nominal_instance_names[&inner_key].clone();
        let outer_key = NominalInstanceKey {
            kind: NominalKind::Struct,
            template: "Cell".into(),
            arguments: vec![Ty::Struct(inner.clone())],
        };
        let outer = analyzer.nominal_instance_names[&outer_key].clone();
        assert_ne!(inner, outer);
        assert_eq!(
            analyzer.struct_layouts[&outer].fields[0].ty,
            Ty::Struct(inner.clone())
        );
        let inner_index = hir
            .structs
            .iter()
            .position(|layout| layout.name == inner)
            .expect("inner layout order");
        let outer_index = hir
            .structs
            .iter()
            .position(|layout| layout.name == outer)
            .expect("outer layout order");
        assert!(inner_index < outer_index);
    }

    #[test]
    fn lowers_generic_enum_type_heads_unit_variants_and_short_patterns() {
        let ir = compile_text(
            "let Maybe(T: type) = enum {\n\
               Some(T),\n\
               None,\n\
             }\n\
             let choose(flag: bool): Maybe(i32) = if flag {\n\
               Maybe(i32).Some(42)\n\
             } else {\n\
               Maybe(i32).None\n\
             }\n\
             let unwrap(move value: Maybe(i32)): i32 = value match {\n\
               Some(item) => item,\n\
               None => 0,\n\
             }\n\
             let main(): i32 = unwrap(choose(false))\n",
        )
        .expect("generic enum program must compile");
        let key = NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Maybe".into(),
            arguments: vec![Ty::I32],
        };
        let canonical = nominal_instance_name(&key);
        assert!(ir.contains(&hex_name(&canonical)));
    }

    #[test]
    fn registers_option_and_result_as_validated_prelude_templates() {
        let analyzer = Analyzer::new(&Program { items: Vec::new() });
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected prelude diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert_eq!(
            &analyzer.enum_template_order[..2],
            &["Option".to_owned(), "Result".to_owned()]
        );

        let option = &analyzer.enum_templates["Option"];
        assert_eq!(option.compile_groups.len(), 1);
        assert_eq!(option.compile_groups[0].len(), 1);
        assert_eq!(option.compile_groups[0][0].name, "T");
        assert_eq!(option.variants.len(), 2);
        assert_eq!(option.variants[0].name, "Some");
        assert_eq!(
            option.variants[0].fields,
            VariantFields::Positional(vec![Type::Named("T".into(), Vec::new())])
        );
        assert_eq!(option.variants[1].name, "None");
        assert_eq!(option.variants[1].fields, VariantFields::Unit);

        let result = &analyzer.enum_templates["Result"];
        assert_eq!(result.compile_groups.len(), 1);
        assert_eq!(
            result.compile_groups[0]
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            vec!["T", "E"]
        );
        assert_eq!(result.variants.len(), 2);
        assert_eq!(result.variants[0].name, "Ok");
        assert_eq!(result.variants[1].name, "Err");

        assert!(analyzer.nominal_instances.is_empty());
        assert!(analyzer.nominal_instance_names.is_empty());
        assert!(analyzer.enum_layouts.is_empty());
        assert!(analyzer.functions.is_empty());
        assert!(analyzer.function_templates.is_empty());
        assert!(analyzer.function_order.is_empty());
    }

    #[test]
    fn constructs_infers_and_matches_prelude_option_and_result() {
        let ir = compile_text(
            r#"
let unwrap_option(move value: Option(i32)): i32 = value match {
  Some(item) => item,
  None => 0,
}
let unwrap_result(move value: Result(i32, bool)): i32 = value match {
  Ok(item) => item,
  Err(_) => 0,
}
let main(): i32 = {
  let some = Option(_).Some(19)
  let none: Option(i32) = Option(_).None
  let ok: Result(i32, bool) = Result(_, _).Ok(23)
  let err: Result(i32, bool) = Result(_, _).Err(false)
  unwrap_option(some) + unwrap_option(none) + unwrap_result(ok) + unwrap_result(err)
}
"#,
        )
        .expect("prelude Option and Result program must compile");
        let option = NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Option".into(),
            arguments: vec![Ty::I32],
        };
        let result = NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Result".into(),
            arguments: vec![Ty::I32, Ty::Bool],
        };
        assert!(ir.contains(&hex_name(&nominal_instance_name(&option))));
        assert!(ir.contains(&hex_name(&nominal_instance_name(&result))));
    }

    #[test]
    fn coalesce_lowers_option_and_result_through_lazy_match_control_flow() {
        let ir = compile_text(
            r#"
let make(mut borrow count: i32): Option(i32) = {
  count = count + 1
  Option(i32).Some(20)
}
let fallback(mut borrow count: i32): i32 = {
  count = count + 10
  22
}
let main(): i32 = {
  let mut count = 0
  let option = make(count) ?? fallback(count)
  let result = Result(i32, bool).Err(false) ?? option
  if count == 11 { result } else { 0 }
}
"#,
        )
        .expect("Option and Result coalescing must compile");

        let calls_to_make = ir
            .lines()
            .filter(|line| line.contains("call ") && line.contains(&function_symbol("make")))
            .count();
        assert_eq!(calls_to_make, 1, "left operand must be evaluated once");
        assert!(ir.matches("switch i32").count() >= 2);
        assert!(
            ir.contains(" phi "),
            "coalesce result must join through a phi"
        );
    }

    #[test]
    fn throw_returns_the_enclosing_result_error_variant() {
        let ir = compile_text(
            r#"
let answer(fail: bool): Result(i32, bool) = {
  if fail { throw true }
  42
}
let main(): i32 = answer(true) ?? 42
"#,
        )
        .expect("throw in an explicit Result function must compile");
        assert!(ir.contains("switch i32"));
        assert!(ir.contains("ret %sali.type."));
    }

    #[test]
    fn throw_requires_an_exact_explicit_result_error_boundary() {
        for (source, expected) in [
            (
                "let fail(): Result(i32, bool) = throw 0\nlet main(): i32 = 0\n",
                "where `bool` is expected",
            ),
            (
                "let fail(): Option(i32) = throw false\nlet main(): i32 = 0\n",
                "only propagate through a `Result`",
            ),
            (
                "let fail(): i32 = throw false\nlet main(): i32 = 0\n",
                "explicit `Result` return type",
            ),
            (
                "let fail() = throw false\nlet main(): i32 = 0\n",
                "explicit `Result` return type",
            ),
        ] {
            let errors = compile_text(source).expect_err("invalid throw must be rejected");
            assert!(
                errors.iter().any(|error| error.message.contains(expected)),
                "missing `{expected}` diagnostic in {errors:?}"
            );
        }
    }

    #[test]
    fn coalesce_hir_keeps_the_fallback_call_in_the_residual_arm() {
        let program = crate::parser::parse(
            r#"
let fallback(): i32 = 42
let main(): i32 = Option(i32).Some(20) ?? fallback()
"#,
        )
        .expect("coalesce source must parse");
        let mut analyzer = Analyzer::new(&program);
        let hir = analyzer.analyze().expect("coalesce HIR must lower");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );
        let main = hir
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("main HIR");
        let HirExprKind::Match { arms, .. } = &main.body.kind else {
            panic!("coalesce must lower to match HIR");
        };
        assert_eq!(main.body.ty, Ty::I32);
        assert_eq!(arms.len(), 2);
        assert!(matches!(arms[0].body.kind, HirExprKind::Read { .. }));
        assert!(matches!(
            &arms[1].body.kind,
            HirExprKind::Call { function, .. } if function == "fallback"
        ));
    }

    #[test]
    fn coalesce_probe_participates_in_outer_inference_and_nests_right_associatively() {
        compile_text(
            r#"
let identity(T: type)(move value: T): T = value
let Boxed(T: type) = struct(value: T)
let main(): i32 = {
  let first = Option(i32).None
  let second = Option(i32).Some(42)
  let scalar = identity(_)(Option(_).None ?? 42)
  let boxed = identity(_)(Option(_).None ?? Boxed(i32)(value: 42))
  let wide = identity(_)(Result(i64, _).Err(false) ?? 42)
  identity(_)(first ?? second ?? 0) + scalar + boxed.value - 84
}
"#,
        )
        .expect("coalesce payload type must be visible to outer inference");
    }

    #[test]
    fn coalesce_infers_empty_builtin_variants_from_expected_or_rhs_payloads() {
        compile_text(
            r#"
let main(): i32 = {
  let inferred_option = Option(_).None ?? 40
  let inferred_result = Result(_, bool).Err(false) ?? 2
  let option = Option(_).None ?? 40
  let result = Result(_, bool).Err(false) ?? 1
  let fully_inferred_result = Result(_, _).Err(false) ?? 1
  let wide = Result(i64, _).Err(false) ?? 42
  let nested = Option(i32).None ?? Option(_).None ?? 1
  inferred_option + inferred_result + option + result + fully_inferred_result + nested - 43
}
"#,
        )
        .expect("coalesce payload evidence must resolve empty variants");
    }

    #[test]
    fn coalesce_does_not_guess_an_unconstrained_result_error_type() {
        let errors = compile_text("let main(): i32 = Result(_, _).Ok(40) ?? 2\n").unwrap_err();
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.message.contains("cannot infer type argument")
                && diagnostic.message.contains("`E`")
                && diagnostic.message.contains("`Result`")
        }));
    }

    #[test]
    fn coalesce_reports_non_containers_mismatched_fallbacks_and_moves() {
        let non_container = compile_text("let main(): i32 = 40 ?? 2\n").unwrap_err();
        assert!(non_container.iter().any(|diagnostic| diagnostic.message
            == "operator `??` requires `Option(T)` or `Result(T, E)` on the left, found `i32`"));

        let mismatch = compile_text("let main(): i32 = Option(i32).None ?? true\n").unwrap_err();
        assert!(mismatch
            .iter()
            .any(|diagnostic| diagnostic.message.contains("type mismatch")));

        let moved = compile_text(
            r#"
let main(): i32 = {
  let value = Result(i32, bool).Ok(42)
  let answer = value ?? 0
  value match { Ok(item) => item, Err(_) => answer }
}
"#,
        )
        .unwrap_err();
        assert!(moved
            .iter()
            .any(|diagnostic| diagnostic.message.contains("moved")));
    }

    #[test]
    fn coalesce_joins_a_fallback_only_move_as_possibly_moved() {
        let errors = compile_text(
            r#"
let Boxed = struct(value: i32)
let consume(move value: Boxed): i32 = value.value
let main(): i32 = {
  let spare = Boxed(41)
  let choice = Option(i32).Some(1)
  let answer = choice ?? consume(spare)
  consume(spare) + answer
}
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|diagnostic| diagnostic.message == "use of possibly moved value"));
    }

    #[test]
    fn keeps_prelude_nominal_instances_structurally_isolated() {
        let program = crate::parser::parse(
            r#"
let main(): i32 = {
  let number = Option(_).Some(42)
  let flag = Option(_).Some(true)
  let first: Result(i32, bool) = Result(_, _).Ok(42)
  let second: Result(bool, i32) = Result(_, _).Err(0)
  let value = number match { Some(item) => item, None => 0 }
  let enabled = flag match { Some(item) => item, None => false }
  let left = first match { Ok(item) => item, Err(_) => 0 }
  let right = second match { Ok(_) => 0, Err(item) => item }
  if enabled { value + left - right - 42 } else { 0 }
}
"#,
        )
        .expect("multiple prelude instances source must parse");
        let mut analyzer = Analyzer::new(&program);
        analyzer.analyze().expect("multiple prelude instances HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analyzer.diagnostics
        );

        let option_i32 = NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Option".into(),
            arguments: vec![Ty::I32],
        };
        let option_bool = NominalInstanceKey {
            kind: NominalKind::Enum,
            template: "Option".into(),
            arguments: vec![Ty::Bool],
        };
        let option_i32_name = analyzer.nominal_instance_names[&option_i32].clone();
        let option_bool_name = analyzer.nominal_instance_names[&option_bool].clone();
        assert_ne!(option_i32_name, option_bool_name);
        assert_eq!(
            analyzer.enum_layouts[&option_i32_name].variants[0].fields[0].ty,
            Ty::I32
        );
        assert_eq!(
            analyzer.enum_layouts[&option_bool_name].variants[0].fields[0].ty,
            Ty::Bool
        );

        let result_keys = analyzer
            .nominal_instances
            .values()
            .filter(|instance| instance.key.template == "Result")
            .map(|instance| instance.key.arguments.clone())
            .collect::<HashSet<_>>();
        assert_eq!(
            result_keys,
            HashSet::from([vec![Ty::I32, Ty::Bool], vec![Ty::Bool, Ty::I32]])
        );
    }

    #[test]
    fn rejects_user_redefinitions_of_prelude_nominal_names() {
        for (source, name) in [
            (
                "let Option = struct(value: i32)\nlet main(): i32 = 42\n",
                "Option",
            ),
            (
                "let Result(T: type) = enum { Value(T) }\nlet main(): i32 = 42\n",
                "Result",
            ),
        ] {
            let program = crate::parser::parse(source).expect("reserved-name source must parse");
            let analyzer = Analyzer::new(&program);
            assert!(analyzer.diagnostics.iter().any(|diagnostic| {
                diagnostic.message == format!("duplicate top-level name `{name}`")
            }));
            let retained = &analyzer.enum_templates[name];
            let retained_variants = retained
                .variants
                .iter()
                .map(|variant| variant.name.as_str())
                .collect::<Vec<_>>();
            assert_eq!(
                retained_variants,
                if name == "Option" {
                    vec!["Some", "None"]
                } else {
                    vec!["Ok", "Err"]
                }
            );
        }
    }

    #[test]
    fn generic_function_validation_rolls_back_temporary_nominal_instances() {
        let program = crate::parser::parse(
            "let Cell(T: type) = struct(value: T)\n\
             let wrap(T: type)(move value: T): Cell(T) = Cell(T)(value)\n\
             let main(): i32 = wrap(i32)(42).value\n",
        )
        .expect("generic function and nominal source must parse");
        let mut analyzer = Analyzer::new(&program);
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected validation diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert!(analyzer.nominal_instances.is_empty());
        assert!(analyzer.nominal_instance_names.is_empty());
        assert!(analyzer.struct_layouts.is_empty());
        assert!(analyzer.struct_order.is_empty());

        let markers: HashSet<_> = analyzer.abstract_type_parameters.keys().cloned().collect();
        analyzer.analyze().expect("closed generic nominal HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected lowering diagnostics: {:?}",
            analyzer.diagnostics
        );
        let instances: Vec<_> = analyzer
            .nominal_instances
            .values()
            .filter(|instance| instance.key.template == "Cell")
            .collect();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].key.arguments, vec![Ty::I32]);
        assert!(analyzer.nominal_instances.keys().all(|name| {
            !name.contains("$generic$") && markers.iter().all(|marker| !name.contains(marker))
        }));

        let ir = compile(&program).expect("closed generic nominal program must compile");
        for marker in markers {
            assert!(!ir.contains(&marker));
            assert!(!ir.contains(&hex_name(&marker)));
        }
    }

    #[test]
    fn rejects_invalid_generic_nominal_forms_without_instantiating_them() {
        let cases = [
            (
                "let Invalid(T: type) = struct(next: Invalid(T))\n\
                 let main(): i32 = 42\n",
                "recursive generic value layout has infinite size",
            ),
            (
                "let Wrap(T: type) = struct(value: T)\n\
                 let Grow(T: type) = struct(next: Wrap(Grow(Wrap(T))))\n\
                 let main(): i32 = 42\n",
                "recursive generic value layout has infinite size",
            ),
            (
                "let Invalid(T: type) = struct(value: Missing)\n\
                 let main(): i32 = 42\n",
                "unknown type `Missing`",
            ),
            (
                "let Cell(T: type) = struct(value: T)\n\
                 let main(): i32 = Cell(Cell(_))(Cell(i32)(42)).value.value\n",
                "nested `_` type argument inference is not supported",
            ),
            (
                "let Cell(T: type) = struct(value: T)\n\
                 extend Cell { let answer = 42 }\n\
                 let main(): i32 = 42\n",
                "generic extend target `Cell` is not supported",
            ),
        ];
        for (source, expected) in cases {
            let diagnostics = compile_text(source).expect_err("source must be rejected");
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains(expected)),
                "missing `{expected}` in {diagnostics:?}"
            );
        }
    }

    #[test]
    fn validation_rollback_does_not_keep_inferred_helpers_or_drop_real_instances() {
        let source = "let identity(T: type)(move value: T): T = value\n\
                      let helper(value: i32) = identity(i32)(value)\n\
                      let preserve(T: type)(move value: T): T = { helper(0); value }\n\
                      let main(): i32 = preserve(i32)(42)\n";
        let ir = compile_text(source).expect("validation rollback program must compile");
        let identity = function_instance_name(&FunctionInstanceKey {
            template: "identity".into(),
            arguments: vec![Ty::I32],
        });
        let symbol = function_symbol(&identity);
        assert!(ir.contains(&format!("define internal i32 @{symbol}(i32 %arg.0)")));
    }

    #[test]
    fn creates_distinct_stable_names_for_generic_function_instances() {
        let i32_key = FunctionInstanceKey {
            template: "identity".into(),
            arguments: vec![Ty::I32],
        };
        let bool_key = FunctionInstanceKey {
            template: "identity".into(),
            arguments: vec![Ty::Bool],
        };
        assert_eq!(
            function_instance_name(&i32_key),
            function_instance_name(&i32_key)
        );
        assert_ne!(
            function_instance_name(&i32_key),
            function_instance_name(&bool_key)
        );
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
    fn stores_and_consumes_an_fn_once_nominal_capture() {
        let ir = compile_text(
            r#"
let Payload = struct(value: i32)
let take(move payload: Payload): i32 = payload.value
let main(): i32 = {
  let payload = Payload(value: 42)
  let invoke = { -> take(payload) }
  invoke()
}
"#,
        )
        .unwrap();
        let symbol = function_symbol("__closure.0");
        assert!(ir.contains(&format!(
            "define internal i32 @{symbol}(%sali.type.5061796c6f6164 %arg.0)"
        )));
        assert!(ir.contains("; closure capture"));
        assert!(ir.contains(&format!("call i32 @{symbol}(%sali.type.5061796c6f6164")));
    }

    #[test]
    fn rejects_calling_an_fn_once_closure_twice() {
        let errors = compile_text(
            r#"
let Payload = struct(value: i32)
let take(move payload: Payload): i32 = payload.value
let main(): i32 = {
  let payload = Payload(value: 42)
  let invoke = { -> take(payload) }
  invoke()
  invoke()
}
"#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| {
            error.message.contains("FnOnce") && error.message.contains("consumed")
        }));
    }

    #[test]
    fn moves_an_fn_once_source_when_the_closure_is_created() {
        let errors = compile_text(
            r#"
let Payload = struct(value: i32)
let take(move payload: Payload): i32 = payload.value
let main(): i32 = {
  let payload = Payload(value: 42)
  let invoke = { -> take(payload) }
  payload.value
}
"#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| error.message.contains("moved")));
    }

    #[test]
    fn flattens_all_groups_of_a_curried_capturing_closure() {
        let ir = compile_text(
            r#"
let main(): i32 = {
  let base = 40
  let add = { (x: i32)(y: i32) -> base + x + y }
  add(1)(1)
}
"#,
        )
        .unwrap();
        let symbol = function_symbol("__closure.0");
        assert!(ir.contains(&format!(
            "define internal i32 @{symbol}(ptr %arg.0, i32 %arg.1, i32 %arg.2)"
        )));
        assert!(ir.contains(&format!("call i32 @{symbol}(ptr")));
    }

    #[test]
    fn rejects_partial_application_of_a_curried_closure() {
        let errors = compile_text(
            r#"
let main(): i32 = {
  let base = 40
  let add = { (x: i32)(y: i32) -> base + x + y }
  let add_one = add(1)
  add_one(1)
}
"#,
        )
        .unwrap_err();
        assert!(errors
            .iter()
            .any(|error| error.message.contains("partial application")));
    }

    #[test]
    fn emits_kind_discriminated_inherent_receiver_abis() {
        let ir = compile_text(
            r#"
let Counter = struct(value: i32)
extend Counter {
  let read(borrow self)(): i32 = self.value
  let reset(mut borrow self)(): () = { self.value = 0 }
  let take(move self)(): i32 = self.value
  let answer = 1
}
let main(): i32 = {
  let mut counter = Counter(42)
  let value = counter.read()
  counter.reset()
  value + counter.take() + Counter.answer
}
"#,
        )
        .unwrap();
        let read = function_symbol("Counter::method::read");
        let reset = function_symbol("Counter::method::reset");
        let take = function_symbol("Counter::method::take");
        let answer = global_symbol("Counter::constant::answer");
        assert!(ir.contains(&format!("define internal i32 @{read}(ptr %arg.0)")));
        assert!(ir.contains(&format!("define internal void @{reset}(ptr %arg.0)")));
        assert!(ir.contains(&format!(
            "define internal i32 @{take}(%sali.type.436f756e746572 %arg.0)"
        )));
        assert!(ir.contains(&format!("call i32 @{read}(ptr")));
        assert!(ir.contains(&format!("call void @{reset}(ptr")));
        assert!(ir.contains(&format!("call i32 @{take}(%sali.type.436f756e746572")));
        assert!(ir.contains(&format!("@{answer} = internal unnamed_addr constant i32 1")));
    }

    #[test]
    fn registers_generic_trait_metadata_and_emits_static_method_dispatch() {
        let program = crate::parser::parse(
            r#"
let Convert(Rhs: type) = trait {
  let Output: type
  let convert(borrow self)(move rhs: Rhs): Output
}
let Number = struct(value: i32)
extend Number: Convert(i32) {
  let Output = i32
  let convert(borrow self)(move rhs: i32): i32 = self.value + rhs
}
let main(): i32 = {
  let number = Number(40)
  number.convert(2)
}
"#,
        )
        .expect("generic trait source must parse");
        let mut analyzer = Analyzer::new(&program);
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected collection diagnostics: {:?}",
            analyzer.diagnostics
        );
        assert_eq!(analyzer.trait_impls.len(), 1);
        let (key, implementation) = analyzer.trait_impls.iter().next().unwrap();
        assert_eq!(key.self_ty, Ty::Struct("Number".into()));
        assert_eq!(key.trait_ref.name, "Convert");
        assert_eq!(key.trait_ref.arguments, vec![Ty::I32]);
        assert_eq!(implementation.associated_types["Output"], Ty::I32);
        let canonical = trait_method_name(key, "convert");
        assert_eq!(implementation.methods["convert"], canonical);

        analyzer.analyze().expect("concrete trait program HIR");
        assert!(
            analyzer.diagnostics.is_empty(),
            "unexpected lowering diagnostics: {:?}",
            analyzer.diagnostics
        );
        let ir = compile(&program).expect("concrete trait program must compile");
        let symbol = function_symbol(&canonical);
        assert!(ir.contains(&format!(
            "define internal i32 @{symbol}(ptr %arg.0, i32 %arg.1)"
        )));
        assert!(ir.contains(&format!("call i32 @{symbol}(ptr")));
    }

    #[test]
    fn lowers_alpha_equivalent_add_trait_to_a_static_call() {
        let program = crate::parser::parse(
            r#"
let Add(Other: type) = trait {
  let add(move self)(move other: Other): Output
  let Output: type
}
let Number = struct(value: i32)
extend Number: Add(Number) {
  let Output = i32
  let add(move self)(move other: Number): i32 = self.value + other.value
}
let main(): i32 = Number(40) + Number(2)
"#,
        )
        .expect("alpha-equivalent Add source must parse");
        let key = TraitImplKey {
            self_ty: Ty::Struct("Number".into()),
            trait_ref: TraitRefKey {
                name: "Add".into(),
                arguments: vec![Ty::Struct("Number".into())],
            },
        };
        let symbol = function_symbol(&trait_method_name(&key, "add"));
        let ir = compile(&program).expect("alpha-equivalent Add source must compile");
        assert!(ir.contains(&format!("call i32 @{symbol}(")));
        assert!(
            ir.contains("add i32"),
            "integer addition must stay built in"
        );
    }

    #[test]
    fn add_output_participates_in_outer_generic_inference() {
        let ir = compile_text(
            r#"
let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
let Number = struct(value: i32)
extend Number: Add(i32) {
  let Output = i32
  let add(move self)(move rhs: i32): i32 = self.value + rhs
}
let identity(T: type)(move value: T): T = value
let main(): i32 = identity(_)(Number(40) + 2)
"#,
        )
        .expect("Add Output must be visible to generic inference");
        let identity = function_instance_name(&FunctionInstanceKey {
            template: "identity".into(),
            arguments: vec![Ty::I32],
        });
        assert!(ir.contains(&format!("call i32 @{}(", function_symbol(&identity))));
    }

    #[test]
    fn add_literal_range_eliminates_incompatible_rhs_candidates() {
        let program = crate::parser::parse(
            r#"
let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
let Number = struct(value: i32)
extend Number: Add(i32) {
  let Output = i64
  let add(move self)(move rhs: i32): i64 = 0
}
extend Number: Add(i64) {
  let Output = i64
  let add(move self)(move rhs: i64): i64 = rhs
}
let main(): i32 = {
  let answer: i64 = Number(0) + do { 2147483648 }
  if answer == 2147483648 { 42 } else { 0 }
}
"#,
        )
        .expect("large literal Add source must parse");
        let i32_key = TraitImplKey {
            self_ty: Ty::Struct("Number".into()),
            trait_ref: TraitRefKey {
                name: "Add".into(),
                arguments: vec![Ty::I32],
            },
        };
        let i64_key = TraitImplKey {
            self_ty: Ty::Struct("Number".into()),
            trait_ref: TraitRefKey {
                name: "Add".into(),
                arguments: vec![Ty::I64],
            },
        };
        let i32_symbol = function_symbol(&trait_method_name(&i32_key, "add"));
        let i64_symbol = function_symbol(&trait_method_name(&i64_key, "add"));
        let ir = compile(&program).expect("large literal must select Add(i64)");
        assert!(ir.contains(&format!("call i64 @{i64_symbol}(")));
        assert!(!ir.contains(&format!("call i64 @{i32_symbol}(")));
    }

    #[test]
    fn add_lowering_is_independent_of_inferred_producer_declaration_order() {
        let program = crate::parser::parse(
            r#"
let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
let Number = struct(value: i32)
extend Number: Add(Number) {
  let Output = Number
  let add(move self)(move rhs: Number): Number = Number(self.value + rhs.value)
}
let main(): i32 = {
  let answer = make() + Number(2)
  answer.value
}
let make() = Number(40)
"#,
        )
        .expect("inferred producer source must parse");
        let key = TraitImplKey {
            self_ty: Ty::Struct("Number".into()),
            trait_ref: TraitRefKey {
                name: "Add".into(),
                arguments: vec![Ty::Struct("Number".into())],
            },
        };
        let symbol = function_symbol(&trait_method_name(&key, "add"));
        let ir = compile(&program).expect("later inferred producer must support overloaded Add");
        assert!(ir.contains(&format!("call %sali.type.4e756d626572 @{symbol}(")));
    }

    #[test]
    fn builtin_add_is_independent_of_inferred_producer_declaration_order() {
        let ir = compile_text(
            r#"
let main(): i32 = make() + 2
let make() = 40
"#,
        )
        .expect("later inferred integer producer must support built-in Add");
        assert!(ir.contains("add i32"));
    }

    #[test]
    fn add_reports_when_no_ambiguous_candidate_has_the_expected_output() {
        let errors = compile_text(
            r#"
let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}
let Number = struct(value: i32)
extend Number: Add(i32) {
  let Output = bool
  let add(move self)(move rhs: i32): bool = false
}
extend Number: Add(i64) {
  let Output = bool
  let add(move self)(move rhs: i64): bool = true
}
let main(): i32 = Number(40) + 2
"#,
        )
        .expect_err("expected output must reject every Add candidate");
        assert!(errors.iter().any(|error| {
            error.message.contains("no matching `Add` implementation")
                && error.message.contains("producing `i32`")
        }));
    }

    #[test]
    fn trait_method_bodies_resolve_concrete_trait_type_substitutions() {
        let ir = compile_text(
            r#"
let Cell(T: type) = struct(value: T)
let Factory(T: type) = trait {
  let Output: type
  let make(borrow self)(move value: T): Output
}
let Maker = struct(seed: i32)
extend Maker: Factory(i32) {
  let Output = Cell(i32)
  let make(borrow self)(move value: i32): Cell(i32) = Cell(T)(value + self.seed)
}
let main(): i32 = {
  let maker = Maker(0)
  maker.make(42).value
}
"#,
        )
        .expect("trait method compile-time substitutions must resolve");
        let instance = NominalInstanceKey {
            kind: NominalKind::Struct,
            template: "Cell".into(),
            arguments: vec![Ty::I32],
        };
        assert!(ir.contains(&hex_name(&nominal_instance_name(&instance))));
    }

    #[test]
    fn inherent_methods_take_precedence_over_trait_candidates() {
        let program = crate::parser::parse(
            r#"
let Answer = trait {
  let answer(borrow self)(): i32
}
let Number = struct(value: i32)
extend Number: Answer {
  let answer(borrow self)(): i32 = 1
}
extend Number {
  let answer(borrow self)(): i32 = self.value
}
let main(): i32 = {
  let number = Number(42)
  number.answer()
}
"#,
        )
        .expect("method precedence source must parse");
        let analyzer = Analyzer::new(&program);
        let trait_key = analyzer.trait_impls.keys().next().unwrap();
        let trait_symbol = function_symbol(&trait_method_name(trait_key, "answer"));
        let inherent_symbol = function_symbol(&inherent_method_name("Number", "answer"));
        let ir = compile(&program).expect("method precedence source must compile");
        assert!(ir.contains(&format!("call i32 @{inherent_symbol}(ptr")));
        assert!(!ir.contains(&format!("call i32 @{trait_symbol}(ptr")));
    }

    #[test]
    fn rejects_unsupported_trait_defaults_gats_and_associated_cycles() {
        let cases = [
            (
                r#"
let Defaulted = trait {
  let value(borrow self)(): i32 = 42
}
let main(): i32 = 0
"#,
                "default trait method",
            ),
            (
                r#"
let Generic = trait {
  let Item(T: type): type
}
let main(): i32 = 0
"#,
                "generic associated type",
            ),
            (
                r#"
let Cycle = trait {
  let A: type
  let B: type
}
let Node = struct(value: i32)
extend Node: Cycle {
  let A = B
  let B = A
}
let main(): i32 = 0
"#,
                "associated type cycle",
            ),
            (
                r#"
let Broken = trait {
  let read(borrow self)(): Missing
}
let main(): i32 = 0
"#,
                "unknown type `Missing`",
            ),
            (
                r#"
let Conflict(T: type) = trait {
  let T: type
}
let main(): i32 = 0
"#,
                "conflicts with a trait type parameter",
            ),
            (
                r#"
let Read = trait {
  let read(borrow self)(): i32
}
let Number = struct(value: i32)
extend Number: Read {
  let read(borrow value: Number)(): i32 = value.value
}
let main(): i32 = 0
"#,
                "signature mismatch",
            ),
            (
                r#"
let Boxed = struct(value: i32)
let InvalidCopy = trait {
  let consume(borrow self)(copy value: Boxed): i32
}
let main(): i32 = 0
"#,
                "requires `Copy`",
            ),
            (
                r#"
let Read = trait {
  let read(borrow self)(): i32
}
let Number = struct(value: i32)
extend Number: Read {}
extend Number: Read {
  let read(borrow self)(): i32 = self.value
}
let main(): i32 = 0
"#,
                "duplicate trait implementation",
            ),
        ];
        for (source, expected) in cases {
            let diagnostics = compile_text(source).expect_err("trait source must be rejected");
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains(expected)),
                "missing `{expected}` in {diagnostics:?}"
            );
        }
    }

    #[test]
    fn substitutes_self_in_associated_function_parameters_and_results() {
        compile_text(
            r#"
let Boxed = struct(value: i32)
extend Boxed {
  let identity(value: Self): Self = value
}
let main(): i32 = Boxed.identity(Boxed(42)).value
"#,
        )
        .unwrap();
    }

    #[test]
    fn keeps_same_named_method_and_associated_function_symbols_distinct() {
        let ir = compile_text(
            r#"
let Number = struct(raw: i32)
extend Number {
  let value(borrow self)(): i32 = self.raw
  let value(): i32 = 2
}
let main(): i32 = {
  let number = Number(40)
  number.value() + Number.value()
}
"#,
        )
        .unwrap();
        let method = function_symbol("Number::method::value");
        let function = function_symbol("Number::function::value");
        assert_ne!(method, function);
        assert!(ir.contains(&format!("@{method}")));
        assert!(ir.contains(&format!("@{function}")));
    }

    #[test]
    fn emits_dynamic_array_bounds_check_before_inbounds_gep_and_hoists_allocas() {
        let ir = compile_text(
            r#"
let read(values: Array(i32, 2), index: i32): i32 = values[index]
let main(): i32 = read([40, 2], 1)
"#,
        )
        .unwrap();
        let function_start = ir.find("define internal i32 @sali.fn.72656164").unwrap();
        let function_tail = &ir[function_start..];
        let function_end = function_tail.find("\n}\n").unwrap() + 3;
        let function = &function_tail[..function_end];
        let bounds = function.find("icmp ult i64").unwrap();
        let trap = function.find("call void @llvm.trap()").unwrap();
        let gep = function.find("getelementptr inbounds").unwrap();
        assert!(bounds < trap && trap < gep);
        assert!(function.rfind("alloca").unwrap() < function.find("br i1").unwrap());
    }

    #[test]
    fn rejects_array_lengths_beyond_the_first_version_limit() {
        let errors = compile_text(
            r#"
let main(): i32 = {
  let values: Array(i32, 2147483648) = [42]
  0
}
"#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| {
            error.message.contains("array length") && error.message.contains("limit")
        }));
    }

    #[test]
    fn rejects_an_outer_move_on_a_loop_backedge_even_for_a_copy_type() {
        let errors = compile_text(
            r#"
let consume(move value: i32): () = ()
let main(): i32 = {
  let value = 42
  while true {
    consume(value)
  }
  0
}
"#,
        )
        .unwrap_err();
        assert!(errors.iter().any(|error| {
            error.message.contains("move") && error.message.contains("loop backedge")
        }));
    }

    #[test]
    fn permits_a_move_that_only_reaches_a_break_exit() {
        compile_text(
            r#"
let Boxed = struct(value: i32)
let consume(move value: Boxed): i32 = value.value
let main(): i32 = {
  let boxed = Boxed(42)
  loop {
    break consume(boxed)
  }
}
"#,
        )
        .unwrap();
    }

    #[test]
    fn nested_breaks_target_the_innermost_loop() {
        let ir = compile_text(
            r#"
let main(): i32 = {
  let mut answer = 40
  loop {
    loop {
      break
    }
    answer = answer + 2
    break answer
  }
}
"#,
        )
        .unwrap();
        assert_eq!(ir.matches("loop.body").count(), 4);
        assert_eq!(ir.matches("loop.end").count(), 4);
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
