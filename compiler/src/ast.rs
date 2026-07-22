#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
    /// Visibility is stored alongside top-level items until module lowering
    /// gives declarations stable module identities.
    pub item_visibilities: Vec<Visibility>,
    /// Source module provenance retained for semantic visibility checks that
    /// cannot be completed by syntactic path resolution (notably trait-method
    /// candidate lookup).
    pub item_origins: Vec<ItemOrigin>,
    pub uses: Vec<UseDecl>,
}

impl Program {
    pub fn new(items: Vec<Item>) -> Self {
        let item_visibilities = vec![Visibility::Private; items.len()];
        let item_origins = vec![ItemOrigin::default(); items.len()];
        Self {
            items,
            item_visibilities,
            item_origins,
            uses: Vec::new(),
        }
    }

    pub fn with_visibilities(items: Vec<Item>, item_visibilities: Vec<Visibility>) -> Self {
        Self::with_uses(items, item_visibilities, Vec::new())
    }

    pub fn with_uses(
        items: Vec<Item>,
        item_visibilities: Vec<Visibility>,
        uses: Vec<UseDecl>,
    ) -> Self {
        let item_origins = vec![ItemOrigin::default(); items.len()];
        Self::with_metadata(items, item_visibilities, item_origins, uses)
    }

    pub fn with_metadata(
        items: Vec<Item>,
        item_visibilities: Vec<Visibility>,
        item_origins: Vec<ItemOrigin>,
        uses: Vec<UseDecl>,
    ) -> Self {
        assert_eq!(
            items.len(),
            item_visibilities.len(),
            "every program item must have a visibility"
        );
        assert_eq!(
            items.len(),
            item_origins.len(),
            "every program item must have source provenance"
        );
        Self {
            items,
            item_visibilities,
            item_origins,
            uses,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ItemOrigin {
    pub package: usize,
    pub module_path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
    pub visibility: Visibility,
    pub path: Vec<String>,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Private,
    Package,
    Public,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(Function),
    Global(Binding),
    Struct(StructDef),
    Enum(EnumDef),
    Effect(EffectDef),
    Access(AccessDef),
    TypeAlias(TypeAliasDef),
    Trait(TraitDef),
    Extend(ExtendDef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub target: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub operations: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessDef {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub members: Vec<TraitMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraitMember {
    Function(Function),
    AssociatedType {
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
        default: Option<Type>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtendDef {
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub target: Type,
    pub trait_ref: Option<Type>,
    pub where_predicates: Vec<WherePredicate>,
    pub members: Vec<ExtendMember>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtendMember {
    Function(Function),
    Const(Binding),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: String,
    pub compile_groups: Vec<Vec<CompileParam>>,
    pub variants: Vec<VariantDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDef {
    pub name: String,
    pub fields: VariantFields,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantFields {
    Unit,
    Positional(Vec<Type>),
    Named(Vec<Field>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub visibility: Visibility,
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// Compile-time groups retain their source grouping but are erased before
    /// runtime calling convention lowering.
    pub compile_groups: Vec<Vec<CompileParam>>,
    /// Parameter groups are retained in the AST so later lowering can implement
    /// partial application without changing the parser.
    pub groups: Vec<Vec<Param>>,
    pub return_type: Option<Type>,
    pub effects: FunctionEffects,
    pub where_predicates: Vec<WherePredicate>,
    pub body: Option<Expr>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct FunctionEffects {
    pub unsafe_effect: bool,
    /// Error type propagated automatically by calls and handled by `try { ... }`.
    pub throws: Option<Box<Type>>,
    /// Nominal user-defined marker effects, canonicalized by module lowering.
    pub custom: Vec<Type>,
    /// Compile-time effect-row parameters awaiting generic instantiation.
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WherePredicate {
    pub subject: Type,
    pub trait_ref: Type,
    pub associated_types: Vec<AssociatedTypeBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssociatedTypeBinding {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileParam {
    pub name: String,
    pub kind: CompileParamKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileParamKind {
    Type,
    Region,
    Access,
    Passing,
    Effect,
    TypeConstructor { parameter_count: usize },
    EffectConstructor { parameter_count: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub mode: PassMode,
    /// An access compile-time parameter used by `borrow(A)` until generic
    /// instantiation selects shared or mutable borrowing.
    pub access: Option<String>,
    /// A `passing` compile-time parameter used in keyword position until
    /// generic instantiation selects auto, copy, or move passing.
    pub passing: Option<String>,
    pub region: Option<String>,
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassMode {
    Inferred,
    Copy,
    Move,
    Borrow,
    MutBorrow,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    I32,
    I64,
    U32,
    U64,
    Bool,
    Unit,
    Borrow {
        mutable: bool,
        access: Option<String>,
        region: Option<String>,
        pointee: Box<Type>,
    },
    Array(Box<Type>, u64),
    Function {
        groups: Vec<Vec<Type>>,
        effects: FunctionEffects,
        result: Box<Type>,
    },
    Named(String, Vec<Type>),
    /// A parsed named type application whose argument labels still need to be
    /// normalized against the constructor's compile-time parameter names.
    NamedArgs(String, Vec<TypeArg>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeArg {
    pub label: Option<String>,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub mutable: bool,
    pub name: String,
    pub annotation: Option<Type>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let(Binding),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallArg {
    pub label: Option<String>,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HandlerChainCall {
    pub scrutinee: Box<Expr>,
    pub payload: String,
    pub error: String,
    pub member: String,
    pub groups: Vec<Vec<CallArg>>,
    pub success: Box<Expr>,
    pub residual: Box<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    Wildcard,
    Integer(i128),
    Bool(bool),
    Binding(String),
    Constructor {
        path: Vec<String>,
        fields: PatternFields,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternFields {
    Unit,
    Positional(Vec<Pattern>),
    Named(Vec<PatternField>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternField {
    pub name: String,
    pub pattern: Pattern,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Unit,
    Integer(i128),
    Bool(bool),
    Name(String),
    Unary(UnaryOp, Box<Expr>),
    Borrow {
        mutable: bool,
        access: Option<String>,
        value: Box<Expr>,
    },
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Coalesce(Box<Expr>, Box<Expr>),
    /// Selective-CPS form of `??`; produced after parsing so the typed
    /// lowering can choose the `Option` or `Result` success pattern.
    HandlerCoalesce {
        scrutinee: Box<Expr>,
        payload: String,
        success: Box<Expr>,
        fallback: Box<Expr>,
    },
    /// Selective-CPS form of a fully applied optional method call. The
    /// typed lowering chooses `Option` or `Result` wrapping after the lazy
    /// success and residual branches have already been transformed.
    HandlerChainCall(Box<HandlerChainCall>),
    Try(Box<Expr>),
    DoBlock {
        body: Box<Expr>,
    },
    Throw(Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    CompoundAssign(Box<Expr>, BinaryOp, Box<Expr>),
    Call(Box<Expr>, Vec<CallArg>),
    Member(Box<Expr>, String),
    ChainMember(Box<Expr>, String),
    Array(Vec<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Block(Vec<Stmt>, Option<Box<Expr>>),
    Unsafe(Box<Expr>),
    Closure(Vec<Param>, Box<Expr>),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Return(Option<Box<Expr>>),
    While {
        condition: Box<Expr>,
        body: Box<Expr>,
    },
    Loop {
        body: Box<Expr>,
    },
    Break(Option<Box<Expr>>),
    Continue,
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    Deref,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
