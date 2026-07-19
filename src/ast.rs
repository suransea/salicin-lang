#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(Function),
    Global(Binding),
    Struct(StructDef),
    Enum(EnumDef),
    Extend(ExtendDef),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtendDef {
    pub target: Type,
    pub trait_ref: Option<Type>,
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
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
    pub name: String,
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
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    /// Parameter groups are retained in the AST so later lowering can implement
    /// partial application without changing the parser.
    pub groups: Vec<Vec<Param>>,
    pub return_type: Option<Type>,
    pub body: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub mode: PassMode,
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassMode {
    Inferred,
    Copy,
    Move,
    Borrow,
    MutBorrow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    I32,
    I64,
    U32,
    U64,
    Bool,
    Void,
    Array(Box<Type>, u64),
    Named(String, Vec<Type>),
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
        value: Box<Expr>,
    },
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Assign(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<CallArg>),
    Member(Box<Expr>, String),
    Array(Vec<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Block(Vec<Stmt>, Option<Box<Expr>>),
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
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
