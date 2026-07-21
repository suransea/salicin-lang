use std::{collections::HashSet, fmt};

use crate::ast::{
    AssociatedTypeBinding, BinaryOp, Binding, CallArg, CompileParam, CompileParamKind, EnumDef,
    Expr, ExtendDef, ExtendMember, Field, Function, Item, MatchArm, Param, PassMode, Pattern,
    PatternField, PatternFields, Program, Stmt, StructDef, TraitDef, TraitMember, Type, UnaryOp,
    UseDecl, VariantDef, VariantFields, Visibility, WherePredicate,
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

enum HeaderGroup {
    Compile(Vec<CompileParam>),
    Runtime(Vec<Param>),
}

type DeclarationGroups = (Vec<Vec<CompileParam>>, Vec<Vec<Param>>);

impl Parser {
    fn program(mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();
        let mut item_visibilities = Vec::new();
        let mut uses = Vec::new();
        self.skip_separators();

        while !self.at(&TokenKind::Eof) {
            let visibility = self.visibility()?;
            if self.at(&TokenKind::Use) {
                uses.extend(self.use_declaration(visibility)?);
            } else {
                if visibility != Visibility::Private && self.at(&TokenKind::Extend) {
                    return Err(self.error_here("`extend` declarations cannot have visibility"));
                }
                items.push(self.item()?);
                item_visibilities.push(visibility);
            }
            if !self.at(&TokenKind::Eof) && !self.at_separator() {
                return Err(self.error_here("expected a newline or `;` after declaration"));
            }
            self.skip_separators();
        }

        if let Err(message) = validate_region_scopes(&items) {
            return Err(self.error_here(message));
        }
        Ok(Program::with_uses(items, item_visibilities, uses))
    }

    fn visibility(&mut self) -> Result<Visibility, ParseError> {
        if !self.take(&TokenKind::Pub) {
            return Ok(Visibility::Private);
        }
        if !self.take(&TokenKind::LParen) {
            return Ok(Visibility::Public);
        }

        self.expect(&TokenKind::Package, "`package` in visibility")?;
        self.expect(&TokenKind::RParen, "`)` after package visibility")?;
        Ok(Visibility::Package)
    }

    fn use_declaration(&mut self, visibility: Visibility) -> Result<Vec<UseDecl>, ParseError> {
        self.expect(&TokenKind::Use, "`use`")?;
        let mut path = vec![self.expect_path_start("an import path")?];

        while self.take(&TokenKind::Dot) {
            if self.take(&TokenKind::LBrace) {
                return self.use_group(visibility, path);
            }
            let segment =
                self.expect_path_continuation(&path, "an import path segment after `.`")?;
            path.push(segment);
        }

        let alias = if self.take(&TokenKind::As) {
            Some(self.expect_import_alias()?)
        } else {
            None
        };
        if alias.is_none()
            && path
                .last()
                .is_some_and(|binding| matches!(binding.as_str(), "self" | "root" | "super" | "_"))
        {
            return Err(self.error_here(format!(
                "import path `{}` requires an explicit usable alias",
                path.join(".")
            )));
        }
        Ok(vec![UseDecl {
            visibility,
            path,
            alias,
        }])
    }

    fn use_group(
        &mut self,
        visibility: Visibility,
        prefix: Vec<String>,
    ) -> Result<Vec<UseDecl>, ParseError> {
        self.skip_newlines();
        if self.at(&TokenKind::RBrace) {
            return Err(self.error_here("import groups cannot be empty"));
        }

        let mut declarations = Vec::new();
        let mut bindings = HashSet::new();
        loop {
            let member = self.expect_relative_path_segment("an import name")?;
            let alias = if self.take(&TokenKind::As) {
                Some(self.expect_import_alias()?)
            } else {
                None
            };
            if alias.is_none() && member == "self" {
                return Err(self.error_here("import name `self` requires an explicit usable alias"));
            }
            let binding = alias.as_deref().unwrap_or(&member);
            if !bindings.insert(binding.to_owned()) {
                return Err(self.error_here(format!(
                    "duplicate import binding `{binding}` in import group"
                )));
            }

            let mut path = prefix.clone();
            path.push(member);
            declarations.push(UseDecl {
                visibility,
                path,
                alias,
            });

            if self.take(&TokenKind::Comma) {
                self.skip_newlines();
                if self.take(&TokenKind::RBrace) {
                    break;
                }
            } else {
                self.skip_newlines();
                self.expect(&TokenKind::RBrace, "`}` after import group")?;
                break;
            }
        }
        Ok(declarations)
    }

    fn item(&mut self) -> Result<Item, ParseError> {
        if self.at(&TokenKind::Let) {
            self.let_item()
        } else if self.at(&TokenKind::Extend) {
            self.extend_definition().map(Item::Extend)
        } else {
            Err(self.error_here(format!(
                "expected `let` or `extend`, found {}",
                describe(&self.current().kind)
            )))
        }
    }

    fn let_item(&mut self) -> Result<Item, ParseError> {
        self.expect(&TokenKind::Let, "`let`")?;
        let mutable = self.take(&TokenKind::Mut);
        let name = self.expect_ident("a declaration name")?;

        let (compile_groups, groups) = self.declaration_groups(false)?;

        if mutable && (!compile_groups.is_empty() || !groups.is_empty()) {
            return Err(self.error_here("`let mut` cannot declare a function"));
        }

        let annotation = if self.take(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };

        if !compile_groups.is_empty() || !groups.is_empty() {
            self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::Equal]);
        }

        let where_predicates = self.where_clause()?;
        if !where_predicates.is_empty() && compile_groups.is_empty() {
            return Err(self.error_here("`where` requires compile-time parameters"));
        }
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);

        self.expect(&TokenKind::Equal, "`=`")?;

        if self.at(&TokenKind::Struct) || self.at(&TokenKind::Enum) || self.at(&TokenKind::Trait) {
            if mutable || annotation.is_some() || !groups.is_empty() || !where_predicates.is_empty()
            {
                return Err(self.error_here(
                    "data declarations cannot be mutable, annotated, or have runtime parameters",
                ));
            }
            return if self.at(&TokenKind::Struct) {
                self.struct_definition(name, compile_groups)
                    .map(Item::Struct)
            } else if self.at(&TokenKind::Enum) {
                self.enum_definition(name, compile_groups).map(Item::Enum)
            } else {
                self.trait_definition(name, compile_groups).map(Item::Trait)
            };
        }

        if compile_groups.is_empty() && groups.is_empty() {
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
                compile_groups,
                groups,
                return_type: annotation,
                where_predicates,
                body: Some(body),
            }))
        }
    }

    fn extend_definition(&mut self) -> Result<ExtendDef, ParseError> {
        self.expect(&TokenKind::Extend, "`extend`")?;
        let (compile_groups, runtime_groups) = self.declaration_groups(false)?;
        if !runtime_groups.is_empty() {
            return Err(self.error_here(
                "extend headers accept only compile-time parameters before the target type",
            ));
        }
        if compile_groups.len() > 1 {
            return Err(
                self.error_here("extend headers support exactly one compile-time parameter group")
            );
        }
        let target = self.type_expr()?;
        let trait_ref = if self.take(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::LBrace]);
        let where_predicates = self.where_clause()?;
        if !where_predicates.is_empty() && compile_groups.is_empty() {
            return Err(self.error_here("extension `where` requires compile-time parameters"));
        }
        self.take_newlines_if_followed_by(&[TokenKind::LBrace]);
        self.expect(&TokenKind::LBrace, "`{` after extend target")?;
        self.skip_separators();

        let mut members = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` before end of extend declaration"));
            }
            members.push(self.extend_member()?);
            if !self.at(&TokenKind::RBrace) && !self.at_separator() {
                return Err(self.error_here("expected a newline or `;` after extend member"));
            }
            self.skip_separators();
        }
        self.expect(&TokenKind::RBrace, "`}` after extend members")?;

        Ok(ExtendDef {
            compile_groups,
            target,
            trait_ref,
            where_predicates,
            members,
        })
    }

    fn extend_member(&mut self) -> Result<ExtendMember, ParseError> {
        if self.at(&TokenKind::Pub) {
            return Err(self.error_here("visibility on extend members is not supported yet"));
        }
        self.expect(&TokenKind::Let, "`let` in extend body")?;
        if self.take(&TokenKind::Mut) {
            let mutable = self.previous().clone();
            return Err(self.error_at(&mutable, "extend members cannot be declared with `let mut`"));
        }
        let name = self.expect_ident("an extend member name")?;

        let (compile_groups, groups) = self.declaration_groups(true)?;
        self.validate_receiver_groups(&groups)?;

        let annotation = if self.take(&TokenKind::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        if !compile_groups.is_empty() || !groups.is_empty() {
            self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::Equal]);
        }
        let where_predicates = self.where_clause()?;
        if !where_predicates.is_empty() {
            return Err(self.error_here("where clauses on extend members are not supported yet"));
        }
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);
        self.expect(&TokenKind::Equal, "`=` in extend member")?;

        if self.at(&TokenKind::Struct) || self.at(&TokenKind::Enum) || self.at(&TokenKind::Trait) {
            return Err(self.error_here("data declarations are not allowed in extend bodies"));
        }

        if compile_groups.is_empty() && groups.is_empty() {
            Ok(ExtendMember::Const(Binding {
                mutable: false,
                name,
                annotation,
                value: self.expression(true)?,
            }))
        } else {
            let body = if self.at(&TokenKind::LBrace) {
                self.block()?
            } else {
                self.expression(true)?
            };
            Ok(ExtendMember::Function(Function {
                name,
                compile_groups,
                groups,
                return_type: annotation,
                where_predicates,
                body: Some(body),
            }))
        }
    }

    fn where_clause(&mut self) -> Result<Vec<WherePredicate>, ParseError> {
        if !self.take(&TokenKind::Where) {
            return Ok(Vec::new());
        }
        let mut predicates = Vec::new();
        loop {
            let subject = self.type_expr()?;
            self.expect(&TokenKind::Colon, "`:` in where predicate")?;
            let (trait_ref, associated_types) = self.where_trait_ref()?;
            predicates.push(WherePredicate {
                subject,
                trait_ref,
                associated_types,
            });
            if !self.take(&TokenKind::Comma) {
                break;
            }
            while self.take(&TokenKind::Newline) {}
            if self.at(&TokenKind::Equal) || self.at(&TokenKind::LBrace) {
                break;
            }
        }
        Ok(predicates)
    }

    fn where_trait_ref(&mut self) -> Result<(Type, Vec<AssociatedTypeBinding>), ParseError> {
        let mut path = vec![self.expect_path_start("a trait")?];
        while self.take(&TokenKind::Dot) {
            path.push(self.expect_path_continuation(&path, "a trait path segment after `.`")?);
        }
        let name = path.join(".");
        let mut arguments = Vec::new();
        let mut associated_types = Vec::new();
        let mut saw_associated = false;
        if self.take(&TokenKind::LParen) && !self.take(&TokenKind::RParen) {
            loop {
                if matches!(self.current().kind, TokenKind::Ident(_))
                    && self.at_offset(1, &TokenKind::Equal)
                {
                    saw_associated = true;
                    let binding = self.expect_ident("an associated type name")?;
                    self.expect(&TokenKind::Equal, "`=` in associated type equality")?;
                    associated_types.push(AssociatedTypeBinding {
                        name: binding,
                        ty: self.type_expr()?,
                    });
                } else {
                    if saw_associated {
                        return Err(self.error_here(
                            "positional trait arguments must precede associated type equalities",
                        ));
                    }
                    arguments.push(self.type_expr()?);
                }
                if self.take(&TokenKind::Comma) {
                    if self.take(&TokenKind::RParen) {
                        break;
                    }
                } else {
                    self.expect(&TokenKind::RParen, "`)` after trait arguments")?;
                    break;
                }
            }
        }
        Ok((Type::Named(name, arguments), associated_types))
    }

    fn declaration_groups(
        &mut self,
        allow_receiver: bool,
    ) -> Result<DeclarationGroups, ParseError> {
        let mut compile_groups: Vec<Vec<CompileParam>> = Vec::new();
        let mut runtime_groups = Vec::new();
        let mut saw_runtime_group = false;

        while self.at(&TokenKind::LParen) {
            if saw_runtime_group && self.group_starts_with_compile_parameter() {
                return Err(self.error_here(
                    "compile-time parameter groups must precede runtime parameter groups",
                ));
            }
            let group = if self.group_starts_with_compile_parameter() {
                self.compile_parameter_group().map(HeaderGroup::Compile)?
            } else {
                let passing_parameters = compile_groups
                    .iter()
                    .flatten()
                    .filter(|parameter| parameter.kind == CompileParamKind::Passing)
                    .map(|parameter| parameter.name.clone())
                    .collect::<HashSet<_>>();
                HeaderGroup::Runtime(
                    self.runtime_parameter_group(allow_receiver, &passing_parameters)?,
                )
            };
            match group {
                HeaderGroup::Compile(params) => {
                    compile_groups.push(params);
                }
                HeaderGroup::Runtime(params) => {
                    saw_runtime_group = true;
                    runtime_groups.push(params);
                }
            }
            self.take_newlines_if_followed_by(&[
                TokenKind::LParen,
                TokenKind::Colon,
                TokenKind::Equal,
            ]);
        }

        Ok((compile_groups, runtime_groups))
    }

    fn group_starts_with_compile_parameter(&self) -> bool {
        self.at(&TokenKind::LParen)
            && matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(_)) | Some(TokenKind::RegionName(_))
            )
            && self.at_offset(2, &TokenKind::Colon)
            && (self.at_offset(3, &TokenKind::Type)
                || self.at_offset(3, &TokenKind::Region)
                || matches!(
                    self.tokens.get(self.index + 3).map(|token| &token.kind),
                    Some(TokenKind::Ident(name)) if matches!(name.as_str(), "access" | "passing")
                ))
    }

    fn current_starts_compile_parameter(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Ident(_) | TokenKind::RegionName(_)
        ) && self.at_offset(1, &TokenKind::Colon)
            && (self.at_offset(2, &TokenKind::Type)
                || self.at_offset(2, &TokenKind::Region)
                || matches!(
                    self.tokens.get(self.index + 2).map(|token| &token.kind),
                    Some(TokenKind::Ident(name)) if matches!(name.as_str(), "access" | "passing")
                ))
    }

    fn compile_parameter_group(&mut self) -> Result<Vec<CompileParam>, ParseError> {
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();

        loop {
            if !self.current_starts_compile_parameter() {
                return Err(self.error_here(
                    "compile-time and runtime parameters cannot be mixed in one group",
                ));
            }
            let name_token = self.current().clone();
            let (name, region_name) = match self.current().kind.clone() {
                TokenKind::Ident(name) => {
                    self.advance();
                    (name, false)
                }
                TokenKind::RegionName(name) => {
                    self.advance();
                    (name, true)
                }
                _ => unreachable!("compile parameter start was checked"),
            };
            self.expect(&TokenKind::Colon, "`:` after compile-time parameter name")?;
            let kind = if self.take(&TokenKind::Type) {
                if region_name {
                    return Err(self.error_at(
                        &name_token,
                        "region names must use the `region` compile-time kind",
                    ));
                }
                if matches!(
                    name.as_str(),
                    "_" | "i32" | "i64" | "u32" | "u64" | "bool" | "void" | "never"
                ) {
                    return Err(self.error_at(
                        &name_token,
                        format!(
                            "reserved type name `{name}` cannot be used as a compile-time parameter"
                        ),
                    ));
                }
                CompileParamKind::Type
            } else if matches!(&self.current().kind, TokenKind::Ident(name) if name == "access") {
                if region_name {
                    return Err(self.error_at(
                        &name_token,
                        "access parameter names must be ordinary identifiers",
                    ));
                }
                self.advance();
                CompileParamKind::Access
            } else if matches!(&self.current().kind, TokenKind::Ident(name) if name == "passing") {
                if region_name {
                    return Err(self.error_at(
                        &name_token,
                        "passing parameter names must be ordinary identifiers",
                    ));
                }
                self.advance();
                CompileParamKind::Passing
            } else {
                self.expect(
                    &TokenKind::Region,
                    "`type`, `access`, `passing`, or `region`",
                )?;
                if !region_name {
                    return Err(self.error_at(
                        &name_token,
                        "region compile-time parameters must start with `'`",
                    ));
                }
                CompileParamKind::Region
            };
            params.push(CompileParam { name, kind });

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

    fn runtime_parameter_group(
        &mut self,
        allow_receiver: bool,
        passing_parameters: &HashSet<String>,
    ) -> Result<Vec<Param>, ParseError> {
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        if self.take(&TokenKind::RParen) {
            return Ok(params);
        }

        loop {
            let passing = match &self.current().kind {
                TokenKind::Ident(name)
                    if passing_parameters.contains(name)
                        && matches!(
                            self.tokens.get(self.index + 1).map(|token| &token.kind),
                            Some(TokenKind::Ident(_))
                        ) =>
                {
                    let name = name.clone();
                    self.advance();
                    Some(name)
                }
                _ => None,
            };
            let (mode, access, region) = if passing.is_some() {
                (PassMode::Inferred, None, None)
            } else if self.take(&TokenKind::Copy) {
                (PassMode::Copy, None, None)
            } else if self.take(&TokenKind::Move) {
                (PassMode::Move, None, None)
            } else if self.take(&TokenKind::Borrow) {
                let (mutable, access, region) = self.optional_borrow_arguments()?;
                (
                    if mutable {
                        PassMode::MutBorrow
                    } else {
                        PassMode::Borrow
                    },
                    access,
                    region,
                )
            } else {
                (PassMode::Inferred, None, None)
            };

            if self.current_starts_compile_parameter() {
                return Err(self.error_here(
                    "compile-time and runtime parameters cannot be mixed in one group",
                ));
            }

            let name = self.expect_ident(if allow_receiver {
                "a parameter name or `self`"
            } else {
                "a parameter name"
            })?;
            let ty = if name == "self" {
                if !allow_receiver {
                    return Err(self.error_here(
                        "contextual `self` receivers are only allowed in extend or trait methods",
                    ));
                }
                if self.at(&TokenKind::Colon) {
                    return Err(self.error_here(
                        "method receiver is contextual `self` and cannot have an explicit type",
                    ));
                }
                Type::Named("Self".into(), Vec::new())
            } else {
                self.expect(&TokenKind::Colon, "`:` after parameter name")?;
                self.type_expr()?
            };
            params.push(Param {
                mode,
                access,
                passing,
                region,
                name,
                ty,
            });

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

    fn optional_region(&mut self) -> Result<Option<String>, ParseError> {
        if !self.at(&TokenKind::LParen)
            || !matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::RegionName(_))
            )
        {
            return Ok(None);
        }
        self.expect(&TokenKind::LParen, "`(` before region")?;
        let token = self.current().clone();
        let TokenKind::RegionName(name) = token.kind else {
            unreachable!("optional region lookahead was checked");
        };
        self.advance();
        self.expect(&TokenKind::RParen, "`)` after region")?;
        Ok(Some(name))
    }

    fn optional_borrow_arguments(
        &mut self,
    ) -> Result<(bool, Option<String>, Option<String>), ParseError> {
        if !self.at(&TokenKind::LParen) {
            return Ok((false, None, None));
        }
        match self.tokens.get(self.index + 1).map(|token| &token.kind) {
            Some(TokenKind::RegionName(_)) => {
                return Ok((false, None, self.optional_region()?));
            }
            Some(TokenKind::Mut) | Some(TokenKind::Ident(_)) => {}
            _ => return Ok((false, None, None)),
        }
        self.expect(&TokenKind::LParen, "`(` after `borrow`")?;
        let (mutable, access) = if self.take(&TokenKind::Mut) {
            (true, None)
        } else {
            let name = self.expect_ident("an access value or access parameter")?;
            if name == "shared" {
                (false, None)
            } else {
                (false, Some(name))
            }
        };
        let region = if self.take(&TokenKind::Comma) {
            let token = self.current().clone();
            let TokenKind::RegionName(name) = token.kind else {
                return Err(self.error_at(&token, "expected a region after access argument"));
            };
            self.advance();
            Some(name)
        } else {
            None
        };
        self.expect(&TokenKind::RParen, "`)` after borrow arguments")?;
        Ok((mutable, access, region))
    }

    fn validate_receiver_groups(&self, groups: &[Vec<Param>]) -> Result<(), ParseError> {
        let receivers = groups
            .iter()
            .enumerate()
            .flat_map(|(group_index, group)| {
                group
                    .iter()
                    .filter(|param| param.name == "self")
                    .map(move |_| group_index)
            })
            .collect::<Vec<_>>();

        if receivers.len() > 1 {
            return Err(self.error_here("a method can have at most one `self` receiver"));
        }
        let Some(group_index) = receivers.first().copied() else {
            return Ok(());
        };
        if group_index != 0 {
            return Err(self.error_here("`self` must appear in the first parameter group"));
        }
        if groups[0].len() != 1 {
            return Err(self.error_here("`self` must be the only parameter in its group"));
        }
        if groups.len() < 2 {
            return Err(self.error_here(
                "an instance method requires an explicit parameter group after `self`",
            ));
        }
        Ok(())
    }

    fn struct_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
    ) -> Result<StructDef, ParseError> {
        self.expect(&TokenKind::Struct, "`struct`")?;
        self.expect(&TokenKind::LParen, "`(` after `struct`")?;
        let fields = self.named_type_fields()?;
        Ok(StructDef {
            name,
            compile_groups,
            fields,
        })
    }

    fn enum_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
    ) -> Result<EnumDef, ParseError> {
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
                } else if self.ident_followed_by_colon() || self.at(&TokenKind::Pub) {
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
        Ok(EnumDef {
            name,
            compile_groups,
            variants,
        })
    }

    fn trait_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
    ) -> Result<TraitDef, ParseError> {
        self.expect(&TokenKind::Trait, "`trait`")?;
        self.expect(&TokenKind::LBrace, "`{` after `trait`")?;
        self.skip_separators();

        let mut members = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` before end of trait declaration"));
            }
            members.push(self.trait_member()?);
            if !self.at(&TokenKind::RBrace) && !self.at_separator() {
                return Err(self.error_here("expected a newline or `;` after trait member"));
            }
            self.skip_separators();
        }
        self.expect(&TokenKind::RBrace, "`}` after trait members")?;

        Ok(TraitDef {
            name,
            compile_groups,
            members,
        })
    }

    fn trait_member(&mut self) -> Result<TraitMember, ParseError> {
        if self.at(&TokenKind::Pub) {
            return Err(self.error_here("visibility on trait members is not supported yet"));
        }
        self.expect(&TokenKind::Let, "`let` in trait body")?;
        if self.take(&TokenKind::Mut) {
            let mutable = self.previous().clone();
            return Err(self.error_at(&mutable, "trait members cannot be declared with `let mut`"));
        }
        let name = self.expect_ident("a trait member name")?;
        let (compile_groups, groups) = self.declaration_groups(true)?;
        self.validate_receiver_groups(&groups)?;

        let return_type = if self.take(&TokenKind::Colon) {
            if self.take(&TokenKind::Type) {
                if !groups.is_empty() {
                    return Err(
                        self.error_here("associated types cannot have runtime parameter groups")
                    );
                }
                self.take_newlines_if_followed_by(&[TokenKind::Equal]);
                let default = if self.take(&TokenKind::Equal) {
                    Some(self.type_expr()?)
                } else {
                    None
                };
                return Ok(TraitMember::AssociatedType {
                    name,
                    compile_groups,
                    default,
                });
            }
            Some(self.type_expr()?)
        } else {
            None
        };

        if compile_groups.is_empty() && groups.is_empty() {
            return Err(
                self.error_here("trait function members require at least one parameter group")
            );
        }

        self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::Equal]);
        let where_predicates = self.where_clause()?;
        if !where_predicates.is_empty() {
            return Err(self.error_here("where clauses on trait members are not supported yet"));
        }
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);
        let body = if self.take(&TokenKind::Equal) {
            Some(if self.at(&TokenKind::LBrace) {
                self.block()?
            } else {
                self.expression(true)?
            })
        } else {
            None
        };

        Ok(TraitMember::Function(Function {
            name,
            compile_groups,
            groups,
            return_type,
            where_predicates,
            body,
        }))
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
            let visibility = self.visibility()?;
            let name = self.expect_ident("a field name")?;
            self.expect(&TokenKind::Colon, "`:` after field name")?;
            fields.push(Field {
                visibility,
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
        self.runtime_parameter_group(false, &HashSet::new())
    }

    fn type_expr(&mut self) -> Result<Type, ParseError> {
        if self.take(&TokenKind::LParen) {
            self.expect(&TokenKind::RParen, "`)` in unit type")?;
            return Ok(Type::Unit);
        }

        let borrow_qualifier = if self.take(&TokenKind::Borrow) {
            let (mutable, access, region) = self.optional_borrow_arguments()?;
            Some((mutable, access, region))
        } else {
            None
        };
        if let Some((mutable, access, region)) = borrow_qualifier {
            return Ok(Type::Borrow {
                mutable,
                access,
                region,
                pointee: Box::new(self.type_expr()?),
            });
        }

        if matches!(&self.current().kind, TokenKind::Ident(name) if name == "_") {
            return Err(self.error_here(
                "`_` type inference has been removed; omit the compile-time argument group or use named arguments",
            ));
        }

        let mut path = vec![self.expect_path_start("a type")?];
        while self.take(&TokenKind::Dot) {
            let segment = self.expect_path_continuation(&path, "a type path segment after `.`")?;
            path.push(segment);
        }
        let name = path.join(".");
        if name == "Array" && self.take(&TokenKind::LParen) {
            let element = self.type_expr()?;
            self.expect(&TokenKind::Comma, "`,` before array length")?;
            let length_token = self.current().clone();
            if matches!(&length_token.kind, TokenKind::Ident(name) if name == "_") {
                return Err(self.error_at(
                    &length_token,
                    "`_` compile-time argument inference has been removed; provide an explicit array length",
                ));
            }
            let TokenKind::Integer(length) = length_token.kind else {
                return Err(self.error_at(
                    &length_token,
                    "array length must be a non-negative decimal integer",
                ));
            };
            let length = u64::try_from(length).map_err(|_| {
                self.error_at(
                    &length_token,
                    "array length must fit in an unsigned 64-bit integer",
                )
            })?;
            self.advance();
            self.take(&TokenKind::Comma);
            self.expect(&TokenKind::RParen, "`)` after array length")?;
            return Ok(Type::Array(Box::new(element), length));
        }

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
                "void" => Type::Unit,
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
        let scrutinee = self.coalesce(allow_trailing_closure)?;
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

    fn coalesce(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let left = self.logical_or(allow_trailing_closure)?;
        if self.take(&TokenKind::QuestionQuestion) {
            let right = self.coalesce(allow_trailing_closure)?;
            Ok(Expr::Coalesce(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
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
            TokenKind::Ident(_) | TokenKind::Root | TokenKind::Super => self.named_pattern(),
            _ => Err(self.error_at(
                &token,
                format!("expected a pattern, found {}", describe(&token.kind)),
            )),
        }
    }

    fn named_pattern(&mut self) -> Result<Pattern, ParseError> {
        let anchored = self.at(&TokenKind::Root) || self.at(&TokenKind::Super);
        let mut path = vec![self.expect_path_start("a pattern name")?];
        while self.take(&TokenKind::Dot) {
            let segment = self.expect_path_continuation(&path, "a name after `.`")?;
            path.push(segment);
        }

        let has_payload = self.at(&TokenKind::LParen);
        let looks_like_constructor = anchored
            || path.len() > 1
            || has_payload
            || path[0].chars().next().is_some_and(char::is_uppercase);
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
            Expr::Index { base, .. } => Self::is_assignable_place(base),
            Expr::Unary(UnaryOp::Deref, _) => true,
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
        let mut expression = self.bitwise_or(allow_trailing_closure)?;
        while self.take(&TokenKind::AndAnd) {
            let right = self.bitwise_or(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), BinaryOp::And, Box::new(right));
        }
        Ok(expression)
    }

    fn bitwise_or(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.bitwise_xor(allow_trailing_closure)?;
        while self.take(&TokenKind::Pipe) {
            let right = self.bitwise_xor(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), BinaryOp::BitOr, Box::new(right));
        }
        Ok(expression)
    }

    fn bitwise_xor(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.bitwise_and(allow_trailing_closure)?;
        while self.take(&TokenKind::Caret) {
            let right = self.bitwise_and(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), BinaryOp::BitXor, Box::new(right));
        }
        Ok(expression)
    }

    fn bitwise_and(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.equality(allow_trailing_closure)?;
        while self.take(&TokenKind::Amp) {
            let right = self.equality(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), BinaryOp::BitAnd, Box::new(right));
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
        let left = self.shift(allow_trailing_closure)?;
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
        let right = self.shift(allow_trailing_closure)?;
        if matches!(
            self.current().kind,
            TokenKind::Less | TokenKind::LessEqual | TokenKind::Greater | TokenKind::GreaterEqual
        ) {
            return Err(self.error_here("comparison operators cannot be chained"));
        }
        Ok(Expr::Binary(Box::new(left), operator, Box::new(right)))
    }

    fn shift(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let mut expression = self.additive(allow_trailing_closure)?;
        loop {
            let operator = if self.take(&TokenKind::Shl) {
                Some(BinaryOp::Shl)
            } else if self.take(&TokenKind::Shr) {
                Some(BinaryOp::Shr)
            } else {
                None
            };
            let Some(operator) = operator else {
                break;
            };
            let right = self.additive(allow_trailing_closure)?;
            expression = Expr::Binary(Box::new(expression), operator, Box::new(right));
        }
        Ok(expression)
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
        } else if self.take(&TokenKind::Star) {
            let operand = self.unary(allow_trailing_closure)?;
            Ok(Expr::Unary(UnaryOp::Deref, Box::new(operand)))
        } else if self.take(&TokenKind::Borrow) {
            let borrow = self.previous().clone();
            let (mutable, access, _) = self.optional_borrow_arguments()?;
            self.borrow_expression(mutable, access, &borrow, allow_trailing_closure)
        } else if self.take(&TokenKind::Mut) {
            Ok(Expr::Name("mut".to_owned()))
        } else {
            self.postfix(allow_trailing_closure)
        }
    }

    fn borrow_expression(
        &mut self,
        mutable: bool,
        access: Option<String>,
        operator: &Token,
        allow_trailing_closure: bool,
    ) -> Result<Expr, ParseError> {
        let value = self.unary(allow_trailing_closure)?;
        if !Self::is_assignable_place(&value) {
            return Err(self.error_at(operator, "borrow operand must be a name or member chain"));
        }
        Ok(Expr::Borrow {
            mutable,
            access,
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
            } else if self.take(&TokenKind::LBracket) {
                let index = self.expression(true)?;
                self.expect(&TokenKind::RBracket, "`]` after index")?;
                expression = Expr::Index {
                    base: Box::new(expression),
                    index: Box::new(index),
                };
            } else if self.take(&TokenKind::Dot) {
                if self.take(&TokenKind::Try) {
                    expression = Expr::Try(Box::new(expression));
                } else {
                    let member = if self.at(&TokenKind::Super)
                        && Self::is_super_path_expression(&expression)
                    {
                        self.advance();
                        "super".to_owned()
                    } else {
                        self.expect_relative_path_segment("a member name after `.`")?
                    };
                    expression = Expr::Member(Box::new(expression), member);
                }
            } else if self.take(&TokenKind::QuestionDot) {
                let member = self.expect_ident("a member name after `?.`")?;
                expression = Expr::ChainMember(Box::new(expression), member);
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
            TokenKind::Copy => {
                self.advance();
                Ok(Expr::Name("copy".to_owned()))
            }
            TokenKind::Move => {
                self.advance();
                Ok(Expr::Name("move".to_owned()))
            }
            TokenKind::Ident(ref name) if name == "_" => Err(self.error_at(
                &token,
                "`_` is not an expression; omit an inferred compile-time argument group or use a named argument",
            )),
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Expr::Name(name))
            }
            TokenKind::Root => {
                self.advance();
                Ok(Expr::Name("root".into()))
            }
            TokenKind::Super => {
                self.advance();
                Ok(Expr::Name("super".into()))
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
            TokenKind::LBracket => self.array_literal(),
            TokenKind::Do => {
                self.advance();
                self.block()
            }
            TokenKind::Try => {
                self.advance();
                let container = if self.at(&TokenKind::Do) {
                    None
                } else {
                    Some(self.type_expr()?)
                };
                self.expect(&TokenKind::Do, "`do` in try block")?;
                Ok(Expr::TryBlock {
                    container,
                    body: Box::new(self.block()?),
                })
            }
            TokenKind::Unsafe => {
                self.advance();
                self.expect(&TokenKind::Do, "`do` after `unsafe`")?;
                Ok(Expr::Unsafe(Box::new(self.block()?)))
            }
            TokenKind::If => self.if_expression(),
            TokenKind::Return => self.return_expression(allow_trailing_closure),
            TokenKind::Throw => self.throw_expression(allow_trailing_closure),
            TokenKind::While => self.while_expression(),
            TokenKind::Loop => self.loop_expression(),
            TokenKind::Break => self.break_expression(allow_trailing_closure),
            TokenKind::LBrace => self.closure(),
            _ => Err(self.error_at(
                &token,
                format!("expected an expression, found {}", describe(&token.kind)),
            )),
        }
    }

    fn array_literal(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBracket, "`[`")?;
        let mut elements = Vec::new();
        if self.take(&TokenKind::RBracket) {
            return Ok(Expr::Array(elements));
        }

        loop {
            elements.push(self.expression(true)?);
            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RBracket) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RBracket, "`]` after array elements")?;
                break;
            }
        }
        Ok(Expr::Array(elements))
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
        if self.at_control_expression_boundary() {
            Ok(Expr::Return(None))
        } else {
            let value = self.expression(allow_trailing_closure)?;
            Ok(Expr::Return(Some(Box::new(value))))
        }
    }

    fn throw_expression(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Throw, "`throw`")?;
        if self.at_control_expression_boundary() {
            return Err(self.error_here("expected an expression after `throw`"));
        }
        let value = self.expression(allow_trailing_closure)?;
        Ok(Expr::Throw(Box::new(value)))
    }

    fn while_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::While, "`while`")?;
        let condition = self.expression(false)?;
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("expected `{` after `while` condition"));
        }
        let body = self.block()?;
        Ok(Expr::While {
            condition: Box::new(condition),
            body: Box::new(body),
        })
    }

    fn loop_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Loop, "`loop`")?;
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("expected `{` after `loop`"));
        }
        Ok(Expr::Loop {
            body: Box::new(self.block()?),
        })
    }

    fn break_expression(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Break, "`break`")?;
        if self.at_control_expression_boundary() {
            Ok(Expr::Break(None))
        } else {
            let value = self.expression(allow_trailing_closure)?;
            Ok(Expr::Break(Some(Box::new(value))))
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

    fn skip_newlines(&mut self) {
        while self.take(&TokenKind::Newline) {}
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

    fn at_control_expression_boundary(&self) -> bool {
        self.at_separator()
            || self.at(&TokenKind::RBrace)
            || self.at(&TokenKind::Eof)
            || self.at(&TokenKind::Comma)
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

    fn expect_path_start(&mut self, expected: &str) -> Result<String, ParseError> {
        match self.current().kind.clone() {
            TokenKind::Ident(name) if name != "_" => {
                self.advance();
                Ok(name)
            }
            TokenKind::Root => {
                self.advance();
                Ok("root".into())
            }
            TokenKind::Super => {
                self.advance();
                Ok("super".into())
            }
            _ => Err(self.error_here(format!(
                "expected {expected}, found {}",
                describe(&self.current().kind)
            ))),
        }
    }

    fn expect_path_continuation(
        &mut self,
        prefix: &[String],
        expected: &str,
    ) -> Result<String, ParseError> {
        if self.at(&TokenKind::Super) && prefix.iter().all(|segment| segment == "super") {
            self.advance();
            return Ok("super".into());
        }
        self.expect_relative_path_segment(expected)
    }

    fn expect_relative_path_segment(&mut self, expected: &str) -> Result<String, ParseError> {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Ident(name) if name != "_" => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(self.error_at(
                &token,
                format!(
                    "expected {expected}, found {}; `root` is only valid as the first path segment, and `super` only in a leading chain",
                    describe(&token.kind)
                ),
            )),
        }
    }

    fn is_super_path_expression(expression: &Expr) -> bool {
        match expression {
            Expr::Name(name) => name == "super",
            Expr::Member(base, member) => member == "super" && Self::is_super_path_expression(base),
            _ => false,
        }
    }

    fn expect_import_alias(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        let alias = match &token.kind {
            TokenKind::Ident(alias) => alias.clone(),
            _ => {
                return Err(self.error_at(
                    &token,
                    format!(
                        "expected an import alias after `as`, found {}",
                        describe(&token.kind)
                    ),
                ));
            }
        };
        if alias == "self" || alias == "_" {
            return Err(self.error_at(
                &token,
                format!("`{alias}` cannot be used as an import alias"),
            ));
        }
        self.advance();
        Ok(alias)
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

fn validate_region_scopes(items: &[Item]) -> Result<(), String> {
    let empty = HashSet::new();
    for item in items {
        match item {
            Item::Function(function) => validate_function_scopes(function, &empty, &empty)?,
            Item::Global(binding) => validate_binding_scopes(binding, &empty, &empty)?,
            Item::Struct(definition) => {
                reject_passing_parameters(
                    &definition.compile_groups,
                    &format!("struct `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for field in &definition.fields {
                    validate_type_regions(&field.ty, &regions)?;
                    validate_type_accesses(&field.ty, &accesses)?;
                }
            }
            Item::Enum(definition) => {
                reject_passing_parameters(
                    &definition.compile_groups,
                    &format!("enum `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for variant in &definition.variants {
                    match &variant.fields {
                        VariantFields::Unit => {}
                        VariantFields::Positional(types) => {
                            for ty in types {
                                validate_type_regions(ty, &regions)?;
                                validate_type_accesses(ty, &accesses)?;
                            }
                        }
                        VariantFields::Named(fields) => {
                            for field in fields {
                                validate_type_regions(&field.ty, &regions)?;
                                validate_type_accesses(&field.ty, &accesses)?;
                            }
                        }
                    }
                }
            }
            Item::Trait(definition) => {
                reject_passing_parameters(
                    &definition.compile_groups,
                    &format!("trait `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for member in &definition.members {
                    match member {
                        TraitMember::Function(function) => {
                            validate_function_scopes(function, &regions, &accesses)?
                        }
                        TraitMember::AssociatedType {
                            name,
                            compile_groups,
                            default,
                        } => {
                            reject_passing_parameters(
                                compile_groups,
                                &format!("associated type `{}`", name),
                            )?;
                            let member_regions = declared_regions(compile_groups, &regions)?;
                            let member_accesses = declared_accesses(compile_groups, &accesses)?;
                            if let Some(default) = default {
                                validate_type_regions(default, &member_regions)?;
                                validate_type_accesses(default, &member_accesses)?;
                            }
                        }
                    }
                }
            }
            Item::Extend(extension) => {
                reject_passing_parameters(&extension.compile_groups, "extend header")?;
                let regions = declared_regions(&extension.compile_groups, &empty)?;
                let accesses = declared_accesses(&extension.compile_groups, &empty)?;
                validate_type_regions(&extension.target, &regions)?;
                validate_type_accesses(&extension.target, &accesses)?;
                if let Some(trait_ref) = &extension.trait_ref {
                    validate_type_regions(trait_ref, &regions)?;
                    validate_type_accesses(trait_ref, &accesses)?;
                }
                for predicate in &extension.where_predicates {
                    validate_type_regions(&predicate.subject, &regions)?;
                    validate_type_regions(&predicate.trait_ref, &regions)?;
                    for binding in &predicate.associated_types {
                        validate_type_regions(&binding.ty, &regions)?;
                    }
                }
                for member in &extension.members {
                    match member {
                        ExtendMember::Function(function) => {
                            validate_function_scopes(function, &regions, &accesses)?
                        }
                        ExtendMember::Const(binding) => {
                            validate_binding_scopes(binding, &regions, &accesses)?
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn reject_passing_parameters(groups: &[Vec<CompileParam>], owner: &str) -> Result<(), String> {
    if groups
        .iter()
        .flatten()
        .any(|parameter| parameter.kind == CompileParamKind::Passing)
    {
        Err(format!(
            "{owner} cannot declare a `passing` parameter; passing parameters belong to functions"
        ))
    } else {
        Ok(())
    }
}

fn declared_accesses(
    groups: &[Vec<CompileParam>],
    outer: &HashSet<String>,
) -> Result<HashSet<String>, String> {
    let mut accesses = outer.clone();
    for parameter in groups.iter().flatten() {
        if parameter.kind == CompileParamKind::Access && !accesses.insert(parameter.name.clone()) {
            return Err(format!("duplicate access parameter `{}`", parameter.name));
        }
    }
    Ok(accesses)
}

fn declared_regions(
    groups: &[Vec<CompileParam>],
    outer: &HashSet<String>,
) -> Result<HashSet<String>, String> {
    let mut regions = outer.clone();
    for parameter in groups.iter().flatten() {
        if parameter.kind != CompileParamKind::Region {
            continue;
        }
        if parameter.name == "static" {
            return Err("`'static` is predefined and cannot be redeclared".to_owned());
        }
        if !regions.insert(parameter.name.clone()) {
            return Err(format!("duplicate region parameter `'{}'", parameter.name));
        }
    }
    Ok(regions)
}

fn validate_function_scopes(
    function: &Function,
    outer_regions: &HashSet<String>,
    outer_accesses: &HashSet<String>,
) -> Result<(), String> {
    let regions = declared_regions(&function.compile_groups, outer_regions)?;
    let accesses = declared_accesses(&function.compile_groups, outer_accesses)?;
    let passings = function
        .compile_groups
        .iter()
        .flatten()
        .filter(|parameter| parameter.kind == CompileParamKind::Passing)
        .map(|parameter| parameter.name.clone())
        .collect::<HashSet<_>>();
    let mut compile_names = HashSet::new();
    for parameter in function.compile_groups.iter().flatten() {
        if !compile_names.insert(parameter.name.clone()) {
            return Err(format!(
                "duplicate compile-time parameter `{}`",
                parameter.name
            ));
        }
    }
    for parameter in function.groups.iter().flatten() {
        if let Some(passing) = &parameter.passing {
            if !passings.contains(passing) {
                return Err(format!("use of undeclared passing parameter `{passing}`"));
            }
        }
        if let Some(access) = &parameter.access {
            validate_access_name(access, &accesses)?;
        }
        if let Some(region) = &parameter.region {
            validate_region_name(region, &regions)?;
        }
        validate_type_regions(&parameter.ty, &regions)?;
        validate_type_accesses(&parameter.ty, &accesses)?;
    }
    if let Some(return_type) = &function.return_type {
        validate_type_regions(return_type, &regions)?;
        validate_type_accesses(return_type, &accesses)?;
    }
    for predicate in &function.where_predicates {
        validate_type_regions(&predicate.subject, &regions)?;
        validate_type_regions(&predicate.trait_ref, &regions)?;
        for binding in &predicate.associated_types {
            validate_type_regions(&binding.ty, &regions)?;
        }
    }
    if let Some(body) = &function.body {
        validate_expr_regions(body, &regions)?;
        validate_expr_accesses(body, &accesses)?;
    }
    Ok(())
}

fn validate_access_name(access: &str, accesses: &HashSet<String>) -> Result<(), String> {
    if accesses.contains(access) {
        Ok(())
    } else {
        Err(format!("use of undeclared access parameter `{access}`"))
    }
}

fn validate_type_accesses(ty: &Type, accesses: &HashSet<String>) -> Result<(), String> {
    match ty {
        Type::Borrow {
            access, pointee, ..
        } => {
            if let Some(access) = access {
                validate_access_name(access, accesses)?;
            }
            validate_type_accesses(pointee, accesses)
        }
        Type::Array(element, _) => validate_type_accesses(element, accesses),
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_accesses(argument, accesses)?;
            }
            Ok(())
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => Ok(()),
    }
}

fn validate_expr_accesses(expression: &Expr, accesses: &HashSet<String>) -> Result<(), String> {
    match expression {
        Expr::Borrow { access, value, .. } => {
            if let Some(access) = access {
                validate_access_name(access, accesses)?;
            }
            validate_expr_accesses(value, accesses)
        }
        Expr::Unary(_, value) | Expr::Try(value) | Expr::Throw(value) | Expr::Unsafe(value) => {
            validate_expr_accesses(value, accesses)
        }
        Expr::TryBlock { container, body } => {
            if let Some(container) = container {
                validate_type_accesses(container, accesses)?;
            }
            validate_expr_accesses(body, accesses)
        }
        Expr::Binary(left, _, right) | Expr::Coalesce(left, right) | Expr::Assign(left, right) => {
            validate_expr_accesses(left, accesses)?;
            validate_expr_accesses(right, accesses)
        }
        Expr::Call(callee, arguments) => {
            validate_expr_accesses(callee, accesses)?;
            for argument in arguments {
                validate_expr_accesses(&argument.value, accesses)?;
            }
            Ok(())
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            validate_expr_accesses(base, accesses)
        }
        Expr::Array(elements) => {
            for element in elements {
                validate_expr_accesses(element, accesses)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            validate_expr_accesses(base, accesses)?;
            validate_expr_accesses(index, accesses)
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &binding.annotation {
                            validate_type_accesses(annotation, accesses)?;
                        }
                        validate_expr_accesses(&binding.value, accesses)?;
                    }
                    Stmt::Expr(expression) => validate_expr_accesses(expression, accesses)?,
                }
            }
            if let Some(tail) = tail {
                validate_expr_accesses(tail, accesses)?;
            }
            Ok(())
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                if let Some(access) = &parameter.access {
                    validate_access_name(access, accesses)?;
                }
                validate_type_accesses(&parameter.ty, accesses)?;
            }
            validate_expr_accesses(body, accesses)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expr_accesses(condition, accesses)?;
            validate_expr_accesses(then_branch, accesses)?;
            if let Some(else_branch) = else_branch {
                validate_expr_accesses(else_branch, accesses)?;
            }
            Ok(())
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                validate_expr_accesses(value, accesses)?;
            }
            Ok(())
        }
        Expr::While { condition, body } => {
            validate_expr_accesses(condition, accesses)?;
            validate_expr_accesses(body, accesses)
        }
        Expr::Loop { body } => validate_expr_accesses(body, accesses),
        Expr::Match { scrutinee, arms } => {
            validate_expr_accesses(scrutinee, accesses)?;
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    validate_expr_accesses(guard, accesses)?;
                }
                validate_expr_accesses(&arm.body, accesses)?;
            }
            Ok(())
        }
        Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Name(_) => Ok(()),
    }
}

