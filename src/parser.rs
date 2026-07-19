use std::fmt;

use crate::ast::{
    BinaryOp, Binding, Expr, Function, Item, Param, PassMode, Program, Stmt, Type, UnaryOp,
};
use crate::lexer::{lex, LexError, Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self {
            message: error.message,
            line: error.line,
            column: error.column,
        }
    }
}

/// Lexes and parses one Salicin source file.
pub fn parse(source: &str) -> Result<Program, ParseError> {
    parse_tokens(lex(source)?)
}

/// Parses a token stream produced by [`crate::lexer::lex`].
pub fn parse_tokens(tokens: Vec<Token>) -> Result<Program, ParseError> {
    Parser { tokens, index: 0 }.program()
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn program(mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();
        self.skip_separators();

        while !self.at(&TokenKind::Eof) {
            items.push(self.item()?);
            if !self.at(&TokenKind::Eof) && !self.at_separator() {
                return Err(self.error_here("expected a newline or `;` after declaration"));
            }
            self.skip_separators();
        }

        Ok(Program { items })
    }

    fn item(&mut self) -> Result<Item, ParseError> {
        self.expect(&TokenKind::Let, "`let`")?;
        let mutable = self.take(&TokenKind::Mut);
        let name = self.expect_ident("a declaration name")?;

        let mut groups = Vec::new();
        while self.at(&TokenKind::LParen) {
            groups.push(self.parameter_group()?);
            self.take_newlines_if_followed_by(&[
                TokenKind::LParen,
                TokenKind::Colon,
                TokenKind::Equal,
            ]);
        }

        if mutable && !groups.is_empty() {
            return Err(self.error_here("`let mut` cannot declare a function"));
        }

        let annotation = if self.take(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };

        if !groups.is_empty() {
            self.take_newlines_if_followed_by(&[TokenKind::Equal]);
        }

        self.expect(&TokenKind::Equal, "`=`")?;

        if groups.is_empty() {
            let value = self.expression(true)?;
            Ok(Item::Global(Binding {
                mutable,
                name,
                annotation,
                value,
            }))
        } else {
            let body = if self.at(&TokenKind::LBrace) {
                self.block()?
            } else {
                self.expression(true)?
            };
            Ok(Item::Function(Function {
                name,
                groups,
                return_type: annotation,
                body: Some(body),
            }))
        }
    }

    fn local_binding(&mut self) -> Result<Binding, ParseError> {
        self.expect(&TokenKind::Let, "`let`")?;
        let mutable = self.take(&TokenKind::Mut);
        let name = self.expect_ident("a binding name")?;

        if self.at(&TokenKind::LParen) {
            return Err(self.error_here(
                "local named functions are not part of the M0 grammar; bind a closure instead",
            ));
        }

        let annotation = if self.take(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Equal, "`=`")?;
        let value = self.expression(true)?;

        Ok(Binding {
            mutable,
            name,
            annotation,
            value,
        })
    }

    fn parameter_group(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        if self.take(&TokenKind::RParen) {
            return Ok(params);
        }

        loop {
            let mode = if self.take(&TokenKind::Copy) {
                PassMode::Copy
            } else if self.take(&TokenKind::Move) {
                PassMode::Move
            } else if self.take(&TokenKind::Borrow) {
                PassMode::Borrow
            } else if self.take(&TokenKind::Mut) {
                self.expect(&TokenKind::Borrow, "`borrow` after `mut`")?;
                PassMode::MutBorrow
            } else {
                PassMode::Inferred
            };

            let name = self.expect_ident("a parameter name")?;
            self.expect(&TokenKind::Colon, "`:` after parameter name")?;
            let ty = self.type_expr()?;
            params.push(Param { mode, name, ty });

            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RParen) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RParen, "`)`")?;
                break;
            }
        }
        Ok(params)
    }

    fn type_expr(&mut self) -> Result<Type, ParseError> {
        if self.take(&TokenKind::LParen) {
            self.expect(&TokenKind::RParen, "`)` in unit type")?;
            return Ok(Type::Void);
        }

        let name = self.expect_ident("a type")?;
        let mut arguments = Vec::new();
        if self.take(&TokenKind::LParen) && !self.take(&TokenKind::RParen) {
            loop {
                arguments.push(self.type_expr()?);
                if self.take(&TokenKind::Comma) {
                    if self.take(&TokenKind::RParen) {
                        break;
                    }
                } else {
                    self.expect(&TokenKind::RParen, "`)` after type arguments")?;
                    break;
                }
            }
        }

        if arguments.is_empty() {
            Ok(match name.as_str() {
                "i32" => Type::I32,
                "i64" => Type::I64,
                "u32" => Type::U32,
                "u64" => Type::U64,
                "bool" => Type::Bool,
                "void" => Type::Void,
                _ => Type::Named(name, arguments),
            })
        } else {
            Ok(Type::Named(name, arguments))
        }
    }

    fn expression(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        self.assignment(allow_trailing_closure)
    }

    fn assignment(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let left = self.logical_or(allow_trailing_closure)?;
        if self.take(&TokenKind::Equal) {
            let equals = self.previous().clone();
            let right = self.assignment(allow_trailing_closure)?;
            if let Expr::Name(name) = left {
                Ok(Expr::Assign(name, Box::new(right)))
            } else {
                Err(self.error_at(&equals, "left side of assignment must be a name"))
            }
        } else {
            Ok(left)
        }
    }

    fn logical_or(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.logical_and(allow_trailing_closure)?;
        while self.take(&TokenKind::OrOr) {
            let right = self.logical_and(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), BinaryOp::Or, Box::new(right));
        }
        Ok(expression)
    }

    fn logical_and(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.equality(allow_trailing_closure)?;
        while self.take(&TokenKind::AndAnd) {
            let right = self.equality(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), BinaryOp::And, Box::new(right));
        }
        Ok(expression)
    }

    fn equality(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let left = self.relation(allow_trailing_closure)?;
        let operator = if self.take(&TokenKind::EqualEqual) {
            Some(BinaryOp::Eq)
        } else if self.take(&TokenKind::BangEqual) {
            Some(BinaryOp::Ne)
        } else {
            None
        };

        let Some(operator) = operator else {
            return Ok(left);
        };
        let right = self.relation(allow_trailing_closure)?;
        if self.at(&TokenKind::EqualEqual) || self.at(&TokenKind::BangEqual) {
            return Err(self.error_here("equality operators cannot be chained"));
        }
        Ok(Expr::Binary(Box::new(left), operator, Box::new(right)))
    }

    fn relation(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let left = self.additive(allow_trailing_closure)?;
        let operator = if self.take(&TokenKind::Less) {
            Some(BinaryOp::Lt)
        } else if self.take(&TokenKind::LessEqual) {
            Some(BinaryOp::Le)
        } else if self.take(&TokenKind::Greater) {
            Some(BinaryOp::Gt)
        } else if self.take(&TokenKind::GreaterEqual) {
            Some(BinaryOp::Ge)
        } else {
            None
        };

        let Some(operator) = operator else {
            return Ok(left);
        };
        let right = self.additive(allow_trailing_closure)?;
        if matches!(
            self.current().kind,
            TokenKind::Less | TokenKind::LessEqual | TokenKind::Greater | TokenKind::GreaterEqual
        ) {
            return Err(self.error_here("comparison operators cannot be chained"));
        }
        Ok(Expr::Binary(Box::new(left), operator, Box::new(right)))
    }

    fn additive(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.multiplicative(allow_trailing_closure)?;
        loop {
            let operator = if self.take(&TokenKind::Plus) {
                Some(BinaryOp::Add)
            } else if self.take(&TokenKind::Minus) {
                Some(BinaryOp::Sub)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            let right = self.multiplicative(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), operator, Box::new(right));
        }
        Ok(expression)
    }

    fn multiplicative(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.unary(allow_trailing_closure)?;
        loop {
            let operator = if self.take(&TokenKind::Star) {
                Some(BinaryOp::Mul)
            } else if self.take(&TokenKind::Slash) {
                Some(BinaryOp::Div)
            } else if self.take(&TokenKind::Percent) {
                Some(BinaryOp::Rem)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            let right = self.unary(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), operator, Box::new(right));
        }
        Ok(expression)
    }

    fn unary(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        if self.take(&TokenKind::Minus) {
            let operand = self.unary(allow_trailing_closure)?;
            Ok(Expr::Unary(UnaryOp::Neg, Box::new(operand)))
        } else if self.take(&TokenKind::Bang) {
            let operand = self.unary(allow_trailing_closure)?;
            Ok(Expr::Unary(UnaryOp::Not, Box::new(operand)))
        } else {
            self.postfix(allow_trailing_closure)
        }
    }

    fn postfix(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.primary(allow_trailing_closure)?;
        let mut has_call_group = false;
        let mut used_trailing_closure = false;

        loop {
            if self.take(&TokenKind::LParen) {
                let mut arguments = Vec::new();
                if !self.take(&TokenKind::RParen) {
                    loop {
                        arguments.push(self.expression(true)?);
                        if self.take(&TokenKind::Comma) {
                            if self.take(&TokenKind::RParen) {
                                break;
                            }
                        } else {
                            self.expect(&TokenKind::RParen, "`)` after arguments")?;
                            break;
                        }
                    }
                }
                expression = Expr::Call(Box::new(expression), arguments);
                has_call_group = true;
            } else if allow_trailing_closure
                && has_call_group
                && !used_trailing_closure
                && self.at(&TokenKind::LBrace)
            {
                let closure = self.closure()?;
                expression = Expr::Call(Box::new(expression), vec![closure]);
                used_trailing_closure = true;
            } else {
                break;
            }
        }

        Ok(expression)
    }

    fn primary(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Integer(value) => {
                self.advance();
                Ok(Expr::Integer(value))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Expr::Name(name))
            }
            TokenKind::LParen => {
                self.advance();
                if self.take(&TokenKind::RParen) {
                    return Ok(Expr::Unit);
                }
                let expression = self.expression(true)?;
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(expression)
            }
            TokenKind::Do => {
                self.advance();
                self.block()
            }
            TokenKind::If => self.if_expression(),
            TokenKind::Return => self.return_expression(allow_trailing_closure),
            TokenKind::LBrace => self.closure(),
            _ => Err(self.error_at(
                &token,
                format!("expected an expression, found {}", describe(&token.kind)),
            )),
        }
    }

    fn if_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::If, "`if`")?;
        let condition = self.expression(false)?;
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("expected `{` after `if` condition"));
        }
        let then_branch = self.block()?;

        // `else` may begin on the next logical line. If it is absent, restore
        // the newlines so the containing block can still see its separator.
        let after_then = self.index;
        while self.take(&TokenKind::Newline) {}
        let else_branch = if self.take(&TokenKind::Else) {
            if self.at(&TokenKind::If) {
                Some(Box::new(self.if_expression()?))
            } else if self.at(&TokenKind::LBrace) {
                Some(Box::new(self.block()?))
            } else {
                return Err(self.error_here("expected `if` or `{` after `else`"));
            }
        } else {
            self.index = after_then;
            None
        };

        Ok(Expr::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch,
        })
    }

    fn return_expression(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Return, "`return`")?;
        if self.at_separator() || self.at(&TokenKind::RBrace) || self.at(&TokenKind::Eof) {
            Ok(Expr::Return(None))
        } else {
            let value = self.expression(allow_trailing_closure)?;
            Ok(Expr::Return(Some(Box::new(value))))
        }
    }

    fn block(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.block_contents()
    }

    fn block_contents(&mut self) -> Result<Expr, ParseError> {
        let mut statements = Vec::new();
        self.skip_separators();

        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` before end of file"));
            }

            if self.at(&TokenKind::Let) {
                let binding = self.local_binding()?;
                if !self.at_separator() {
                    return Err(self.error_here("expected a newline or `;` after local binding"));
                }
                self.skip_separators();
                statements.push(Stmt::Let(binding));
                continue;
            }

            let expression = self.expression(true)?;
            if self.take(&TokenKind::RBrace) {
                return Ok(Expr::Block(statements, Some(Box::new(expression))));
            }
            if !self.at_separator() {
                return Err(self.error_here("expected a newline, `;`, or `}` after expression"));
            }

            let mut had_semicolon = false;
            while self.at_separator() {
                had_semicolon |= self.take(&TokenKind::Semicolon);
                if !had_semicolon || self.at(&TokenKind::Newline) {
                    self.take(&TokenKind::Newline);
                }
            }

            if self.take(&TokenKind::RBrace) {
                if had_semicolon {
                    statements.push(Stmt::Expr(expression));
                    return Ok(Expr::Block(statements, None));
                }
                return Ok(Expr::Block(statements, Some(Box::new(expression))));
            }
            statements.push(Stmt::Expr(expression));
        }

        self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(Expr::Block(statements, None))
    }

    fn closure(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_separators();
        if self.take(&TokenKind::RBrace) {
            return Ok(Expr::Closure(
                Vec::new(),
                Box::new(Expr::Block(Vec::new(), None)),
            ));
        }

        let mut groups = Vec::new();
        if !self.take(&TokenKind::Arrow) {
            if !self.at(&TokenKind::LParen) {
                return Err(self.error_here("non-empty closure requires `->` and parameters"));
            }
            while self.at(&TokenKind::LParen) {
                groups.push(self.parameter_group()?);
            }
            self.expect(&TokenKind::Arrow, "`->` after closure parameters")?;
        } else {
            groups.push(Vec::new());
        }

        let body = self.block_contents()?;
        let mut expression = body;
        for params in groups.into_iter().rev() {
            expression = Expr::Closure(params, Box::new(expression));
        }
        Ok(expression)
    }

    fn skip_separators(&mut self) {
        while self.at_separator() {
            self.advance();
        }
    }

    /// Consumes logical newlines only when the next token is one of the
    /// explicitly permitted declaration-header continuations. On failure the
    /// parser position is restored, so ordinary expression calls never gain
    /// cross-line postfix behavior.
    fn take_newlines_if_followed_by(&mut self, continuations: &[TokenKind]) -> bool {
        let checkpoint = self.index;
        while self.take(&TokenKind::Newline) {}

        let consumed = self.index != checkpoint;
        let followed_by_continuation = continuations.iter().any(|kind| self.at(kind));
        if consumed && followed_by_continuation {
            true
        } else {
            self.index = checkpoint;
            false
        }
    }

    fn at_separator(&self) -> bool {
        self.at(&TokenKind::Newline) || self.at(&TokenKind::Semicolon)
    }

    fn expect_ident(&mut self, expected: &str) -> Result<String, ParseError> {
        let token = self.current().clone();
        if let TokenKind::Ident(name) = token.kind {
            self.advance();
            Ok(name)
        } else {
            Err(self.error_at(
                &token,
                format!("expected {expected}, found {}", describe(&token.kind)),
            ))
        }
    }

    fn expect(&mut self, kind: &TokenKind, expected: &str) -> Result<(), ParseError> {
        if self.take(kind) {
            Ok(())
        } else {
            Err(self.error_here(format!(
                "expected {expected}, found {}",
                describe(&self.current().kind)
            )))
        }
    }

    fn take(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    fn advance(&mut self) {
        if !self.at(&TokenKind::Eof) {
            self.index += 1;
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        self.error_at(self.current(), message)
    }

    fn error_at(&self, token: &Token, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            line: token.line,
            column: token.column,
        }
    }
}

fn describe(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Let => "`let`",
        TokenKind::Mut => "`mut`",
        TokenKind::Copy => "`copy`",
        TokenKind::Move => "`move`",
        TokenKind::Borrow => "`borrow`",
        TokenKind::Do => "`do`",
        TokenKind::If => "`if`",
        TokenKind::Else => "`else`",
        TokenKind::Return => "`return`",
        TokenKind::True => "`true`",
        TokenKind::False => "`false`",
        TokenKind::Ident(_) => "an identifier",
        TokenKind::Integer(_) => "an integer",
        TokenKind::LParen => "`(`",
        TokenKind::RParen => "`)`",
        TokenKind::LBrace => "`{`",
        TokenKind::RBrace => "`}`",
        TokenKind::Colon => "`:`",
        TokenKind::Comma => "`,`",
        TokenKind::Semicolon => "`;`",
        TokenKind::Newline => "a newline",
        TokenKind::Arrow => "`->`",
        TokenKind::Equal => "`=`",
        TokenKind::EqualEqual => "`==`",
        TokenKind::Bang => "`!`",
        TokenKind::BangEqual => "`!=`",
        TokenKind::Plus => "`+`",
        TokenKind::Minus => "`-`",
        TokenKind::Star => "`*`",
        TokenKind::Slash => "`/`",
        TokenKind::Percent => "`%`",
        TokenKind::Less => "`<`",
        TokenKind::LessEqual => "`<=`",
        TokenKind::Greater => "`>`",
        TokenKind::GreaterEqual => "`>=`",
        TokenKind::AndAnd => "`&&`",
        TokenKind::OrOr => "`||`",
        TokenKind::Eof => "end of file",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_globals_and_curried_functions() {
        let program = parse(
            "let answer: i32 = 40 + 2\n\
             let add(copy x: i32)(y: i32): i32 = { x + y }\n",
        )
        .unwrap();

        assert_eq!(program.items.len(), 2);
        let Item::Function(function) = &program.items[1] else {
            panic!("expected function");
        };
        assert_eq!(function.name, "add");
        assert_eq!(function.groups.len(), 2);
        assert_eq!(function.groups[0][0].mode, PassMode::Copy);
        assert_eq!(function.return_type, Some(Type::I32));
    }

    #[test]
    fn keeps_call_groups_nested() {
        let program = parse("let main(): i32 = add(1)(2)\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Call(inner, second) = function.body.as_ref().unwrap() else {
            panic!("expected outer call");
        };
        assert_eq!(second, &vec![Expr::Integer(2)]);
        assert!(matches!(inner.as_ref(), Expr::Call(_, first) if first == &vec![Expr::Integer(1)]));
    }

    #[test]
    fn allows_newlines_inside_a_named_function_header() {
        let program = parse(
            "let add(x: i32)\n\
               (y: i32)\n\
               : i32\n\
               = x + y\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(function.groups.len(), 2);
        assert_eq!(function.return_type, Some(Type::I32));
    }

    #[test]
    fn newline_does_not_continue_an_expression_call() {
        let program = parse(
            "let main(): i32 = {\n\
               add(1)\n\
               (2)\n\
             }\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Some(Expr::Block(statements, Some(tail))) = &function.body else {
            panic!("expected block");
        };
        assert!(matches!(
            statements.as_slice(),
            [Stmt::Expr(Expr::Call(_, arguments))] if arguments == &vec![Expr::Integer(1)]
        ));
        assert_eq!(tail.as_ref(), &Expr::Integer(2));
    }

    #[test]
    fn parses_local_bindings_assignment_and_block_tail() {
        let program = parse(
            "let main(): i32 = {\n\
               let mut x = 2\n\
               x = x * 3 + 1\n\
               x\n\
             }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Block(statements, Some(tail)) = function.body.as_ref().unwrap() else {
            panic!("expected block with a tail value");
        };
        assert_eq!(statements.len(), 2);
        assert_eq!(tail.as_ref(), &Expr::Name("x".into()));
    }

    #[test]
    fn semicolon_discards_the_last_block_value() {
        let program = parse("let main(): () = { 1; }\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(matches!(function.body, Some(Expr::Block(_, None))));
    }

    #[test]
    fn parses_do_if_else_and_return() {
        let program = parse(
            "let choose(flag: bool): i32 = do {\n\
               if flag { return 1 }\n\
               else { 2 }\n\
             }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(matches!(function.body, Some(Expr::Block(_, _))));
    }

    #[test]
    fn trailing_closure_creates_a_new_call_group() {
        let program = parse("let value = map(items) { (x: i32) -> x + 1 }\n").unwrap();
        let Item::Global(binding) = &program.items[0] else {
            panic!("expected global");
        };
        let Expr::Call(first_call, trailing_group) = &binding.value else {
            panic!("expected trailing call group");
        };
        assert_eq!(trailing_group.len(), 1);
        assert!(matches!(first_call.as_ref(), Expr::Call(_, _)));
    }

    #[test]
    fn reports_a_source_location() {
        let error = parse("let main(): i32 = {\n  let x =\n}\n").unwrap_err();
        assert_eq!((error.line, error.column), (3, 1));
        assert!(error.message.contains("expression"));
    }
}
