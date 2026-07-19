use std::fmt;

use crate::ast::{
    BinaryOp, Binding, CallArg, EnumDef, Expr, Field, Function, Item, MatchArm, Param, PassMode,
    Pattern, PatternField, PatternFields, Program, Stmt, StructDef, Type, UnaryOp, VariantDef,
    VariantFields,
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

        if self.at(&TokenKind::Struct) || self.at(&TokenKind::Enum) {
            if mutable || annotation.is_some() || !groups.is_empty() {
                return Err(self
                    .error_here("M1 data declarations cannot be mutable, annotated, or generic"));
            }
            return if self.at(&TokenKind::Struct) {
                self.struct_definition(name).map(Item::Struct)
            } else {
                self.enum_definition(name).map(Item::Enum)
            };
        }

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

    fn struct_definition(&mut self, name: String) -> Result<StructDef, ParseError> {
        self.expect(&TokenKind::Struct, "`struct`")?;
        self.expect(&TokenKind::LParen, "`(` after `struct`")?;
        let fields = self.named_type_fields()?;
        Ok(StructDef { name, fields })
    }

    fn enum_definition(&mut self, name: String) -> Result<EnumDef, ParseError> {
        self.expect(&TokenKind::Enum, "`enum`")?;
        self.expect(&TokenKind::LBrace, "`{` after `enum`")?;
        self.skip_separators();
        let mut variants = Vec::new();

        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` before end of enum declaration"));
            }

            let variant_name = self.expect_ident("a variant name")?;
            let fields = if self.take(&TokenKind::LParen) {
                if self.take(&TokenKind::RParen) {
                    VariantFields::Positional(Vec::new())
                } else if self.ident_followed_by_colon() {
                    VariantFields::Named(self.named_type_fields_after_open()?)
                } else {
                    let mut types = Vec::new();
                    loop {
                        types.push(self.type_expr()?);
                        if self.take(&TokenKind::Comma) {
                            if self.take(&TokenKind::RParen) {
                                break;
                            }
                        } else {
                            self.expect(&TokenKind::RParen, "`)` after variant fields")?;
                            break;
                        }
                    }
                    VariantFields::Positional(types)
                }
            } else {
                VariantFields::Unit
            };
            variants.push(VariantDef {
                name: variant_name,
                fields,
            });

            if self.take(&TokenKind::Comma) {
                self.skip_separators();
                continue;
            }

            self.skip_separators();
            if !self.at(&TokenKind::RBrace) {
                return Err(self.error_here("expected `,` between enum variants"));
            }
        }

        self.expect(&TokenKind::RBrace, "`}` after enum variants")?;
        Ok(EnumDef { name, variants })
    }

    fn named_type_fields(&mut self) -> Result<Vec<Field>, ParseError> {
        if self.take(&TokenKind::RParen) {
            return Ok(Vec::new());
        }
        self.named_type_fields_after_open()
    }

    fn named_type_fields_after_open(&mut self) -> Result<Vec<Field>, ParseError> {
        let mut fields = Vec::new();
        loop {
            let name = self.expect_ident("a field name")?;
            self.expect(&TokenKind::Colon, "`:` after field name")?;
            fields.push(Field {
                name,
                ty: self.type_expr()?,
            });
            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RParen) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RParen, "`)` after fields")?;
                break;
            }
        }
        Ok(fields)
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
        let left = self.match_expression(allow_trailing_closure)?;
        if self.take(&TokenKind::Equal) {
            let equals = self.previous().clone();
            let right = self.assignment(allow_trailing_closure)?;
            if Self::is_assignable_place(&left) {
                Ok(Expr::Assign(Box::new(left), Box::new(right)))
            } else {
                Err(self.error_at(
                    &equals,
                    "left side of assignment must be a name or member chain",
                ))
            }
        } else {
            Ok(left)
        }
    }

    fn match_expression(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let scrutinee = self.logical_or(allow_trailing_closure)?;
        if !self.take(&TokenKind::Match) {
            return Ok(scrutinee);
        }

        self.expect(&TokenKind::LBrace, "`{` after `match`")?;
        self.skip_separators();
        let mut arms = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` before end of match expression"));
            }

            let pattern = self.pattern()?;
            let guard = if self.take(&TokenKind::If) {
                Some(self.expression(true)?)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow, "`=>` after match pattern")?;
            let body = self.expression(true)?;
            arms.push(MatchArm {
                pattern,
                guard,
                body,
            });

            if self.take(&TokenKind::Comma) {
                self.skip_separators();
                continue;
            }

            self.skip_separators();
            if !self.at(&TokenKind::RBrace) {
                return Err(self.error_here("expected `,` between match arms"));
            }
        }
        self.expect(&TokenKind::RBrace, "`}` after match arms")?;

        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        })
    }

    fn pattern(&mut self) -> Result<Pattern, ParseError> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Integer(value) => {
                self.advance();
                Ok(Pattern::Integer(value))
            }
            TokenKind::Minus => {
                self.advance();
                let integer = self.current().clone();
                let TokenKind::Integer(value) = integer.kind else {
                    return Err(self.error_at(&integer, "expected integer literal after `-`"));
                };
                self.advance();
                let value = value.checked_neg().ok_or_else(|| {
                    self.error_at(&token, "negative integer pattern is out of range")
                })?;
                Ok(Pattern::Integer(value))
            }
            TokenKind::True => {
                self.advance();
                Ok(Pattern::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Pattern::Bool(false))
            }
            TokenKind::Ident(name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            TokenKind::Ident(_) => self.named_pattern(),
            _ => Err(self.error_at(
                &token,
                format!("expected a pattern, found {}", describe(&token.kind)),
            )),
        }
    }

    fn named_pattern(&mut self) -> Result<Pattern, ParseError> {
        let mut path = vec![self.expect_ident("a pattern name")?];
        while self.take(&TokenKind::Dot) {
            path.push(self.expect_ident("a name after `.`")?);
        }

        let has_payload = self.at(&TokenKind::LParen);
        let looks_like_constructor =
            path.len() > 1 || has_payload || path[0].chars().next().is_some_and(char::is_uppercase);
        if !looks_like_constructor {
            return Ok(Pattern::Binding(path.pop().expect("path has one element")));
        }

        let fields = if self.take(&TokenKind::LParen) {
            if self.take(&TokenKind::RParen) {
                PatternFields::Positional(Vec::new())
            } else if self.ident_followed_by_colon() {
                let mut fields = Vec::new();
                loop {
                    let name = self.expect_ident("a pattern field name")?;
                    self.expect(&TokenKind::Colon, "`:` after pattern field name")?;
                    fields.push(PatternField {
                        name,
                        pattern: self.pattern()?,
                    });
                    if self.take(&TokenKind::Comma) {
                        if self.take(&TokenKind::RParen) {
                            break;
                        }
                    } else {
                        self.expect(&TokenKind::RParen, "`)` after pattern fields")?;
                        break;
                    }
                }
                PatternFields::Named(fields)
            } else {
                let mut patterns = Vec::new();
                loop {
                    patterns.push(self.pattern()?);
                    if self.take(&TokenKind::Comma) {
                        if self.take(&TokenKind::RParen) {
                            break;
                        }
                    } else {
                        self.expect(&TokenKind::RParen, "`)` after patterns")?;
                        break;
                    }
                }
                PatternFields::Positional(patterns)
            }
        } else {
            PatternFields::Unit
        };

        Ok(Pattern::Constructor { path, fields })
    }

    fn is_assignable_place(expression: &Expr) -> bool {
        match expression {
            Expr::Name(_) => true,
            Expr::Member(base, _) => Self::is_assignable_place(base),
            _ => false,
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
        } else if self.take(&TokenKind::Borrow) {
            let borrow = self.previous().clone();
            self.borrow_expression(false, &borrow, allow_trailing_closure)
        } else if self.take(&TokenKind::Mut) {
            let mutable = self.previous().clone();
            self.expect(&TokenKind::Borrow, "`borrow` after `mut`")?;
            self.borrow_expression(true, &mutable, allow_trailing_closure)
        } else {
            self.postfix(allow_trailing_closure)
        }
    }

    fn borrow_expression(
        &mut self,
        mutable: bool,
        operator: &Token,
        allow_trailing_closure: bool,
    ) -> Result<Expr, ParseError> {
        let value = self.unary(allow_trailing_closure)?;
        if !Self::is_assignable_place(&value) {
            return Err(self.error_at(operator, "borrow operand must be a name or member chain"));
        }
        Ok(Expr::Borrow {
            mutable,
            value: Box::new(value),
        })
    }

    fn postfix(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.primary(allow_trailing_closure)?;
        let mut has_call_group = false;
        let mut used_trailing_closure = false;

        loop {
            if self.take(&TokenKind::LParen) {
                let mut arguments = Vec::new();
                let mut labeled = None;
                if !self.take(&TokenKind::RParen) {
                    loop {
                        let argument_start = self.current().clone();
                        let label = if self.ident_followed_by_colon() {
                            let label = self.expect_ident("an argument label")?;
                            self.expect(&TokenKind::Colon, "`:` after argument label")?;
                            Some(label)
                        } else {
                            None
                        };
                        let is_labeled = label.is_some();
                        if let Some(expected_labeled) = labeled {
                            if expected_labeled != is_labeled {
                                return Err(self.error_at(
                                    &argument_start,
                                    "labeled and positional arguments cannot be mixed",
                                ));
                            }
                        } else {
                            labeled = Some(is_labeled);
                        }
                        arguments.push(CallArg {
                            label,
                            value: self.expression(true)?,
                        });
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
            } else if self.take(&TokenKind::Dot) {
                let member = self.expect_ident("a member name after `.`")?;
                expression = Expr::Member(Box::new(expression), member);
            } else if allow_trailing_closure
                && has_call_group
                && !used_trailing_closure
                && self.at(&TokenKind::LBrace)
            {
                let closure = self.closure()?;
                expression = Expr::Call(
                    Box::new(expression),
                    vec![CallArg {
                        label: None,
                        value: closure,
                    }],
                );
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

    fn ident_followed_by_colon(&self) -> bool {
        matches!(self.current().kind, TokenKind::Ident(_)) && self.at_offset(1, &TokenKind::Colon)
    }

    fn at_offset(&self, offset: usize, kind: &TokenKind) -> bool {
        self.tokens.get(self.index + offset).is_some_and(|token| {
            std::mem::discriminant(&token.kind) == std::mem::discriminant(kind)
        })
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
        TokenKind::Struct => "`struct`",
        TokenKind::Enum => "`enum`",
        TokenKind::Match => "`match`",
        TokenKind::True => "`true`",
        TokenKind::False => "`false`",
        TokenKind::Ident(_) => "an identifier",
        TokenKind::Integer(_) => "an integer",
        TokenKind::LParen => "`(`",
        TokenKind::RParen => "`)`",
        TokenKind::LBrace => "`{`",
        TokenKind::RBrace => "`}`",
        TokenKind::Colon => "`:`",
        TokenKind::Dot => "`.`",
        TokenKind::Comma => "`,`",
        TokenKind::Semicolon => "`;`",
        TokenKind::Newline => "a newline",
        TokenKind::Arrow => "`->`",
        TokenKind::FatArrow => "`=>`",
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
        assert!(matches!(
            second.as_slice(),
            [CallArg {
                label: None,
                value: Expr::Integer(2)
            }]
        ));
        assert!(matches!(
            inner.as_ref(),
            Expr::Call(_, first)
                if matches!(
                    first.as_slice(),
                    [CallArg {
                        label: None,
                        value: Expr::Integer(1)
                    }]
                )
        ));
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
            [Stmt::Expr(Expr::Call(_, arguments))]
                if matches!(
                    arguments.as_slice(),
                    [CallArg {
                        label: None,
                        value: Expr::Integer(1)
                    }]
                )
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
    fn parses_structs_and_enum_field_shapes() {
        let program = parse(
            "let Point = struct(x: i32, y: i32)\n\
             let Shape = enum {\n\
               Circle(radius: i32),\n\
               Pair(i32, i32),\n\
               Unit,\n\
             }\n",
        )
        .unwrap();

        let Item::Struct(point) = &program.items[0] else {
            panic!("expected struct");
        };
        assert_eq!(point.name, "Point");
        assert_eq!(point.fields.len(), 2);

        let Item::Enum(shape) = &program.items[1] else {
            panic!("expected enum");
        };
        assert_eq!(shape.variants.len(), 3);
        assert!(matches!(shape.variants[0].fields, VariantFields::Named(_)));
        assert!(matches!(
            shape.variants[1].fields,
            VariantFields::Positional(_)
        ));
        assert_eq!(shape.variants[2].fields, VariantFields::Unit);
    }

    #[test]
    fn parses_labeled_construction_member_access_and_assignment() {
        let program = parse(
            "let Point = struct(x: i32, y: i32)\n\
             let main(): i32 = {\n\
               let mut point = Point(x: 1, y: 2)\n\
               point.x = 3\n\
               point.x\n\
             }\n",
        )
        .unwrap();

        let Item::Function(main) = &program.items[1] else {
            panic!("expected function");
        };
        let Some(Expr::Block(statements, Some(tail))) = &main.body else {
            panic!("expected block");
        };
        let Stmt::Let(binding) = &statements[0] else {
            panic!("expected binding");
        };
        assert!(matches!(
            &binding.value,
            Expr::Call(_, arguments)
                if arguments.iter().map(|argument| argument.label.as_deref()).collect::<Vec<_>>()
                    == vec![Some("x"), Some("y")]
        ));
        assert!(matches!(
            &statements[1],
            Stmt::Expr(Expr::Assign(left, right))
                if matches!(left.as_ref(), Expr::Member(_, field) if field == "x")
                    && right.as_ref() == &Expr::Integer(3)
        ));
        assert!(matches!(tail.as_ref(), Expr::Member(_, field) if field == "x"));
    }

    #[test]
    fn parses_postfix_match_patterns_and_guards() {
        let program = parse(
            "let classify(shape: Shape): i32 = shape match {\n\
               Shape.Circle(radius: value) if value > 0 => value,\n\
               Shape.Unit => 0,\n\
               _ => -1,\n\
             }\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Some(Expr::Match { scrutinee, arms }) = &function.body else {
            panic!("expected match");
        };
        assert_eq!(scrutinee.as_ref(), &Expr::Name("shape".into()));
        assert_eq!(arms.len(), 3);
        assert!(arms[0].guard.is_some());
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Constructor { path, fields: PatternFields::Named(fields) }
                if path == &vec!["Shape".to_owned(), "Circle".to_owned()]
                    && fields[0].name == "radius"
        ));
        assert_eq!(arms[2].pattern, Pattern::Wildcard);
    }

    #[test]
    fn rejects_mixed_labeled_and_positional_arguments() {
        let error = parse("let point = Point(x: 1, 2)\n").unwrap_err();
        assert!(error.message.contains("cannot be mixed"));
    }

    #[test]
    fn parses_shared_and_mutable_borrow_places() {
        let program = parse(
            "let main(): () = {\n\
               let shared = borrow value.field\n\
               let exclusive = mut borrow value\n\
             }\n",
        )
        .unwrap();

        let Item::Function(main) = &program.items[0] else {
            panic!("expected function");
        };
        let Some(Expr::Block(statements, None)) = &main.body else {
            panic!("expected block");
        };
        assert!(matches!(
            &statements[0],
            Stmt::Let(Binding {
                value: Expr::Borrow {
                    mutable: false,
                    value,
                },
                ..
            }) if matches!(value.as_ref(), Expr::Member(_, field) if field == "field")
        ));
        assert!(matches!(
            &statements[1],
            Stmt::Let(Binding {
                value: Expr::Borrow {
                    mutable: true,
                    value,
                },
                ..
            }) if value.as_ref() == &Expr::Name("value".into())
        ));
    }

    #[test]
    fn rejects_borrowing_a_non_place_expression() {
        let error = parse("let invalid = borrow make()\n").unwrap_err();
        assert!(error.message.contains("name or member chain"));
    }

    #[test]
    fn reports_a_source_location() {
        let error = parse("let main(): i32 = {\n  let x =\n}\n").unwrap_err();
        assert_eq!((error.line, error.column), (3, 1));
        assert!(error.message.contains("expression"));
    }
}