fn validate_binding_scopes(
    binding: &Binding,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    if let Some(annotation) = &binding.annotation {
        validate_type_regions(annotation, regions)?;
        validate_type_accesses(annotation, accesses)?;
    }
    validate_expr_regions(&binding.value, regions)?;
    validate_expr_accesses(&binding.value, accesses)
}

fn validate_type_regions(ty: &Type, regions: &HashSet<String>) -> Result<(), String> {
    match ty {
        Type::Borrow {
            region, pointee, ..
        } => {
            if let Some(region) = region {
                validate_region_name(region, regions)?;
            }
            validate_type_regions(pointee, regions)
        }
        Type::Array(element, _) => validate_type_regions(element, regions),
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_regions(argument, regions)?;
            }
            Ok(())
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => Ok(()),
    }
}

fn validate_region_name(region: &str, regions: &HashSet<String>) -> Result<(), String> {
    if region == "static" || regions.contains(region) {
        Ok(())
    } else {
        Err(format!("use of undeclared region `'{}'", region))
    }
}

fn validate_expr_regions(expression: &Expr, regions: &HashSet<String>) -> Result<(), String> {
    match expression {
        Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Name(_) => Ok(()),
        Expr::Unary(_, value)
        | Expr::Try(value)
        | Expr::Throw(value)
        | Expr::Unsafe(value)
        | Expr::Borrow { value, .. } => validate_expr_regions(value, regions),
        Expr::TryBlock { container, body } => {
            if let Some(container) = container {
                validate_type_regions(container, regions)?;
            }
            validate_expr_regions(body, regions)
        }
        Expr::Binary(left, _, right) | Expr::Coalesce(left, right) | Expr::Assign(left, right) => {
            validate_expr_regions(left, regions)?;
            validate_expr_regions(right, regions)
        }
        Expr::Call(callee, arguments) => {
            validate_expr_regions(callee, regions)?;
            for argument in arguments {
                validate_expr_regions(&argument.value, regions)?;
            }
            Ok(())
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => validate_expr_regions(base, regions),
        Expr::Array(elements) => {
            for element in elements {
                validate_expr_regions(element, regions)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            validate_expr_regions(base, regions)?;
            validate_expr_regions(index, regions)
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &binding.annotation {
                            validate_type_regions(annotation, regions)?;
                        }
                        validate_expr_regions(&binding.value, regions)?;
                    }
                    Stmt::Expr(expression) => validate_expr_regions(expression, regions)?,
                }
            }
            if let Some(tail) = tail {
                validate_expr_regions(tail, regions)?;
            }
            Ok(())
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                if let Some(region) = &parameter.region {
                    validate_region_name(region, regions)?;
                }
                validate_type_regions(&parameter.ty, regions)?;
            }
            validate_expr_regions(body, regions)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expr_regions(condition, regions)?;
            validate_expr_regions(then_branch, regions)?;
            if let Some(else_branch) = else_branch {
                validate_expr_regions(else_branch, regions)?;
            }
            Ok(())
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                validate_expr_regions(value, regions)?;
            }
            Ok(())
        }
        Expr::While { condition, body } => {
            validate_expr_regions(condition, regions)?;
            validate_expr_regions(body, regions)
        }
        Expr::Loop { body } => validate_expr_regions(body, regions),
        Expr::Match { scrutinee, arms } => {
            validate_expr_regions(scrutinee, regions)?;
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    validate_expr_regions(guard, regions)?;
                }
                validate_expr_regions(&arm.body, regions)?;
            }
            Ok(())
        }
    }
}

