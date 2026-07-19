#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(Function),
    Global(Binding),
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
pub enum Expr {
    Unit,
    Integer(i128),
    Bool(bool),
    Name(String),
    Unary(UnaryOp, Box<Expr>),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Assign(String, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Block(Vec<Stmt>, Option<Box<Expr>>),
    Closure(Vec<Param>, Box<Expr>),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    Return(Option<Box<Expr>>),
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