fn describe(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Let => "`let`",
        TokenKind::Pub => "`pub`",
        TokenKind::Package => "`package`",
        TokenKind::Use => "`use`",
        TokenKind::As => "`as`",
        TokenKind::Root => "`root`",
        TokenKind::Super => "`super`",
        TokenKind::Mut => "`mut`",
        TokenKind::Copy => "`copy`",
        TokenKind::Move => "`move`",
        TokenKind::Borrow => "`borrow`",
        TokenKind::Type => "`type`",
        TokenKind::Region => "`region`",
        TokenKind::Do => "`do`",
        TokenKind::Unsafe => "`unsafe`",
        TokenKind::If => "`if`",
        TokenKind::Else => "`else`",
        TokenKind::Return => "`return`",
        TokenKind::Throw => "`throw`",
        TokenKind::While => "`while`",
        TokenKind::Loop => "`loop`",
        TokenKind::Break => "`break`",
        TokenKind::Extend => "`extend`",
        TokenKind::Struct => "`struct`",
        TokenKind::Enum => "`enum`",
        TokenKind::Trait => "`trait`",
        TokenKind::Where => "`where`",
        TokenKind::Match => "`match`",
        TokenKind::Try => "`try`",
        TokenKind::True => "`true`",
        TokenKind::False => "`false`",
        TokenKind::RegionName(_) => "a region name",
        TokenKind::Ident(_) => "an identifier",
        TokenKind::Integer(_) => "an integer",
        TokenKind::LParen => "`(`",
        TokenKind::RParen => "`)`",
        TokenKind::LBracket => "`[`",
        TokenKind::RBracket => "`]`",
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
        TokenKind::Amp => "`&`",
        TokenKind::Pipe => "`|`",
        TokenKind::Caret => "`^`",
        TokenKind::Shl => "`<<`",
        TokenKind::Shr => "`>>`",
        TokenKind::QuestionQuestion => "`??`",
        TokenKind::QuestionDot => "`?.`",
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
    fn bitwise_and_shift_precedence_is_fixed_by_the_language() {
        let program = parse(
            "let shifts = 1 + 2 << 3 + 4\n\
             let bits = 1 | 2 ^ 3 & 4\n",
        )
        .unwrap();

        let Item::Global(shifts) = &program.items[0] else {
            panic!("expected shifts global");
        };
        assert!(matches!(
            &shifts.value,
            Expr::Binary(left, BinaryOp::Shl, right)
                if matches!(left.as_ref(), Expr::Binary(_, BinaryOp::Add, _))
                    && matches!(right.as_ref(), Expr::Binary(_, BinaryOp::Add, _))
        ));

        let Item::Global(bits) = &program.items[1] else {
            panic!("expected bits global");
        };
        assert!(matches!(
            &bits.value,
            Expr::Binary(one, BinaryOp::BitOr, xor)
                if matches!(one.as_ref(), Expr::Integer(1))
                    && matches!(
                        xor.as_ref(),
                        Expr::Binary(two, BinaryOp::BitXor, and)
                            if matches!(two.as_ref(), Expr::Integer(2))
                                && matches!(and.as_ref(), Expr::Binary(_, BinaryOp::BitAnd, _))
                    )
        ));
    }

    #[test]
    fn preserves_top_level_visibility_alongside_items() {
        let program = parse(
            "let private = 0\n\
             pub let exported = 1\n\
             pub(package) let shared = 2\n",
        )
        .unwrap();

        assert_eq!(
            program.item_visibilities,
            vec![Visibility::Private, Visibility::Public, Visibility::Package,]
        );
        assert_eq!(program.items.len(), program.item_visibilities.len());
        assert!(program.uses.is_empty());
    }

    #[test]
    fn parses_and_expands_import_declarations() {
        let program = parse(
            "use net.http.Client\n\
             use net.http.Client as OtherClient\n\
             pub use net.http.{get, post as send}\n\
             pub(package) use root.core.Value\n\
             let answer = 42\n",
        )
        .unwrap();

        assert_eq!(
            program.uses,
            vec![
                UseDecl {
                    visibility: Visibility::Private,
                    path: vec!["net".into(), "http".into(), "Client".into()],
                    alias: None,
                },
                UseDecl {
                    visibility: Visibility::Private,
                    path: vec!["net".into(), "http".into(), "Client".into()],
                    alias: Some("OtherClient".into()),
                },
                UseDecl {
                    visibility: Visibility::Public,
                    path: vec!["net".into(), "http".into(), "get".into()],
                    alias: None,
                },
                UseDecl {
                    visibility: Visibility::Public,
                    path: vec!["net".into(), "http".into(), "post".into()],
                    alias: Some("send".into()),
                },
                UseDecl {
                    visibility: Visibility::Package,
                    path: vec!["root".into(), "core".into(), "Value".into()],
                    alias: None,
                },
            ]
        );
        assert_eq!(program.items.len(), 1);
        assert_eq!(program.item_visibilities, vec![Visibility::Private]);
    }

    #[test]
    fn rejects_empty_duplicate_and_invalid_imports() {
        let empty = parse("use net.http.{}\n").unwrap_err();
        assert!(empty.message.contains("cannot be empty"));

        let duplicate = parse("use net.http.{get, post as get}\n").unwrap_err();
        assert!(duplicate.message.contains("duplicate import binding `get`"));

        let duplicates_for_semantic_resolution =
            parse("use net.http.get\nuse other.get\n").unwrap();
        assert_eq!(duplicates_for_semantic_resolution.uses.len(), 2);

        for alias in ["self", "_"] {
            let error = parse(&format!("use net.http.Client as {alias}\n")).unwrap_err();
            assert!(error.message.contains("cannot be used as an import alias"));
        }

        let missing_alias = parse("use net.http.Client as\n").unwrap_err();
        assert!(missing_alias.message.contains("import alias"));

        for source in ["use root\n", "use super.super\n", "use self\n"] {
            let error = parse(source).unwrap_err();
            assert!(error.message.contains("explicit usable alias"), "{error:?}");
        }
        for source in [
            "use root.super.value\n",
            "use super.root.value\n",
            "use net.{root}\n",
            "use net.{super}\n",
            "use net.{_}\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(
                error.message.contains("path segment"),
                "{source}: {error:?}"
            );
        }

        let anchors = parse(
            "use root as package_root\n\
             use super.super as ancestor\n\
             use self as current\n",
        )
        .unwrap();
        assert_eq!(
            anchors
                .uses
                .iter()
                .map(|import| import.alias.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("package_root"), Some("ancestor"), Some("current")]
        );

        let contextual = parse("use net.{self as contextual}\nuse root.self.value\n").unwrap();
        assert_eq!(contextual.uses[0].alias.as_deref(), Some("contextual"));
        let implicit_self = parse("use net.{self}\n").unwrap_err();
        assert!(implicit_self.message.contains("explicit usable alias"));
    }

    #[test]
    fn rejects_misplaced_ordinary_path_anchors() {
        for source in [
            "let bad(): foo.root.Value = 0\n",
            "let bad(): i32 = root.super.value\n",
            "let bad(value: root.Option): i32 = value match { root.super.Option.None => 0 }\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(error.message.contains("first path segment"), "{error:?}");
        }

        parse(
            "let ok(value: super.super.model.Value): i32 = super.super.api.read(root.self.value)\n",
        )
        .unwrap();
    }

    #[test]
    fn accepts_root_super_and_contextual_self_in_ordinary_paths() {
        let program = parse(
            "let resolve(value: root.model.Value): super.model.Result = root.api.call(super.value)\n\
             let unwrap(value: root.Option): i32 = value match { root.Option.Some(self) => self }\n",
        )
        .unwrap();

        let Item::Function(resolve) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            resolve.groups[0][0].ty,
            Type::Named("root.model.Value".into(), Vec::new())
        );
        assert_eq!(
            resolve.return_type,
            Some(Type::Named("super.model.Result".into(), Vec::new()))
        );
        assert!(matches!(
            &resolve.body,
            Some(Expr::Call(callee, arguments))
                if matches!(callee.as_ref(), Expr::Member(base, name)
                    if name == "call"
                        && matches!(base.as_ref(), Expr::Member(root, name)
                            if name == "api" && root.as_ref() == &Expr::Name("root".into())))
                    && matches!(&arguments[0].value, Expr::Member(base, name)
                        if name == "value" && base.as_ref() == &Expr::Name("super".into()))
        ));

        let Item::Function(unwrap) = &program.items[1] else {
            panic!("expected function");
        };
        let Some(Expr::Match { arms, .. }) = &unwrap.body else {
            panic!("expected match");
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Constructor { path, fields: PatternFields::Positional(fields) }
                if path == &vec!["root".to_owned(), "Option".to_owned(), "Some".to_owned()]
                    && fields == &vec![Pattern::Binding("self".into())]
        ));
    }

    #[test]
    fn parses_dotted_type_paths() {
        let program =
            parse("let convert(value: net.http.Point): net.http.Result(core.Status) = value\n")
                .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };

        assert_eq!(
            function.groups[0][0].ty,
            Type::Named("net.http.Point".into(), Vec::new())
        );
        assert_eq!(
            function.return_type,
            Some(Type::Named(
                "net.http.Result".into(),
                vec![Type::Named("core.Status".into(), Vec::new())],
            ))
        );
    }

    #[test]
    fn rejects_visibility_where_it_is_not_supported_yet() {
        let extension = parse("pub extend Thing {}\n").unwrap_err();
        assert!(extension
            .message
            .contains("`extend` declarations cannot have visibility"));

        let trait_member = parse("let Trait = trait { pub let f(value: i32): i32 }\n").unwrap_err();
        assert!(trait_member.message.contains("trait members"));

        let extend_member = parse("extend Thing { pub(package) let answer = 42 }\n").unwrap_err();
        assert!(extend_member.message.contains("extend members"));
    }

    #[test]
    fn parses_void_as_an_alias_of_the_unit_type() {
        let program = parse(
            "let canonical(): () = ()\n\
             let alias(): void = ()\n",
        )
        .unwrap();

        for item in &program.items {
            let Item::Function(function) = item else {
                panic!("expected function");
            };
            assert_eq!(function.return_type, Some(Type::Unit));
        }
    }

    #[test]
    fn separates_compile_time_and_runtime_parameter_groups() {
        let program = parse(
            "let identity(T: type)(value: T): T = value\n\
             let staged(T: type)(U: type)(value: T): U = value\n",
        )
        .unwrap();

        let Item::Function(identity) = &program.items[0] else {
            panic!("expected generic function");
        };
        assert_eq!(identity.compile_groups.len(), 1);
        assert_eq!(identity.compile_groups[0].len(), 1);
        assert_eq!(identity.compile_groups[0][0].name, "T");
        assert_eq!(identity.compile_groups[0][0].kind, CompileParamKind::Type);
        assert_eq!(identity.groups.len(), 1);
        assert_eq!(
            identity.groups[0][0].ty,
            Type::Named("T".into(), Vec::new())
        );
        assert_eq!(
            identity.return_type,
            Some(Type::Named("T".into(), Vec::new()))
        );

        let Item::Function(staged) = &program.items[1] else {
            panic!("expected generic function");
        };
        assert_eq!(
            staged
                .compile_groups
                .iter()
                .map(Vec::len)
                .collect::<Vec<_>>(),
            vec![1, 1]
        );
        assert_eq!(staged.groups.len(), 1);
    }

    #[test]
    fn preserves_multiple_compile_parameters_in_one_group() {
        let program = parse("let choose(T: type, U: type)(value: T): U = value\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected generic function");
        };
        assert_eq!(function.compile_groups.len(), 1);
        assert_eq!(
            function.compile_groups[0]
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>(),
            vec!["T", "U"]
        );
    }

    #[test]
    fn parses_generic_structs_and_enums() {
        let program = parse(
            "let Cell(T: type) = struct(value: T)\n\
             let Maybe(T: type) = enum {\n\
               Some(T),\n\
               Named(value: T),\n\
               None,\n\
             }\n",
        )
        .unwrap();

        let Item::Struct(cell) = &program.items[0] else {
            panic!("expected generic struct");
        };
        assert_eq!(cell.compile_groups[0][0].name, "T");
        assert_eq!(cell.fields[0].ty, Type::Named("T".into(), Vec::new()));

        let Item::Enum(maybe) = &program.items[1] else {
            panic!("expected generic enum");
        };
        assert_eq!(maybe.compile_groups[0][0].kind, CompileParamKind::Type);
        assert!(matches!(
            &maybe.variants[0].fields,
            VariantFields::Positional(types)
                if types == &vec![Type::Named("T".into(), Vec::new())]
        ));
        assert!(matches!(
            &maybe.variants[1].fields,
            VariantFields::Named(fields)
                if fields[0].ty == Type::Named("T".into(), Vec::new())
        ));
    }

    #[test]
    fn parses_trait_method_signatures_and_associated_types() {
        let program = parse(
            "let Foo = trait {\n\
               let f(borrow self)(x: i32): i32;\n\
               let Item: type\n\
             }\n",
        )
        .unwrap();

        let Item::Trait(definition) = &program.items[0] else {
            panic!("expected trait definition");
        };
        assert_eq!(definition.name, "Foo");
        assert!(definition.compile_groups.is_empty());
        assert_eq!(definition.members.len(), 2);

        let TraitMember::Function(function) = &definition.members[0] else {
            panic!("expected trait function");
        };
        assert_eq!(function.name, "f");
        assert!(function.compile_groups.is_empty());
        assert_eq!(function.groups.len(), 2);
        assert_eq!(function.groups[0][0].name, "self");
        assert_eq!(function.groups[0][0].mode, PassMode::Borrow);
        assert_eq!(function.groups[1][0].name, "x");
        assert_eq!(function.groups[1][0].ty, Type::I32);
        assert_eq!(function.return_type, Some(Type::I32));
        assert_eq!(function.body, None);

        let TraitMember::AssociatedType {
            name,
            compile_groups,
            default,
        } = &definition.members[1]
        else {
            panic!("expected associated type");
        };
        assert_eq!(name, "Item");
        assert!(compile_groups.is_empty());
        assert_eq!(default, &None);
    }

    #[test]
    fn preserves_generic_traits_and_trait_member_defaults() {
        let program = parse(
            "let Convert(T: type) = trait {\n\
               let convert(U: type)(borrow self)(value: U): T = value\n\
               let Output(V: type): type = Pair(T, V)\n\
             }\n",
        )
        .unwrap();

        let Item::Trait(definition) = &program.items[0] else {
            panic!("expected generic trait definition");
        };
        assert_eq!(definition.compile_groups.len(), 1);
        assert_eq!(definition.compile_groups[0][0].name, "T");

        let TraitMember::Function(function) = &definition.members[0] else {
            panic!("expected default method");
        };
        assert_eq!(function.compile_groups[0][0].name, "U");
        assert_eq!(
            function.return_type,
            Some(Type::Named("T".into(), Vec::new()))
        );
        assert_eq!(function.body, Some(Expr::Name("value".into())));

        let TraitMember::AssociatedType {
            name,
            compile_groups,
            default,
        } = &definition.members[1]
        else {
            panic!("expected generic associated type");
        };
        assert_eq!(name, "Output");
        assert_eq!(compile_groups[0][0].name, "V");
        assert_eq!(
            default,
            &Some(Type::Named(
                "Pair".into(),
                vec![
                    Type::Named("T".into(), Vec::new()),
                    Type::Named("V".into(), Vec::new()),
                ],
            ))
        );
    }

    #[test]
    fn rejects_runtime_parameter_groups_on_associated_types() {
        let error = parse("let Broken = trait { let Item(value: i32): type }\n").unwrap_err();
        assert!(error
            .message
            .contains("cannot have runtime parameter groups"));
    }

    #[test]
    fn rejects_removed_underscore_inference_syntax() {
        for source in [
            "let value: Cell(_) = Cell(i32)(20)\n",
            "let value = Cell(_)(20)\n",
            "let value = Cell(T: _)(20)\n",
            "let value = Cell(Cell(_))(Cell(i32)(20))\n",
            "let value = _\n",
            "let value: Array(i32, _) = []\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(error.message.contains("`_`"));
            assert!(error.message.contains("inference") || error.message.contains("inferred"));
        }
    }

    #[test]
    fn parses_unsafe_raw_pointer_dereference_and_assignment() {
        let program = parse(
            "let main(): i32 = {\n  let mut value = 41\n  let pointer = MutPtr(borrow(mut) value)\n  unsafe do {\n    *pointer = *pointer + 1\n  }\n  value\n}\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Some(Expr::Block(statements, _)) = &function.body else {
            panic!("expected function block");
        };
        assert!(matches!(
            &statements[2],
            Stmt::Expr(Expr::Unsafe(body))
                if matches!(body.as_ref(), Expr::Block(_, Some(tail)) if matches!(
                    tail.as_ref(),
                    Expr::Assign(left, _)
                        if matches!(left.as_ref(), Expr::Unary(UnaryOp::Deref, _))
                ))
        ));
    }

    #[test]
    fn keeps_generic_construction_and_variant_heads_as_regular_postfix_expressions() {
        fn argument(label: Option<&str>, value: Expr) -> CallArg {
            CallArg {
                label: label.map(str::to_owned),
                value,
            }
        }

        fn type_head(name: &str, type_argument: Expr) -> Expr {
            Expr::Call(
                Box::new(Expr::Name(name.to_owned())),
                vec![argument(None, type_argument)],
            )
        }

        let program = parse(
            "let cell = Cell(i32)(value: 42)\n\
             let nested = Cell(Cell(i32))(value: 42)\n\
             let some = Maybe(i32).Some(42)\n\
             let none = Maybe(i32).None\n",
        )
        .unwrap();

        let Item::Global(cell) = &program.items[0] else {
            panic!("expected cell binding");
        };
        assert_eq!(
            cell.value,
            Expr::Call(
                Box::new(type_head("Cell", Expr::Name("i32".into()))),
                vec![argument(Some("value"), Expr::Integer(42))],
            )
        );

        let Item::Global(nested) = &program.items[1] else {
            panic!("expected nested cell binding");
        };
        assert_eq!(
            nested.value,
            Expr::Call(
                Box::new(type_head(
                    "Cell",
                    type_head("Cell", Expr::Name("i32".into())),
                )),
                vec![argument(Some("value"), Expr::Integer(42))],
            )
        );

        let Item::Global(some) = &program.items[2] else {
            panic!("expected Some binding");
        };
        assert_eq!(
            some.value,
            Expr::Call(
                Box::new(Expr::Member(
                    Box::new(type_head("Maybe", Expr::Name("i32".into()))),
                    "Some".into(),
                )),
                vec![argument(None, Expr::Integer(42))],
            )
        );

        let Item::Global(none) = &program.items[3] else {
            panic!("expected None binding");
        };
        assert_eq!(
            none.value,
            Expr::Member(
                Box::new(type_head("Maybe", Expr::Name("i32".into()))),
                "None".into(),
            )
        );
    }

    #[test]
    fn rejects_mixed_or_misordered_compile_parameter_groups() {
        let cases = [
            (
                "let bad(value: i32)(T: type): i32 = value\n",
                "must precede runtime",
            ),
            ("let bad(T: type, value: T): T = value\n", "cannot be mixed"),
            ("let bad(value: T, U: type): T = value\n", "cannot be mixed"),
        ];

        for (source, expected) in cases {
            let error = parse(source).unwrap_err();
            assert!(
                error.message.contains(expected),
                "expected `{expected}` in `{}`",
                error.message
            );
        }
    }

    #[test]
    fn rejects_reserved_compile_parameter_names() {
        for name in ["_", "i32", "i64", "u32", "u64", "bool", "void", "never"] {
            let source = format!("let invalid({name}: type)(value: i32): i32 = value\n");
            let error = parse(&source).unwrap_err();
            assert_eq!(
                error.message,
                format!("reserved type name `{name}` cannot be used as a compile-time parameter")
            );
            assert_eq!((error.line, error.column), (1, 13));
        }
    }

    #[test]
    fn rejects_runtime_parameters_on_generic_data_and_extend_headers() {
        let data = parse("let Bad(T: type)(value: T) = struct(value: T)\n").unwrap_err();
        assert!(data.message.contains("runtime parameters"));

        let extension = parse("extend(value: i32) Cell(i32) {}\n").unwrap_err();
        assert!(extension.message.contains("only compile-time parameters"));
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
    fn parses_throw_with_a_required_operand() {
        let program = parse("let fail(): Result(i32, bool) = throw false\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            function.body,
            Some(Expr::Throw(Box::new(Expr::Bool(false))))
        );

        let error = parse("let fail(): Result(i32, bool) = { throw\n}\n").unwrap_err();
        assert!(error.message.contains("expression after `throw`"));
    }

    #[test]
    fn parses_annotated_and_contextual_try_do_blocks() {
        let program = parse(
            "let main(): Result(i32, bool) = try Result(i32, bool) do { 42 }\n\
             let other(): Result(i32, bool) = try do { throw true }\n",
        )
        .unwrap();
        let Item::Function(main) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(matches!(
            main.body,
            Some(Expr::TryBlock {
                container: Some(_),
                ..
            })
        ));
        let Item::Function(other) = &program.items[1] else {
            panic!("expected function");
        };
        assert!(matches!(
            other.body,
            Some(Expr::TryBlock {
                container: None,
                ..
            })
        ));
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
            "let Point = struct(x: i32, pub(package) y: i32, pub z: i32)\n\
             let Shape = enum {\n\
               Circle(pub radius: i32, pub(package) center: Point, label: i32),\n\
               Pair(i32, i32),\n\
               Unit,\n\
             }\n",
        )
        .unwrap();

        let Item::Struct(point) = &program.items[0] else {
            panic!("expected struct");
        };
        assert_eq!(point.name, "Point");
        assert_eq!(point.fields.len(), 3);
        assert_eq!(
            point
                .fields
                .iter()
                .map(|field| field.visibility)
                .collect::<Vec<_>>(),
            vec![Visibility::Private, Visibility::Package, Visibility::Public,]
        );

        let Item::Enum(shape) = &program.items[1] else {
            panic!("expected enum");
        };
        assert_eq!(shape.variants.len(), 3);
        assert!(matches!(
            &shape.variants[0].fields,
            VariantFields::Named(fields)
                if fields
                    .iter()
                    .map(|field| field.visibility)
                    .eq([
                        Visibility::Public,
                        Visibility::Package,
                        Visibility::Private,
                    ])
        ));
        assert!(matches!(
            &shape.variants[1].fields,
            VariantFields::Positional(types) if types == &vec![Type::I32, Type::I32]
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
    fn parses_coalesce_right_associatively_between_match_and_logical_or() {
        let program = parse(
            "let chain = a || b ??\n  c || d ?? e\n\
             let matched = a ?? b match { _ => c }\n\
             let assigned = target = a ?? b\n",
        )
        .unwrap();

        let Item::Global(chain) = &program.items[0] else {
            panic!("expected chain binding");
        };
        assert!(matches!(
            &chain.value,
            Expr::Coalesce(left, right)
                if matches!(left.as_ref(), Expr::Binary(_, BinaryOp::Or, _))
                    && matches!(
                        right.as_ref(),
                        Expr::Coalesce(nested_left, nested_right)
                            if matches!(nested_left.as_ref(), Expr::Binary(_, BinaryOp::Or, _))
                                && nested_right.as_ref() == &Expr::Name("e".into())
                    )
        ));

        let Item::Global(matched) = &program.items[1] else {
            panic!("expected match binding");
        };
        assert!(matches!(
            &matched.value,
            Expr::Match { scrutinee, .. }
                if matches!(scrutinee.as_ref(), Expr::Coalesce(_, _))
        ));

        let Item::Global(assigned) = &program.items[2] else {
            panic!("expected assignment binding");
        };
        assert!(matches!(
            &assigned.value,
            Expr::Assign(_, value) if matches!(value.as_ref(), Expr::Coalesce(_, _))
        ));
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
               let exclusive = borrow(mut) value\n\
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
                    ..
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
                    ..
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
    fn parses_fixed_array_types_multiline_literals_and_indexes() {
        let program = parse(
            "let read(values: Array(i32, 2)): i32 = {\n\
               let local: Array(i32, 3) = [\n\
                 40,\n\
                 1,\n\
                 1,\n\
               ]\n\
               local[0] + local[1]\n\
             }\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            function.groups[0][0].ty,
            Type::Array(Box::new(Type::I32), 2)
        );
        let Some(Expr::Block(statements, Some(tail))) = &function.body else {
            panic!("expected block");
        };
        let Stmt::Let(binding) = &statements[0] else {
            panic!("expected local array binding");
        };
        assert_eq!(
            binding.annotation,
            Some(Type::Array(Box::new(Type::I32), 3))
        );
        assert!(matches!(&binding.value, Expr::Array(elements) if elements.len() == 3));
        assert!(matches!(
            tail.as_ref(),
            Expr::Binary(left, BinaryOp::Add, right)
                if matches!(left.as_ref(), Expr::Index { .. })
                    && matches!(right.as_ref(), Expr::Index { .. })
        ));
    }

    #[test]
    fn indexed_places_can_be_assigned_and_borrowed() {
        let program = parse(
            "let main(): i32 = {\n\
               let mut values = [0]\n\
               values[0] = 42\n\
               let item = borrow values[0]\n\
               values[0]\n\
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
            &statements[1],
            Stmt::Expr(Expr::Assign(left, _)) if matches!(left.as_ref(), Expr::Index { .. })
        ));
        assert!(matches!(
            &statements[2],
            Stmt::Let(Binding {
                value: Expr::Borrow { value, .. },
                ..
            }) if matches!(value.as_ref(), Expr::Index { .. })
        ));
        assert!(matches!(tail.as_ref(), Expr::Index { .. }));
    }

    #[test]
    fn parses_explicit_borrow_types() {
        let program = parse(
            "let main(): i32 = {\n\
               let value = 42\n\
               let shared: borrow i32 = borrow value\n\
               let mutable: borrow(mut) i32 = borrow(mut) value\n\
               shared\n\
             }\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Some(Expr::Block(statements, _)) = &function.body else {
            panic!("expected block");
        };
        let Stmt::Let(shared) = &statements[1] else {
            panic!("expected shared borrow binding");
        };
        assert_eq!(
            shared.annotation,
            Some(Type::Borrow {
                mutable: false,
                access: None,
                region: None,
                pointee: Box::new(Type::I32),
            })
        );
        let Stmt::Let(mutable) = &statements[2] else {
            panic!("expected mutable borrow binding");
        };
        assert_eq!(
            mutable.annotation,
            Some(Type::Borrow {
                mutable: true,
                access: None,
                region: None,
                pointee: Box::new(Type::I32),
            })
        );
    }

    #[test]
    fn rejects_legacy_mut_borrow_token_sequence() {
        for source in [
            "let invalid(mut borrow value: i32): i32 = value\n",
            "let invalid(value: mut borrow i32): i32 = value\n",
            "let invalid(value: i32): borrow i32 = mut borrow value\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn parses_region_parameters_and_borrow_regions() {
        let program =
            parse("let choose('a: region)(borrow('a) value: i32): borrow('a) i32 = borrow value\n")
                .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(function.compile_groups[0][0].name, "a");
        assert_eq!(function.compile_groups[0][0].kind, CompileParamKind::Region);
        assert_eq!(function.groups[0][0].region.as_deref(), Some("a"));
        assert_eq!(
            function.return_type,
            Some(Type::Borrow {
                mutable: false,
                access: None,
                region: Some("a".to_owned()),
                pointee: Box::new(Type::I32),
            })
        );
    }

    #[test]
    fn parses_access_parameters_in_borrow_modes_types_and_expressions() {
        let program = parse(
            "let identity(A: access, 'a: region, T: type)\n\
               (borrow(A, 'a) value: T): borrow(A, 'a) T = borrow(A, 'a) value\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(function.compile_groups[0][0].kind, CompileParamKind::Access);
        assert_eq!(function.groups[0][0].access.as_deref(), Some("A"));
        assert_eq!(function.groups[0][0].region.as_deref(), Some("a"));
        assert!(matches!(
            function.return_type,
            Some(Type::Borrow {
                mutable: false,
                access: Some(ref access),
                region: Some(ref region),
                ..
            }) if access == "A" && region == "a"
        ));
        assert!(matches!(
            function.body,
            Some(Expr::Borrow {
                mutable: false,
                access: Some(ref access),
                ..
            }) if access == "A"
        ));
    }

    #[test]
    fn parses_passing_parameters_in_keyword_position() {
        let program = parse("let identity(P: passing, T: type)(P value: T): T = value\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            function.compile_groups[0][0].kind,
            CompileParamKind::Passing
        );
        assert_eq!(function.groups[0][0].mode, PassMode::Inferred);
        assert_eq!(function.groups[0][0].passing.as_deref(), Some("P"));
    }

    #[test]
    fn rejects_passing_parameters_on_data_declarations() {
        let error = parse("let Wrapper(P: passing) = struct(value: i32)\n").unwrap_err();
        assert!(error
            .message
            .contains("passing parameters belong to functions"));
    }

    #[test]
    fn rejects_undeclared_access_parameters() {
        let error = parse("let invalid(borrow(A) value: i32): i32 = value\n").unwrap_err();
        assert!(error.message.contains("undeclared access parameter `A`"));
    }

    #[test]
    fn parses_while_without_treating_its_body_as_a_trailing_closure() {
        let program = parse(
            "let main(): i32 = {\n\
               let mut value = 0\n\
               while ready() {\n\
                 value = value + 1\n\
               }\n\
               value\n\
             }\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Some(Expr::Block(statements, Some(_))) = &function.body else {
            panic!("expected block");
        };
        assert!(matches!(
            &statements[1],
            Stmt::Expr(Expr::While { condition, body })
                if matches!(condition.as_ref(), Expr::Call(_, arguments) if arguments.is_empty())
                    && matches!(body.as_ref(), Expr::Block(_, _))
        ));
    }

    #[test]
    fn parses_loop_with_break_value() {
        let program = parse("let main(): i32 = loop {\n  break 40 + 2\n}\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Some(Expr::Loop { body }) = &function.body else {
            panic!("expected loop");
        };
        assert!(matches!(
            body.as_ref(),
            Expr::Block(_, Some(tail))
                if matches!(
                    tail.as_ref(),
                    Expr::Break(Some(value))
                        if matches!(value.as_ref(), Expr::Binary(_, BinaryOp::Add, _))
                )
        ));
    }

    #[test]
    fn newline_and_comma_end_a_bare_break() {
        let program = parse(
            "let choose(value: bool): i32 = {\n\
               loop {\n\
                 break\n\
                 42\n\
               }\n\
               value match {\n\
                 true => break,\n\
                 false => 0,\n\
               }\n\
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
            &statements[0],
            Stmt::Expr(Expr::Loop { body })
                if matches!(
                    body.as_ref(),
                    Expr::Block(loop_statements, Some(value))
                        if matches!(loop_statements.as_slice(), [Stmt::Expr(Expr::Break(None))])
                            && value.as_ref() == &Expr::Integer(42)
                )
        ));
        assert!(matches!(
            tail.as_ref(),
            Expr::Match { arms, .. } if matches!(arms[0].body, Expr::Break(None))
        ));
    }

    #[test]
    fn array_length_must_be_a_non_negative_decimal_integer() {
        let error = parse("let main(values: Array(i32, -1)): i32 = 0\n").unwrap_err();
        assert!(error.message.contains("non-negative decimal integer"));
    }

    #[test]
    fn parses_extend_methods_associated_functions_constants_and_trait_refs() {
        let program = parse(
            "let A = struct(value: i32)\n\
             extend A: Foo {\n\
               let reset(borrow(mut) self)(): () = {}\n\
               let answer: i32 = 42\n\
               let make(value: i32): A = A(value)\n\
             }\n",
        )
        .unwrap();

        let Item::Extend(extension) = &program.items[1] else {
            panic!("expected extend declaration");
        };
        assert_eq!(extension.target, Type::Named("A".into(), Vec::new()));
        assert_eq!(
            extension.trait_ref,
            Some(Type::Named("Foo".into(), Vec::new()))
        );
        assert_eq!(extension.members.len(), 3);

        let ExtendMember::Function(reset) = &extension.members[0] else {
            panic!("expected method");
        };
        assert_eq!(reset.name, "reset");
        assert_eq!(reset.groups.len(), 2);
        assert_eq!(reset.groups[0].len(), 1);
        assert_eq!(reset.groups[0][0].name, "self");
        assert_eq!(reset.groups[0][0].mode, PassMode::MutBorrow);
        assert_eq!(
            reset.groups[0][0].ty,
            Type::Named("Self".into(), Vec::new())
        );
        assert!(reset.groups[1].is_empty());

        let ExtendMember::Const(answer) = &extension.members[1] else {
            panic!("expected associated constant");
        };
        assert_eq!(answer.name, "answer");
        assert_eq!(answer.annotation, Some(Type::I32));

        let ExtendMember::Function(make) = &extension.members[2] else {
            panic!("expected associated function");
        };
        assert_eq!(make.name, "make");
        assert!(make
            .groups
            .iter()
            .flatten()
            .all(|param| param.name != "self"));
    }

    #[test]
    fn parses_compile_parameters_on_extend_functions() {
        let program = parse(
            "extend A {\n\
               let convert(T: type)(borrow self)(value: T): T = value\n\
               let make(T: type)(value: T): T = value\n\
             }\n",
        )
        .unwrap();

        let Item::Extend(extension) = &program.items[0] else {
            panic!("expected extend declaration");
        };
        let ExtendMember::Function(convert) = &extension.members[0] else {
            panic!("expected generic method");
        };
        assert_eq!(convert.compile_groups[0][0].name, "T");
        assert_eq!(convert.groups.len(), 2);
        assert_eq!(convert.groups[0][0].name, "self");
        assert_eq!(convert.groups[1][0].ty, Type::Named("T".into(), Vec::new()));

        let ExtendMember::Function(make) = &extension.members[1] else {
            panic!("expected generic associated function");
        };
        assert_eq!(make.compile_groups.len(), 1);
        assert_eq!(make.groups.len(), 1);
    }

    #[test]
    fn parses_compile_parameters_on_extend_headers() {
        let program = parse(
            "let Cell(T: type) = struct(value: T)\n\
             extend(T: type) Cell(T)\n\
             where T: Copy {\n\
               let get(borrow self)(): T = self.value\n\
             }\n",
        )
        .unwrap();

        let Item::Extend(extension) = &program.items[1] else {
            panic!("expected extend declaration");
        };
        assert_eq!(extension.compile_groups.len(), 1);
        assert_eq!(extension.where_predicates.len(), 1);
        assert_eq!(extension.compile_groups[0][0].name, "T");
        assert_eq!(
            extension.target,
            Type::Named("Cell".into(), vec![Type::Named("T".into(), Vec::new())])
        );
    }

    #[test]
    fn parses_multiline_where_predicates_without_inference_placeholders() {
        let program = parse(
            "let choose(T: type)(copy value: T): T\n\
             where T: Copy,\n\
                   T: Marker(i32, Item = T), = value\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected a generic function");
        };
        assert_eq!(function.where_predicates.len(), 2);
        assert_eq!(
            function.where_predicates[0].subject,
            Type::Named("T".into(), Vec::new())
        );
        assert_eq!(
            function.where_predicates[1].trait_ref,
            Type::Named("Marker".into(), vec![Type::I32])
        );
        assert_eq!(function.where_predicates[1].associated_types.len(), 1);
        assert_eq!(
            function.where_predicates[1].associated_types[0].name,
            "Item"
        );
        assert_eq!(
            function.where_predicates[1].associated_types[0].ty,
            Type::Named("T".into(), Vec::new())
        );
    }

    #[test]
    fn rejects_invalid_extend_receivers() {
        let cases = [
            (
                "extend A { let invalid(self: A)(): () = {} }\n",
                "cannot have an explicit type",
            ),
            (
                "extend A { let invalid(self, value: i32)(): () = {} }\n",
                "only parameter",
            ),
            (
                "extend A { let invalid(self): () = {} }\n",
                "requires an explicit parameter group",
            ),
            (
                "extend A { let invalid(value: i32)(self)(): () = {} }\n",
                "first parameter group",
            ),
            (
                "extend A { let invalid(self)(self)(): () = {} }\n",
                "at most one",
            ),
        ];

        for (source, expected) in cases {
            let error = parse(source).unwrap_err();
            assert!(
                error.message.contains(expected),
                "expected `{expected}` in `{}`",
                error.message
            );
        }
    }

    #[test]
    fn parses_borrow_and_move_receivers_with_explicit_following_groups() {
        let program = parse(
            "extend A {\n\
               let inspect(borrow self)(): i32 = self.value\n\
               let replace(move self)(value: i32)(other: i32): A = A(value + other)\n\
             }\n",
        )
        .unwrap();

        let Item::Extend(extension) = &program.items[0] else {
            panic!("expected extend declaration");
        };
        let ExtendMember::Function(inspect) = &extension.members[0] else {
            panic!("expected method");
        };
        assert_eq!(inspect.groups[0][0].mode, PassMode::Borrow);
        assert!(inspect.groups[1].is_empty());

        let ExtendMember::Function(replace) = &extension.members[1] else {
            panic!("expected method");
        };
        assert_eq!(replace.groups[0][0].mode, PassMode::Move);
        assert_eq!(replace.groups.len(), 3);
    }

    #[test]
    fn rejects_receivers_outside_extend_and_invalid_extend_members() {
        let receiver = parse("let invalid(self: A)(): () = {}\n").unwrap_err();
        assert!(receiver.message.contains("only allowed in extend"));

        let mutable = parse("extend A { let mut answer = 42 }\n").unwrap_err();
        assert!(mutable.message.contains("let mut"));

        let data = parse("extend A { let Nested = struct(value: i32) }\n").unwrap_err();
        assert!(data.message.contains("data declarations"));

        let missing = parse("extend A { let answer: i32\n}\n").unwrap_err();
        assert!(missing.message.contains("expected `=`"));
    }

    #[test]
    fn reports_a_source_location() {
        let error = parse("let main(): i32 = {\n  let x =\n}\n").unwrap_err();
        assert_eq!((error.line, error.column), (3, 1));
        assert!(error.message.contains("expression"));
    }

    #[test]
    fn parses_optional_fields_and_complete_method_groups() {
        let program = parse(
            "let field = value?.answer\n\
             let called = value?.convert(1)(2)\n",
        )
        .unwrap();
        let Item::Global(field) = &program.items[0] else {
            panic!("expected field binding");
        };
        assert!(matches!(
            &field.value,
            Expr::ChainMember(base, member)
                if matches!(base.as_ref(), Expr::Name(name) if name == "value")
                    && member == "answer"
        ));
        let Item::Global(called) = &program.items[1] else {
            panic!("expected call binding");
        };
        let Expr::Call(first, second_group) = &called.value else {
            panic!("expected second call group");
        };
        let Expr::Call(root, first_group) = first.as_ref() else {
            panic!("expected first call group");
        };
        assert!(matches!(
            root.as_ref(),
            Expr::ChainMember(base, member)
                if matches!(base.as_ref(), Expr::Name(name) if name == "value")
                    && member == "convert"
        ));
        assert_eq!(first_group.len(), 1);
        assert_eq!(second_group.len(), 1);
    }
}
