use std::{collections::HashSet, fmt};

use crate::ast::{
    default_trait_self_parameter, AssociatedKind, AssociatedTypeBinding, BinaryOp, Binding,
    CallArg, CompileParam, CompileParamDefault, EffectDef, EnumDef, Expr, ExtendDef, ExtendMember,
    Field, ForeignAbi, ForeignFunction, Function, FunctionEffects, Item, MatchArm, Param, PassMode,
    Pattern, PatternField, PatternFields, Program, Sort, SortDef, StaticCallArg, StaticExpr, Stmt,
    StructDef, StructRepresentation, TraitDef, TraitMember, Type, TypeAliasDef, TypeArg,
    TypeFormDef, USizeConst, UnaryOp, UseDecl, VariantDef, VariantFields, Visibility,
    WherePredicate,
};
use crate::lexer::{lex, LexError, Token, TokenKind};

mod post_parse;

pub(crate) use post_parse::{infer_extend_parameters, normalize_and_validate_scopes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// Half-open UTF-8 byte range in the original source.
    pub start_byte: usize,
    pub end_byte: usize,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    fn from_lex(source: &str, error: LexError) -> Self {
        let start_byte = byte_offset(source, error.line, error.column);
        let end_byte = source[start_byte..]
            .chars()
            .next()
            .map_or(start_byte, |ch| start_byte + ch.len_utf8());
        Self {
            message: error.message,
            start_byte,
            end_byte,
            line: error.line,
            column: error.column,
        }
    }
}

/// Lexes and parses one Salicin source file.
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = lex(source).map_err(|error| ParseError::from_lex(source, error))?;
    parse_tokens(tokens)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SourceLayout {
    pub parameter_groups: Vec<usize>,
    pub repeated_parameter_groups: Vec<usize>,
    pub where_predicates: Vec<usize>,
    pub match_arms: Vec<SourceBracedRegion>,
    pub blocks: Vec<SourceBracedRegion>,
    pub closures: Vec<SourceBracedRegion>,
    pub trailing_closures: Vec<SourceTrailingClosure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceBracedRegion {
    pub open_byte: usize,
    pub close_byte: usize,
    pub body_start_byte: usize,
    pub open_line: usize,
    pub close_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceTrailingClosure {
    pub start_byte: usize,
    pub close_byte: usize,
}

pub(crate) fn parse_with_source_layout(
    source: &str,
) -> Result<(Program, SourceLayout), ParseError> {
    let tokens = lex(source).map_err(|error| ParseError::from_lex(source, error))?;
    let mut parser = Parser::new(tokens);
    let program = parser.program()?;
    Ok((program, parser.layout))
}

fn byte_offset(source: &str, line: usize, column: usize) -> usize {
    let mut current_line = 1;
    let mut current_column = 1;
    for (offset, ch) in source.char_indices() {
        if current_line == line && current_column == column {
            return offset;
        }
        if ch == '\n' {
            current_line += 1;
            current_column = 1;
        } else {
            current_column += 1;
        }
    }
    source.len()
}

/// Parses a token stream produced by [`crate::lexer::lex`].
pub fn parse_tokens(tokens: Vec<Token>) -> Result<Program, ParseError> {
    Parser::new(tokens).program()
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    /// Names that may occur in `with(...)`, including individual `effect`
    /// identities and complete `effects` rows.
    effect_parameters_in_scope: HashSet<String>,
    next_control_binding: usize,
    async_depth: usize,
    layout: SourceLayout,
}

type DeclarationGroups = (
    Vec<Vec<CompileParam>>,
    Vec<Vec<Param>>,
    FunctionEffects,
    bool,
    bool,
);

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            index: 0,
            effect_parameters_in_scope: HashSet::new(),
            next_control_binding: 0,
            async_depth: 0,
            layout: SourceLayout::default(),
        }
    }

    fn program(&mut self) -> Result<Program, ParseError> {
        let mut items = Vec::new();
        let mut item_visibilities = Vec::new();
        let mut item_origins = Vec::new();
        let mut uses = Vec::new();
        self.skip_separators();

        while !self.at(&TokenKind::Eof) {
            let visibility = self.visibility()?;
            if self.qualified_alias_declaration_follows() {
                uses.push(self.qualified_alias_declaration(visibility)?);
            } else if self.at_context_ident("use") {
                uses.extend(self.use_declaration(visibility)?);
            } else if self.at_context_ident("extern") {
                return Err(self.error_here(
                    "grouped `extern` declarations have been removed; use `let name(...): result = foreign(c, \"symbol\")`",
                ));
            } else if self.at_context_ident("test") {
                if visibility != Visibility::Private {
                    return Err(self.error_here("test declarations cannot have visibility"));
                }
                let start = self.current().clone();
                items.push(Item::Function(self.test_declaration()?));
                item_visibilities.push(Visibility::Private);
                item_origins.push(crate::ast::ItemOrigin {
                    source: Some(Box::new(crate::ast::SourceLocation {
                        path: None,
                        line: start.line,
                        column: start.column,
                        end_line: start.end_line,
                        end_column: start.end_column,
                    })),
                    ..crate::ast::ItemOrigin::default()
                });
            } else {
                if visibility != Visibility::Private && self.at(&TokenKind::Extend) {
                    return Err(self.error_here("`extend` declarations cannot have visibility"));
                }
                let start = self.current().clone();
                items.push(self.item()?);
                item_visibilities.push(visibility);
                item_origins.push(crate::ast::ItemOrigin {
                    source: Some(Box::new(crate::ast::SourceLocation {
                        path: None,
                        line: start.line,
                        column: start.column,
                        end_line: start.end_line,
                        end_column: start.end_column,
                    })),
                    ..crate::ast::ItemOrigin::default()
                });
            }
            if !self.at(&TokenKind::Eof) && !self.at_separator() {
                return Err(self.error_here("expected a newline or `;` after declaration"));
            }
            self.skip_separators();
        }

        if let Err(message) = infer_extend_parameters(&mut items) {
            return Err(self.error_here(message));
        }
        if let Err(message) = normalize_and_validate_scopes(&mut items) {
            return Err(self.error_here(message));
        }
        Ok(Program::with_metadata(
            items,
            item_visibilities,
            item_origins,
            uses,
        ))
    }

    fn test_declaration(&mut self) -> Result<Function, ParseError> {
        self.advance();
        self.expect(&TokenKind::LParen, "`(` after `test`")?;
        let TokenKind::String(name) = self.current().kind.clone() else {
            return Err(self.error_here("test name must be a string literal"));
        };
        if name.is_empty() {
            return Err(self.error_here("test name cannot be empty"));
        }
        self.advance();
        self.expect(&TokenKind::RParen, "`)` after test name")?;
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("expected a trailing braced test body"));
        }
        let body = self.block()?;
        let encoded_name = name
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Ok(Function {
            name: format!("$test${encoded_name}"),
            foreign: None,
            builtin: false,
            compile_groups: Vec::new(),
            groups: vec![Vec::new()],
            return_type: Some(Type::Bool),
            effects: FunctionEffects {
                custom: vec![Type::Named("core.testing.failure".to_owned(), Vec::new())],
                ..FunctionEffects::default()
            },
            where_predicates: Vec::new(),
            body: Some(body),
        })
    }

    fn qualified_alias_declaration_follows(&self) -> bool {
        if !self.at(&TokenKind::Let)
            || !matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(_) | TokenKind::Mut)
            )
            || !self.at_offset(2, &TokenKind::Equal)
        {
            return false;
        }

        let mut index = self.index + 3;
        if !matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Ident(_) | TokenKind::Root | TokenKind::Super)
        ) {
            return false;
        }
        index += 1;
        let mut segments = 1;
        while matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Dot)
        ) {
            if !matches!(
                self.tokens.get(index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(_) | TokenKind::Mut | TokenKind::Super)
            ) {
                return false;
            }
            segments += 1;
            index += 2;
        }
        (segments > 1
            || self.tokens.get(self.index + 3).is_some_and(|token| {
                matches!(token.kind, TokenKind::Root | TokenKind::Super)
                    || matches!(&token.kind, TokenKind::Ident(name) if name == "self")
            }))
            && matches!(
                self.tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::Newline | TokenKind::Semicolon | TokenKind::Eof)
            )
    }

    fn qualified_alias_declaration(
        &mut self,
        visibility: Visibility,
    ) -> Result<UseDecl, ParseError> {
        self.expect(&TokenKind::Let, "`let`")?;
        let alias = if self.take(&TokenKind::Mut) {
            "mut".to_owned()
        } else {
            self.expect_ident("an alias name")?
        };
        self.expect(&TokenKind::Equal, "`=` in alias declaration")?;
        let mut path = vec![self.expect_path_start("an alias target path")?];
        while self.take(&TokenKind::Dot) {
            path.push(
                self.expect_path_continuation(&path, "an alias target path segment after `.`")?,
            );
        }
        Ok(UseDecl {
            visibility,
            path,
            alias: Some(alias),
        })
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
        if !self.at_context_ident("use") {
            return Err(self.error_here("expected `use`"));
        }
        self.advance();
        let mut path = vec![self.expect_path_start("an import path")?];

        while self.take(&TokenKind::Dot) {
            if self.take(&TokenKind::LBrace) {
                return self.use_group(visibility, path);
            }
            let segment =
                self.expect_path_continuation(&path, "an import path segment after `.`")?;
            path.push(segment);
        }

        let alias = if self.at_context_ident("as") {
            self.advance();
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
            let alias = if self.at_context_ident("as") {
                self.advance();
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
        let name = self.declaration_name()?;

        let (compile_groups, groups, mut effects, has_callable_boundary, mut has_effect_clause) =
            self.declaration_groups(false, &[])?;

        if mutable && (!compile_groups.is_empty() || !groups.is_empty()) {
            return Err(self.error_here("`let mut` cannot declare a function"));
        }

        if groups.is_empty() && self.at(&TokenKind::Colon) {
            if matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(name)) if name == "domain"
            ) {
                return Err(self.error_here(
                    "`domain` was removed; user code must declare a finite sort with `let name = sort(1) { ... }` because abstract sorts are compiler-owned",
                ));
            }
            if matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(name)) if name == "sort"
            ) && compile_groups.is_empty()
            {
                if mutable {
                    return Err(self.error_here("abstract sort declarations cannot be mutable"));
                }
                self.advance();
                self.advance();
                let level = self.sort_level_literal()?;
                return Ok(Item::Sort(SortDef {
                    name,
                    level,
                    members: None,
                }));
            }
            if self.at_offset(1, &TokenKind::Type) {
                self.advance();
                self.advance();
                self.take_newlines_if_followed_by(&[TokenKind::Equal]);
                return if self.at(&TokenKind::Equal) {
                    if self.builtin_initializer_follows(1) {
                        self.advance();
                        self.consume_builtin_initializer()?;
                        self.type_form_definition(name, compile_groups, mutable, true)
                    } else {
                        self.type_alias(name, compile_groups, mutable)
                    }
                } else {
                    self.type_form_definition(name, compile_groups, mutable, false)
                };
            }
            if self.type_constructor_signature_follows() {
                self.advance();
                let mut alias_groups = Vec::new();
                while self.group_starts_with_compile_parameter() {
                    alias_groups.push(self.compile_parameter_group()?);
                }
                self.expect(&TokenKind::Colon, "`:` before type-constructor result sort")?;
                self.expect(&TokenKind::Type, "`type` as type-constructor result sort")?;
                self.take_newlines_if_followed_by(&[TokenKind::Equal]);
                return if self.at(&TokenKind::Equal) && self.builtin_initializer_follows(1) {
                    self.advance();
                    self.consume_builtin_initializer()?;
                    self.type_form_definition(name, alias_groups, mutable, true)
                } else {
                    self.type_constructor_alias(name, alias_groups, mutable)
                };
            }
        }

        let logical_result = if self.take(&TokenKind::Colon) {
            Some(self.function_result_type()?)
        } else {
            None
        };
        if !has_callable_boundary {
            let (legacy_effects, _failure_error, legacy_has_effect_clause) =
                self.function_effect_clause()?;
            effects = legacy_effects;
            has_effect_clause = legacy_has_effect_clause;
        }
        let failure_error = effects.failure.as_deref().cloned();
        let annotation =
            logical_result.map(|result| Self::apply_failure_effect(result, failure_error.clone()));
        self.effect_parameters_in_scope.clear();

        if !compile_groups.is_empty() || !groups.is_empty() {
            self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::Equal]);
        }

        if self.at(&TokenKind::Where) {
            return Err(self.error_here(
                "colon-style `where` predicates were removed; write `= requires(t is trait) ...`",
            ));
        }
        let mut where_predicates = Vec::new();
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);

        if !self.at(&TokenKind::Equal) && (!compile_groups.is_empty() || !groups.is_empty()) {
            return Ok(Item::Function(Function {
                name,
                foreign: None,
                builtin: false,
                compile_groups,
                groups,
                return_type: annotation,
                effects,
                where_predicates,
                body: None,
            }));
        }

        self.expect(&TokenKind::Equal, "`=`")?;

        if self.at_context_ident("requires") {
            self.advance();
            where_predicates.extend(self.constraint_arguments("`(` after `requires`")?);
            if self.at_separator() || self.at(&TokenKind::Eof) {
                return Ok(Item::Function(Function {
                    name,
                    foreign: None,
                    builtin: false,
                    compile_groups,
                    groups,
                    return_type: annotation,
                    effects,
                    where_predicates,
                    body: None,
                }));
            }
        }

        if self.at_context_ident("builtin") {
            if mutable {
                return Err(self.error_here("builtin definitions cannot be mutable"));
            }
            if compile_groups.is_empty() && groups.is_empty() {
                return Err(self.error_here(
                    "`builtin()` defines a function or type declaration, not a global value",
                ));
            }
            let builtin_bootstrap = name == "builtin"
                && compile_groups.is_empty()
                && matches!(groups.as_slice(), [group] if group.is_empty());
            if builtin_bootstrap && annotation.is_some() {
                return Err(self.error_here(
                    "the compiler-definition bootstrap has exact shape `let builtin() = builtin()`",
                ));
            }
            if annotation.is_none() && !builtin_bootstrap {
                return Err(self.error_here(
                    "builtin functions require an explicit result type before `= builtin()`",
                ));
            }
            self.consume_builtin_initializer()?;
            return Ok(Item::Function(Function {
                name,
                foreign: None,
                builtin: true,
                compile_groups,
                groups,
                return_type: annotation
                    .or(builtin_bootstrap.then_some(Type::Named("never".to_owned(), Vec::new()))),
                effects,
                where_predicates,
                body: None,
            }));
        }

        if self.at_context_ident("foreign") {
            if mutable {
                return Err(self.error_here("foreign declarations cannot be mutable"));
            }
            if compile_groups.is_empty() && groups.is_empty() {
                return Err(self.error_here(
                    "`foreign(...)` defines a function declaration and requires one runtime parameter group",
                ));
            }
            if !compile_groups.is_empty() {
                return Err(self.error_here("foreign functions cannot be generic"));
            }
            if groups.len() != 1 {
                return Err(
                    self.error_here("C ABI functions require exactly one runtime parameter group")
                );
            }
            if annotation.is_none() {
                return Err(self.error_here(
                    "foreign functions require an explicit result type before `= foreign(...)`",
                ));
            }
            if has_effect_clause {
                return Err(self.error_here(
                    "foreign declarations acquire `Unsafe` implicitly and cannot declare effects",
                ));
            }
            if !where_predicates.is_empty() {
                return Err(self.error_here("foreign functions cannot use `where` clauses"));
            }
            let foreign = self.foreign_initializer(&name)?;
            return Ok(Item::Function(Function {
                name,
                foreign: Some(foreign),
                builtin: false,
                compile_groups: Vec::new(),
                groups,
                return_type: annotation,
                effects: FunctionEffects {
                    unsafety: true,
                    ..FunctionEffects::default()
                },
                where_predicates: Vec::new(),
                body: None,
            }));
        }

        if self.at_context_ident("type") {
            return Err(self.error_here(
                "`type` is an abstract sort and cannot appear as a declaration value; write `let name: type`",
            ));
        }

        if self.at_context_ident("effect") {
            if mutable
                || annotation.is_some()
                || has_callable_boundary
                || !groups.is_empty()
                || !where_predicates.is_empty()
            {
                return Err(self.error_here(
                    "effect declarations cannot be mutable, annotated, have runtime parameters, or use where clauses",
                ));
            }
            self.advance();
            return self
                .effect_definition(name, compile_groups)
                .map(Item::Effect);
        }

        if self.at_context_ident("domain") {
            return Err(self.error_here(
                "`domain` was removed; declare a finite sort with `let name = sort(1) { ... }`",
            ));
        }

        if self.at_context_ident("sort") {
            if mutable
                || annotation.is_some()
                || has_callable_boundary
                || !compile_groups.is_empty()
                || !groups.is_empty()
                || !where_predicates.is_empty()
            {
                return Err(self.error_here(
                    "sort declarations cannot be mutable, generic, annotated, or have parameters",
                ));
            }
            self.advance();
            let level = self.sort_level_literal()?;
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here(
                    "abstract sorts use `let name: sort(n)`; an empty defined sort uses `let name = sort(n) {}`",
                ));
            }
            return self.sort_definition(name, level).map(Item::Sort);
        }

        if self.at(&TokenKind::Struct) || self.at(&TokenKind::Enum) || self.at(&TokenKind::Trait) {
            if mutable || annotation.is_some() || has_callable_boundary || !groups.is_empty() {
                return Err(self.error_here(
                    "data declarations cannot be mutable, annotated, or have runtime parameters",
                ));
            }
            return if self.at(&TokenKind::Struct) {
                if !where_predicates.is_empty() {
                    return Err(self.error_here("struct declarations cannot use `where` clauses"));
                }
                self.struct_definition(name, compile_groups)
                    .map(Item::Struct)
            } else if self.at(&TokenKind::Enum) {
                if !where_predicates.is_empty() {
                    return Err(self.error_here("enum declarations cannot use `where` clauses"));
                }
                self.enum_definition(name, compile_groups).map(Item::Enum)
            } else {
                if !where_predicates.is_empty() {
                    return Err(self.error_here(
                        "trait inheritance constraints are written after `trait`, before `{`",
                    ));
                }
                self.trait_definition(name, compile_groups).map(Item::Trait)
            };
        }

        if compile_groups.is_empty() && groups.is_empty() {
            if has_callable_boundary {
                return Err(self.error_here("effect annotations require a function declaration"));
            }
            let value = self.expression(true)?;
            Ok(Item::Global(Binding {
                value_source: None,
                mutable,
                name,
                annotation,
                value,
            }))
        } else {
            let transparent_modifier = groups.is_empty()
                && annotation.is_none()
                && compile_groups.len() == 1
                && compile_groups[0].len() == 1
                && compile_groups[0][0].kind == Sort::ParameterModifier;
            if transparent_modifier && !self.at(&TokenKind::LBrace) {
                let body = self.expression(true)?;
                return Ok(Item::Function(Function {
                    name,
                    foreign: None,
                    builtin: false,
                    compile_groups,
                    groups,
                    return_type: None,
                    effects,
                    where_predicates,
                    body: Some(body),
                }));
            }
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here(
                    "named closure declarations require a braced body; write `= { expression }`",
                ));
            }
            let body = self.block()?;
            Ok(Item::Function(Function {
                name,
                foreign: None,
                builtin: false,
                compile_groups,
                groups,
                return_type: annotation,
                effects,
                where_predicates,
                body: Some(body),
            }))
        }
    }

    fn foreign_initializer(
        &mut self,
        declaration_name: &str,
    ) -> Result<ForeignFunction, ParseError> {
        self.advance();
        self.expect(&TokenKind::LParen, "`(` after `foreign`")?;
        let abi = self.expect_ident("a foreign ABI name")?;
        if abi != "c" {
            return Err(self.error_here(format!(
                "unsupported foreign ABI `{abi}`; only `foreign(c)` is available"
            )));
        }
        let link_name = if self.take(&TokenKind::Comma) {
            let TokenKind::String(link_name) = self.current().kind.clone() else {
                return Err(self.error_here(
                    "the optional second `foreign` argument must be a linker symbol string",
                ));
            };
            self.advance();
            link_name
        } else {
            declaration_name.to_owned()
        };
        self.expect(&TokenKind::RParen, "`)` after `foreign` initializer")?;
        if !foreign_link_name_is_valid(&link_name) {
            return Err(self.error_here(format!(
                "foreign link name `{link_name}` must be a non-empty ASCII linker symbol"
            )));
        }
        Ok(ForeignFunction {
            abi: ForeignAbi::C,
            link_name,
        })
    }

    fn effect_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
    ) -> Result<EffectDef, ParseError> {
        if !self.take(&TokenKind::LBrace) {
            return Ok(EffectDef {
                name,
                compile_groups,
                operations: Vec::new(),
            });
        }
        self.skip_separators();
        let mut operations = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            self.expect(&TokenKind::Let, "`let` in effect body")?;
            if self.take(&TokenKind::Mut) {
                return Err(self.error_here("effect operations cannot use `let mut`"));
            }
            let operation = self.expect_ident("an effect operation name")?;
            if operation == "handle" || operation == "done" {
                return Err(self.error_here(format!(
                    "effect operation name `{operation}` is reserved by handler lowering"
                )));
            }
            let (
                operation_compile_groups,
                groups,
                mut effects,
                has_callable_boundary,
                _has_effect_clause,
            ) = self.declaration_groups(false, &[])?;
            if !operation_compile_groups.is_empty() {
                return Err(self.error_here(
                    "compile-time parameters on effect operations are not supported yet",
                ));
            }
            if groups.is_empty() {
                return Err(self.error_here(
                    "effect operations require an explicit runtime parameter group; use `()` for no arguments",
                ));
            }
            self.expect(&TokenKind::Colon, "`:` before effect operation result type")?;
            let logical_result = self.function_result_type()?;
            if !has_callable_boundary {
                effects = self.function_effect_clause()?.0;
            }
            let failure_error = effects.failure.as_deref().cloned();
            let return_type = Some(Self::apply_failure_effect(logical_result, failure_error));
            self.effect_parameters_in_scope.clear();
            if self.at(&TokenKind::Where) {
                return Err(
                    self.error_here("where clauses on effect operations are not supported yet")
                );
            }
            if self.at(&TokenKind::Equal) {
                return Err(
                    self.error_here("effect operations are requirements and cannot have bodies")
                );
            }
            let labels = groups
                .iter()
                .flatten()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>();
            if operations.iter().any(|candidate: &Function| {
                candidate.name == operation
                    && candidate
                        .groups
                        .iter()
                        .flatten()
                        .map(|parameter| parameter.name.as_str())
                        .eq(labels.iter().copied())
            }) {
                return Err(self.error_here(format!(
                    "duplicate effect operation `{name}.{operation}` with the same parameter names"
                )));
            }
            operations.push(Function {
                name: operation,
                foreign: None,
                builtin: false,
                compile_groups: operation_compile_groups,
                groups,
                return_type,
                effects,
                where_predicates: Vec::new(),
                body: None,
            });
            if !self.at(&TokenKind::RBrace) && !self.at_separator() {
                return Err(self.error_here("expected a newline or `;` after effect operation"));
            }
            self.skip_separators();
        }
        self.expect(&TokenKind::RBrace, "`}` after effect operations")?;
        Ok(EffectDef {
            name,
            compile_groups,
            operations,
        })
    }

    fn type_constructor_signature_follows(&self) -> bool {
        self.at(&TokenKind::Colon)
            && self.at_offset(1, &TokenKind::LParen)
            && self.at_offset(2, &TokenKind::Comptime)
            && matches!(
                self.tokens.get(self.index + 3).map(|token| &token.kind),
                Some(TokenKind::Ident(_))
            )
            && self.at_offset(4, &TokenKind::Colon)
            && self.at_offset(5, &TokenKind::Type)
    }

    fn type_alias(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
        mutable: bool,
    ) -> Result<Item, ParseError> {
        if mutable {
            return Err(self.error_here("type aliases cannot be declared with `let mut`"));
        }
        if compile_groups.iter().flatten().any(|parameter| {
            !matches!(
                parameter.kind,
                Sort::Type | Sort::Region | Sort::USize | Sort::Named(_)
            )
        }) {
            return Err(self.error_here(
                "type aliases accept only `type`, `region`, `usize`, or closed-value compile-time parameters",
            ));
        }
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);
        self.expect(&TokenKind::Equal, "`=` in type alias")?;
        let target = self.type_expr()?;
        Ok(Item::TypeAlias(TypeAliasDef {
            name,
            compile_groups,
            target,
        }))
    }

    fn type_form_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
        mutable: bool,
        builtin: bool,
    ) -> Result<Item, ParseError> {
        if mutable {
            return Err(self.error_here("type forms cannot be declared with `let mut`"));
        }
        Ok(Item::TypeForm(TypeFormDef {
            name,
            compile_groups,
            values: Vec::new(),
            builtin,
        }))
    }

    fn builtin_initializer_follows(&self, offset: usize) -> bool {
        matches!(
            self.tokens.get(self.index + offset).map(|token| &token.kind),
            Some(TokenKind::Ident(name)) if name == "builtin"
        ) && matches!(
            self.tokens
                .get(self.index + offset + 1)
                .map(|token| &token.kind),
            Some(TokenKind::LParen)
        ) && matches!(
            self.tokens
                .get(self.index + offset + 2)
                .map(|token| &token.kind),
            Some(TokenKind::RParen)
        )
    }

    fn consume_builtin_initializer(&mut self) -> Result<(), ParseError> {
        self.advance();
        self.expect(&TokenKind::LParen, "`(` after `builtin`")?;
        self.expect(
            &TokenKind::RParen,
            "`)` after compiler definition marker `builtin(`",
        )
    }

    fn type_constructor_alias(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
        mutable: bool,
    ) -> Result<Item, ParseError> {
        if mutable {
            return Err(
                self.error_here("type-constructor aliases cannot be declared with `let mut`")
            );
        }
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);
        self.expect(&TokenKind::Equal, "`=` in type-constructor alias")?;
        let target = self.type_expr()?;
        let Type::Named(target_name, target_arguments) = target else {
            return Err(
                self.error_here("a type-constructor alias must name another type constructor")
            );
        };
        if !target_arguments.is_empty() {
            return Err(self.error_here(
                "a type-constructor alias target must be unapplied; use a parameterized type alias for an applied result",
            ));
        }
        let arguments = compile_groups
            .iter()
            .flatten()
            .map(|parameter| Type::Named(parameter.name.clone(), Vec::new()))
            .collect();
        Ok(Item::TypeAlias(TypeAliasDef {
            name,
            compile_groups,
            target: Type::Named(target_name, arguments),
        }))
    }

    fn declaration_name(&mut self) -> Result<String, ParseError> {
        let name = match &self.current().kind {
            TokenKind::Ident(name) => name.clone(),
            TokenKind::Type => "type".to_owned(),
            TokenKind::Region => "region".to_owned(),
            TokenKind::Borrow => "borrow".to_owned(),
            TokenKind::Do => "do".to_owned(),
            TokenKind::Try => "try".to_owned(),
            TokenKind::Throw => "throw".to_owned(),
            TokenKind::Unsafe => "unsafe".to_owned(),
            TokenKind::Loop => "loop".to_owned(),
            _ => {
                return Err(self.error_here(format!(
                    "expected a declaration name, found {}",
                    describe(&self.current().kind)
                )))
            }
        };
        self.advance();
        Ok(name)
    }

    fn sort_level_literal(&mut self) -> Result<u64, ParseError> {
        self.expect(&TokenKind::LParen, "`(` after `sort`")?;
        let token = self.current().clone();
        let TokenKind::Integer(level) = token.kind else {
            return Err(self.error_at(
                &token,
                "top-level sort levels must be positive integer literals",
            ));
        };
        let level = u64::try_from(level)
            .map_err(|_| self.error_at(&token, "sort level does not fit in `usize`"))?;
        if level == 0 {
            return Err(self.error_at(&token, "`sort(0)` is invalid; sort levels start at 1"));
        }
        self.advance();
        self.expect(&TokenKind::RParen, "`)` after sort level")?;
        Ok(level)
    }

    fn compile_sort_level(&mut self) -> Result<crate::ast::SortLevel, ParseError> {
        self.expect(&TokenKind::LParen, "`(` after `sort`")?;
        let token = self.current().clone();
        let level = match token.kind {
            TokenKind::Integer(level) => {
                let level = u64::try_from(level)
                    .map_err(|_| self.error_at(&token, "sort level does not fit in `usize`"))?;
                if level == 0 {
                    return Err(
                        self.error_at(&token, "`sort(0)` is invalid; sort levels start at 1")
                    );
                }
                crate::ast::SortLevel::Literal(level)
            }
            TokenKind::Ident(name) => crate::ast::SortLevel::Parameter(name),
            _ => {
                return Err(self.error_at(
                    &token,
                    "sort level must be a positive integer literal or a compile-time parameter",
                ))
            }
        };
        self.advance();
        self.expect(&TokenKind::RParen, "`)` after sort level")?;
        Ok(level)
    }

    fn sort_definition(&mut self, name: String, level: u64) -> Result<SortDef, ParseError> {
        let members = if self.take(&TokenKind::LBrace) {
            self.skip_separators();
            let mut members = Vec::new();
            let mut seen = HashSet::new();
            while !self.take(&TokenKind::RBrace) {
                let member = self.sort_member_name()?;
                if !seen.insert(member.clone()) {
                    return Err(self.error_here(format!("duplicate sort member `{member}`")));
                }
                members.push(member);

                self.take(&TokenKind::Comma);
                self.skip_separators();
            }
            Some(members)
        } else {
            None
        };
        Ok(SortDef {
            name,
            level,
            members,
        })
    }

    fn sort_member_name(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        let name = match token.kind {
            TokenKind::Ident(name) if name != "_" => name,
            TokenKind::Mut => "mut".to_owned(),
            TokenKind::Copy => "copy".to_owned(),
            TokenKind::Move => "move".to_owned(),
            TokenKind::True => "true".to_owned(),
            TokenKind::False => "false".to_owned(),
            TokenKind::Type => "type".to_owned(),
            TokenKind::Region => "region".to_owned(),
            _ => {
                return Err(self.error_at(
                    &token,
                    format!(
                        "expected a sort member name, found {}",
                        describe(&token.kind)
                    ),
                ))
            }
        };
        self.advance();
        Ok(name)
    }

    fn extend_definition(&mut self) -> Result<ExtendDef, ParseError> {
        self.expect(&TokenKind::Extend, "`extend`")?;
        self.expect(&TokenKind::LParen, "`(` after `extend`")?;
        let target = self.type_expr()?;
        let trait_ref = if self.take(&TokenKind::Comma) {
            Some(self.type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::RParen, "`)` after extend arguments")?;
        self.take_newlines_if_followed_by(&[TokenKind::LParen, TokenKind::LBrace]);
        let where_predicates = if self.at(&TokenKind::LParen) {
            self.requires_parameter_group()?
        } else if self.at(&TokenKind::Where) {
            return Err(self.error_here(
                "`where` extension predicates were removed; write `(requires: t is trait)`",
            ));
        } else {
            Vec::new()
        };
        self.take_newlines_if_followed_by(&[TokenKind::LBrace]);
        self.expect(
            &TokenKind::LBrace,
            "trailing implementation block after `extend(...)`",
        )?;
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
            compile_groups: Vec::new(),
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

        let (compile_groups, groups, mut effects, has_callable_boundary, _has_effect_clause) =
            self.declaration_groups(true, &[])?;
        self.validate_receiver_groups(&name, &groups)?;

        let logical_result = if self.take(&TokenKind::Colon) {
            Some(self.function_result_type()?)
        } else {
            None
        };
        if !has_callable_boundary {
            effects = self.function_effect_clause()?.0;
        }
        let failure_error = effects.failure.as_deref().cloned();
        let annotation =
            logical_result.map(|result| Self::apply_failure_effect(result, failure_error.clone()));
        self.effect_parameters_in_scope.clear();
        if !compile_groups.is_empty() || !groups.is_empty() {
            self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::Equal]);
        }
        if self.at(&TokenKind::Where) {
            return Err(self.error_here(
                "colon-style extension-member predicates were removed; write `= requires(t is trait) ...`",
            ));
        }
        let where_predicates = Vec::new();
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);
        if !self.at(&TokenKind::Equal) && (!compile_groups.is_empty() || !groups.is_empty()) {
            return Ok(ExtendMember::Function(Function {
                name,
                foreign: None,
                builtin: false,
                compile_groups,
                groups,
                return_type: annotation,
                effects,
                where_predicates,
                body: None,
            }));
        }
        self.expect(&TokenKind::Equal, "`=` in extend member")?;

        let mut where_predicates = where_predicates;
        if self.at_context_ident("requires") {
            self.advance();
            where_predicates.extend(self.constraint_arguments("`(` after `requires`")?);
            if self.at_separator() || self.at(&TokenKind::RBrace) {
                return Ok(ExtendMember::Function(Function {
                    name,
                    foreign: None,
                    builtin: false,
                    compile_groups,
                    groups,
                    return_type: annotation,
                    effects,
                    where_predicates,
                    body: None,
                }));
            }
        }

        if self.at_context_ident("builtin") {
            if compile_groups.is_empty() && groups.is_empty() {
                return Err(self.error_here("`builtin()` cannot define an associated constant"));
            }
            if annotation.is_none() {
                return Err(self.error_here(
                    "builtin methods require an explicit result type before `= builtin()`",
                ));
            }
            self.consume_builtin_initializer()?;
            return Ok(ExtendMember::Function(Function {
                name,
                foreign: None,
                builtin: true,
                compile_groups,
                groups,
                return_type: annotation,
                effects,
                where_predicates,
                body: None,
            }));
        }

        if self.at(&TokenKind::Struct) || self.at(&TokenKind::Enum) || self.at(&TokenKind::Trait) {
            return Err(self.error_here("data declarations are not allowed in extend bodies"));
        }

        if compile_groups.is_empty() && groups.is_empty() {
            if has_callable_boundary {
                return Err(self.error_here("effect annotations require a function member"));
            }
            Ok(ExtendMember::Const(Binding {
                value_source: None,
                mutable: false,
                name,
                annotation,
                value: self.expression(true)?,
            }))
        } else {
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here(
                    "named closure declarations require a braced body; write `= { expression }`",
                ));
            }
            let body = self.block()?;
            Ok(ExtendMember::Function(Function {
                name,
                foreign: None,
                builtin: false,
                compile_groups,
                groups,
                return_type: annotation,
                effects,
                where_predicates,
                body: Some(body),
            }))
        }
    }

    fn constraint_arguments(
        &mut self,
        opening_description: &str,
    ) -> Result<Vec<WherePredicate>, ParseError> {
        self.expect(&TokenKind::LParen, opening_description)?;
        self.constraint_expressions_until_rparen()
    }

    fn requires_parameter_group(&mut self) -> Result<Vec<WherePredicate>, ParseError> {
        self.expect(&TokenKind::LParen, "`(` before `requires:`")?;
        let label = self.expect_ident("`requires` constraint parameter label")?;
        if label != "requires" {
            return Err(self.error_here(format!(
                "expected constraint parameter label `requires`, found `{label}`"
            )));
        }
        self.expect(&TokenKind::Colon, "`:` after `requires`")?;
        self.constraint_expressions_until_rparen()
    }

    fn constraint_expressions_until_rparen(&mut self) -> Result<Vec<WherePredicate>, ParseError> {
        let mut predicates = Vec::new();
        loop {
            self.constraint_expression(&mut predicates)?;
            if self.take(&TokenKind::AndAnd) || self.take(&TokenKind::Comma) {
                if self.at(&TokenKind::RParen) {
                    break;
                }
                continue;
            }
            break;
        }
        self.expect(&TokenKind::RParen, "`)` after compile-time constraints")?;
        Ok(predicates)
    }

    fn constraint_expression(
        &mut self,
        predicates: &mut Vec<WherePredicate>,
    ) -> Result<(), ParseError> {
        self.layout.where_predicates.push(self.current().start_byte);
        let mut path = vec![self.expect_path_start("a constraint subject type")?];
        while self.at(&TokenKind::Dot) {
            self.advance();
            path.push(
                self.expect_path_continuation(
                    &path,
                    "a constraint subject path segment after `.`",
                )?,
            );
        }

        if self.at_context_ident("is") {
            self.advance();
            let (trait_ref, associated_types) = self.where_trait_ref()?;
            if !associated_types.is_empty() {
                return Err(self.error_here(
                    "associated type constraints are separate projection equalities; write `t is trait && t.item == type`",
                ));
            }
            predicates.push(WherePredicate {
                subject: Type::Named(path.join("."), Vec::new()),
                trait_ref,
                associated_types,
            });
            return Ok(());
        }

        if path.len() < 2 {
            return Err(self.error_here(
                "expected compile-time `is` or an associated type projection equality",
            ));
        }
        let name = path.pop().expect("projection path has a member");
        let mut compile_groups = Vec::new();
        while self.group_starts_with_compile_parameter() {
            compile_groups.push(self.compile_parameter_group()?);
        }
        self.expect(
            &TokenKind::EqualEqual,
            "`==` in associated type projection equality",
        )?;
        let ty = self.type_expr()?;
        let subject = Type::Named(path.join("."), Vec::new());
        let Some(predicate) = predicates
            .iter_mut()
            .rev()
            .find(|predicate| predicate.subject == subject)
        else {
            return Err(self.error_here(
                "an associated type projection equality must follow an `is` constraint for the same subject",
            ));
        };
        predicate.associated_types.push(AssociatedTypeBinding {
            name,
            compile_groups,
            ty,
        });
        Ok(())
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
        let mut labeled = 0;
        if self.take(&TokenKind::LParen) && !self.take(&TokenKind::RParen) {
            loop {
                let starts_associated_binding = matches!(self.current().kind, TokenKind::Ident(_))
                    && (self.at_offset(1, &TokenKind::Equal)
                        || (self.at_offset(1, &TokenKind::LParen)
                            && self.at_offset(2, &TokenKind::Comptime)
                            && matches!(
                                self.tokens.get(self.index + 3).map(|token| &token.kind),
                                Some(TokenKind::Ident(_)) | Some(TokenKind::RegionName(_))
                            )
                            && self.at_offset(4, &TokenKind::Colon)));
                if starts_associated_binding {
                    saw_associated = true;
                    let binding = self.expect_ident("an associated type name")?;
                    let mut compile_groups = Vec::new();
                    while self.at(&TokenKind::LParen) {
                        compile_groups.push(self.compile_parameter_group()?);
                    }
                    self.expect(&TokenKind::Equal, "`=` in associated type equality")?;
                    associated_types.push(AssociatedTypeBinding {
                        name: binding,
                        compile_groups,
                        ty: self.type_expr()?,
                    });
                } else {
                    if saw_associated {
                        return Err(self.error_here(
                            "positional trait arguments must precede associated type equalities",
                        ));
                    }
                    let label = if matches!(self.current().kind, TokenKind::Ident(_))
                        && self.at_offset(1, &TokenKind::Colon)
                        && !self.at_offset(2, &TokenKind::Type)
                        && !self.at_offset(2, &TokenKind::Region)
                        && !matches!(
                            self.tokens.get(self.index + 2).map(|token| &token.kind),
                            Some(TokenKind::Ident(kind))
                                if matches!(kind.as_str(), "access" | "effect")
                        ) {
                        labeled += 1;
                        let label = self.expect_ident("a trait argument label")?;
                        self.expect(&TokenKind::Colon, "`:` after trait argument label")?;
                        Some(label)
                    } else {
                        None
                    };
                    arguments.push(TypeArg {
                        label,
                        ty: self.type_expr()?,
                    });
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
            if labeled != 0 && labeled != arguments.len() {
                return Err(
                    self.error_here("trait arguments must be either all labeled or all positional")
                );
            }
        }
        let trait_ref = if arguments.iter().any(|argument| argument.label.is_some()) {
            Type::NamedArgs(name, arguments)
        } else {
            Type::Named(
                name,
                arguments.into_iter().map(|argument| argument.ty).collect(),
            )
        };
        Ok((trait_ref, associated_types))
    }

    fn declaration_groups(
        &mut self,
        allow_receiver: bool,
        outer_effect_parameters: &[String],
    ) -> Result<DeclarationGroups, ParseError> {
        self.effect_parameters_in_scope.clear();
        self.effect_parameters_in_scope
            .extend(outer_effect_parameters.iter().cloned());
        let mut compile_groups: Vec<Vec<CompileParam>> = Vec::new();
        let mut runtime_groups = Vec::new();

        while self.group_starts_with_compile_parameter() {
            let params = self.compile_parameter_group()?;
            self.effect_parameters_in_scope.extend(
                params
                    .iter()
                    .filter(|parameter| parameter.kind.is_effect_classifier())
                    .map(|parameter| parameter.name.clone()),
            );
            compile_groups.push(params);
            self.take_newlines_if_followed_by(&[
                TokenKind::LParen,
                TokenKind::Colon,
                TokenKind::Equal,
            ]);
        }

        while self.at(&TokenKind::LParen) {
            runtime_groups.push(
                self.runtime_parameter_group(
                    allow_receiver,
                    &compile_groups
                        .iter()
                        .flatten()
                        .map(|parameter| parameter.name.clone())
                        .collect(),
                    true,
                )?,
            );
            self.take_newlines_if_followed_by(&[
                TokenKind::LParen,
                TokenKind::Ellipsis,
                TokenKind::Colon,
                TokenKind::Equal,
            ]);
        }
        if !runtime_groups.is_empty() {
            return Ok((
                compile_groups,
                runtime_groups,
                FunctionEffects::default(),
                false,
                false,
            ));
        }

        let callable_boundary = self.at(&TokenKind::Colon)
            && matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(name)) if name == "with"
            );
        if !callable_boundary {
            return Ok((
                compile_groups,
                runtime_groups,
                FunctionEffects::default(),
                false,
                false,
            ));
        }
        self.expect(
            &TokenKind::Colon,
            "callable-type/body boundary `:` before runtime callable type",
        )?;
        self.take_newlines_if_followed_by(&[
            TokenKind::LParen,
            TokenKind::Ellipsis,
            TokenKind::Ident("with".to_owned()),
        ]);
        let (effects, _failure_error, has_effect_clause) = self.function_effect_clause()?;
        self.take_newlines_if_followed_by(&[TokenKind::LParen, TokenKind::Ellipsis]);
        let modifier_parameters = compile_groups
            .iter()
            .flatten()
            .map(|parameter| parameter.name.clone())
            .collect::<HashSet<_>>();
        while self.at(&TokenKind::LParen) {
            if self.group_starts_with_compile_parameter() {
                return Err(self.error_here(
                    "compile-time parameter groups must precede the callable-type/body boundary",
                ));
            }
            runtime_groups.push(self.runtime_parameter_group(
                allow_receiver,
                &modifier_parameters,
                true,
            )?);
            self.take_newlines_if_followed_by(&[
                TokenKind::LParen,
                TokenKind::Ellipsis,
                TokenKind::Colon,
                TokenKind::Equal,
            ]);
        }

        if self.take(&TokenKind::Ellipsis) {
            self.layout
                .repeated_parameter_groups
                .push(self.previous().start_byte);
            let schema = self.repeated_parameter_group_schema()?;
            let pack = match &schema {
                Type::Named(name, _) => name.clone(),
                _ => {
                    return Err(self.error_here(
                        "a repeated runtime parameter group requires a parameter schema",
                    ));
                }
            };
            if matches!(&schema, Type::Named(_, arguments) if arguments.is_empty()) {
                let declared = compile_groups.iter().flatten().any(|parameter| {
                    parameter.name == pack && parameter.kind == Sort::ParameterPack
                });
                if !declared {
                    return Err(self.error_here(format!(
                        "repeated runtime group `{pack}` requires a preceding `...{pack}: parameters` declaration"
                    )));
                }
            }
            runtime_groups.push(vec![Param {
                mode: PassMode::Inferred,
                access: None,
                modifiers: Vec::new(),
                region: None,
                name: pack.clone(),
                ty: Type::Named("$parameter$groups$expand".to_owned(), vec![schema]),
            }]);
            self.take_newlines_if_followed_by(&[
                TokenKind::LParen,
                TokenKind::Colon,
                TokenKind::Equal,
            ]);
            while self.at(&TokenKind::LParen) {
                if self.group_starts_with_compile_parameter() {
                    return Err(self.error_here(
                        "compile-time parameter groups must precede repeated runtime parameter groups",
                    ));
                }
                let passing_parameters = compile_groups
                    .iter()
                    .flatten()
                    .filter(|parameter| parameter.kind.is_parameter_modifier())
                    .map(|parameter| parameter.name.clone())
                    .collect::<HashSet<_>>();
                runtime_groups.push(self.runtime_parameter_group(
                    allow_receiver,
                    &passing_parameters,
                    true,
                )?);
                self.take_newlines_if_followed_by(&[
                    TokenKind::LParen,
                    TokenKind::Colon,
                    TokenKind::Equal,
                ]);
            }
        }

        Ok((
            compile_groups,
            runtime_groups,
            effects,
            true,
            has_effect_clause,
        ))
    }

    fn repeated_parameter_group_schema(&mut self) -> Result<Type, ParseError> {
        let mut path = vec![self.expect_path_start("a parameter schema")?];
        while self.take(&TokenKind::Dot) {
            let segment =
                self.expect_path_continuation(&path, "a parameter schema path segment after `.`")?;
            path.push(segment);
        }
        let name = path.join(".");
        let mut arguments = Vec::new();
        if self.take(&TokenKind::LParen) && !self.take(&TokenKind::RParen) {
            loop {
                arguments.push(self.type_expr()?);
                if self.take(&TokenKind::Comma) {
                    if self.take(&TokenKind::RParen) {
                        break;
                    }
                } else {
                    self.expect(&TokenKind::RParen, "`)` after parameter schema arguments")?;
                    break;
                }
            }
        }
        Ok(Type::Named(name, arguments))
    }

    fn group_starts_with_compile_parameter(&self) -> bool {
        self.at(&TokenKind::LParen)
            && self.at_offset(1, &TokenKind::Comptime)
            && if self.at_offset(2, &TokenKind::Ellipsis) {
                matches!(
                    self.tokens.get(self.index + 3).map(|token| &token.kind),
                    Some(TokenKind::Ident(_))
                ) && self.at_offset(4, &TokenKind::Colon)
                    && matches!(
                        self.tokens.get(self.index + 5).map(|token| &token.kind),
                        Some(TokenKind::Ident(kind)) if kind == "parameters"
                    )
            } else {
                matches!(
                    self.tokens.get(self.index + 2).map(|token| &token.kind),
                    Some(TokenKind::Ident(_)) | Some(TokenKind::RegionName(_))
                ) && self.at_offset(3, &TokenKind::Colon)
                    && self.compile_parameter_sort_starts_at(4)
            }
    }

    fn current_starts_compile_parameter(&self) -> bool {
        self.at(&TokenKind::Comptime)
            && matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(_)) | Some(TokenKind::RegionName(_))
            )
            && self.at_offset(2, &TokenKind::Colon)
            && self.compile_parameter_sort_starts_at(3)
    }

    fn compile_parameter_sort_starts_at(&self, offset: usize) -> bool {
        self.at_offset(offset, &TokenKind::Type)
            || self.at_offset(offset, &TokenKind::Region)
            || self.constructor_compile_parameter_sort_starts_at(offset)
            || matches!(
                self.tokens
                    .get(self.index + offset)
                    .map(|token| &token.kind),
                Some(TokenKind::Ident(_))
            )
    }

    fn constructor_compile_parameter_sort_starts_at(&self, offset: usize) -> bool {
        let mut index = self.index + offset;
        let mut groups = 0;
        while matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::LParen)
        ) {
            groups += 1;
            index += 1;
            loop {
                if !self.kind_at(index, &TokenKind::Comptime) {
                    return false;
                }
                index += 1;
                if !matches!(
                    self.tokens.get(index).map(|token| &token.kind),
                    Some(TokenKind::Ident(_))
                ) {
                    return false;
                }
                index += 1;
                if !matches!(
                    self.tokens.get(index).map(|token| &token.kind),
                    Some(TokenKind::Colon)
                ) {
                    return false;
                }
                index += 1;
                if !self.kind_at(index, &TokenKind::Type)
                    && !self.kind_at(index, &TokenKind::Region)
                    && !matches!(
                        self.tokens.get(index).map(|token| &token.kind),
                        Some(TokenKind::Ident(_))
                    )
                {
                    return false;
                }
                index += 1;
                if matches!(
                    self.tokens.get(index).map(|token| &token.kind),
                    Some(TokenKind::Comma)
                ) {
                    index += 1;
                    if matches!(
                        self.tokens.get(index).map(|token| &token.kind),
                        Some(TokenKind::RParen)
                    ) {
                        break;
                    }
                    continue;
                }
                break;
            }
            if !matches!(
                self.tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::RParen)
            ) {
                return false;
            }
            index += 1;
        }
        if groups == 0 {
            return false;
        }
        if !matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Colon)
        ) {
            return false;
        }
        index += 1;
        self.kind_at(index, &TokenKind::Type)
            || matches!(
                self.tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::Ident(name)) if matches!(name.as_str(), "effect" | "parameters")
            )
    }

    fn compile_parameter_sort(
        &mut self,
        name_token: &Token,
        name: &str,
        region_name: bool,
    ) -> Result<Sort, ParseError> {
        if region_name {
            return Err(self.error_at(
                name_token,
                "region literals cannot be compile-time parameter names; write `comptime r: region` for a region parameter",
            ));
        }

        if self.take(&TokenKind::Type) {
            if matches!(
                name,
                "_" | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "bool"
                    | "never"
            ) {
                return Err(self.error_at(
                    name_token,
                    format!(
                        "reserved type name `{name}` cannot be used as a compile-time parameter"
                    ),
                ));
            }
            return Ok(Sort::Type);
        }

        if let TokenKind::Ident(kind) = self.current().kind.clone() {
            if kind == "sort" {
                self.advance();
                return self.compile_sort_level().map(Sort::Universe);
            }
            if kind == "region" {
                if name == "static" {
                    return Err(self.error_at(
                        name_token,
                        "region entity `'static` is predefined and cannot be redeclared",
                    ));
                }
                self.advance();
                return Ok(Sort::Region);
            }
            let parameter_kind = match kind.as_str() {
                "usize" => Some(Sort::USize),
                "access" => Some(Sort::Named("access".to_owned())),
                "effect" => Some(Sort::Effect),
                "effects" => Some(Sort::Effects),
                "parameters" => Some(Sort::Parameters),
                _ => Some(Sort::Named(kind)),
            };
            if let Some(parameter_kind) = parameter_kind {
                self.advance();
                return Ok(parameter_kind);
            }
        }

        if self.at(&TokenKind::LParen) {
            if matches!(
                name,
                "_" | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "bool"
                    | "never"
            ) {
                return Err(self.error_at(
                    name_token,
                    format!(
                        "reserved type name `{name}` cannot be used as a compile-time parameter"
                    ),
                ));
            }
            return self.constructor_compile_parameter_sort();
        }

        self.expect(
            &TokenKind::Region,
            "`type`, `usize`, `access`, `effect`, `effects`, `parameters`, a constructor sort, or `region`",
        )?;
        if name == "static" {
            return Err(self.error_at(
                name_token,
                "region entity `'static` is predefined and cannot be redeclared",
            ));
        }
        Ok(Sort::Region)
    }

    fn constructor_compile_parameter_sort(&mut self) -> Result<Sort, ParseError> {
        let mut parameter_groups = Vec::new();
        while self.at(&TokenKind::LParen) {
            parameter_groups.push(self.constructor_sort_parameter_group()?);
        }
        self.expect(&TokenKind::Colon, "`:` before constructor result sort")?;
        if self.take(&TokenKind::Type) {
            return Ok(Sort::TypeConstructor { parameter_groups });
        }
        if matches!(&self.current().kind, TokenKind::Ident(name) if name == "effect") {
            self.advance();
            return Ok(Sort::EffectConstructor { parameter_groups });
        }
        if matches!(&self.current().kind, TokenKind::Ident(name) if name == "parameters") {
            self.advance();
            if parameter_groups == [vec![Sort::Parameters]] {
                return Ok(Sort::ParameterModifier);
            }
            return Err(self.error_here(
                "parameter modifier sorts must have the exact shape `(P: parameters): parameters`",
            ));
        }
        Err(self.error_here("expected constructor result sort `type`, `effect`, or `parameters`"))
    }

    fn constructor_sort_parameter_group(&mut self) -> Result<Vec<Sort>, ParseError> {
        self.expect(&TokenKind::LParen, "`(` in constructor sort")?;
        if self.take(&TokenKind::RParen) {
            return Err(self.error_here("constructor sort parameter groups cannot be empty"));
        }

        let mut parameter_kinds = Vec::new();
        loop {
            self.expect(
                &TokenKind::Comptime,
                "`comptime` before constructor sort parameter",
            )?;
            let name_token = self.current().clone();
            let name = self.expect_ident("a constructor sort parameter name")?;
            if matches!(
                name.as_str(),
                "_" | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "bool"
                    | "never"
            ) {
                return Err(self.error_at(
                    &name_token,
                    format!(
                        "reserved type name `{name}` cannot be used as a constructor sort parameter"
                    ),
                ));
            }
            self.expect(
                &TokenKind::Colon,
                "`:` after constructor sort parameter name",
            )?;
            parameter_kinds.push(self.compile_parameter_sort(&name_token, &name, false)?);

            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RParen) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RParen, "`)` after constructor sort parameters")?;
                break;
            }
        }

        Ok(parameter_kinds)
    }

    fn compile_parameter_group(&mut self) -> Result<Vec<CompileParam>, ParseError> {
        self.layout.parameter_groups.push(self.current().start_byte);
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();

        loop {
            self.expect(
                &TokenKind::Comptime,
                "`comptime` before compile-time parameter",
            )?;
            let variadic = self.take(&TokenKind::Ellipsis);
            if variadic {
                let name = self.expect_ident("a parameter-pack name")?;
                self.expect(&TokenKind::Colon, "`:` after parameter-pack name")?;
                if !matches!(&self.current().kind, TokenKind::Ident(kind) if kind == "parameters") {
                    return Err(self.error_here(
                        "`...` compile-time packs currently require sort `parameters`",
                    ));
                }
                self.advance();
                params.push(CompileParam {
                    name,
                    kind: Sort::ParameterPack,
                    default: None,
                });
                self.take(&TokenKind::Comma);
                self.expect(&TokenKind::RParen, "`)` after parameter pack")?;
                break;
            }
            if !matches!(
                self.current().kind,
                TokenKind::Ident(_) | TokenKind::RegionName(_)
            ) || !self.at_offset(1, &TokenKind::Colon)
                || !self.compile_parameter_sort_starts_at(2)
            {
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
            let kind = self.compile_parameter_sort(&name_token, &name, region_name)?;
            let default = self.compile_parameter_default(kind.clone())?;
            params.push(CompileParam {
                name,
                kind,
                default,
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

    fn compile_parameter_default(
        &mut self,
        kind: Sort,
    ) -> Result<Option<CompileParamDefault>, ParseError> {
        if !self.take(&TokenKind::Equal) {
            return Ok(None);
        }

        let default = match kind {
            Sort::Universe(_) => {
                return Err(self.error_here("defaults for universe parameters are not supported"));
            }
            Sort::Effect | Sort::Effects => {
                CompileParamDefault::Name(self.compile_parameter_default_name("an effect default")?)
            }
            Sort::Parameters => {
                return Err(self
                    .error_here("defaults for parameter-schema parameters are not supported yet"));
            }
            Sort::ParameterPack => {
                return Err(self.error_here("parameter packs cannot have defaults"));
            }
            Sort::ParameterModifier => {
                return Err(self.error_here(
                    "defaults for parameter modifier functions are not supported yet",
                ));
            }
            Sort::Region => {
                let token = self.current().clone();
                let TokenKind::RegionName(name) = token.kind else {
                    return Err(self.error_at(
                        &token,
                        format!("expected a region default, found {}", describe(&token.kind)),
                    ));
                };
                self.advance();
                CompileParamDefault::Region(name)
            }
            Sort::Type
            | Sort::USize
            | Sort::TypeConstructor { .. }
            | Sort::EffectConstructor { .. } => {
                return Err(self.error_here(
                    "defaults for type and constructor parameters are not supported yet",
                ));
            }
            Sort::Named(_) => CompileParamDefault::Name(
                self.compile_parameter_default_name("a compile-time value default")?,
            ),
        };
        Ok(Some(default))
    }

    fn compile_parameter_default_name(&mut self, expected: &str) -> Result<String, ParseError> {
        let token = self.current().clone();
        let name = match token.kind {
            TokenKind::Ident(name) => name,
            TokenKind::Mut => "mut".to_owned(),
            TokenKind::Copy => "copy".to_owned(),
            TokenKind::Move => "move".to_owned(),
            TokenKind::True => "true".to_owned(),
            TokenKind::False => "false".to_owned(),
            _ => {
                return Err(self.error_at(
                    &token,
                    format!("expected {expected}, found {}", describe(&token.kind)),
                ))
            }
        };
        self.advance();
        Ok(name)
    }

    fn runtime_parameter_group(
        &mut self,
        allow_receiver: bool,
        modifier_parameters: &HashSet<String>,
        record_layout: bool,
    ) -> Result<Vec<Param>, ParseError> {
        if record_layout {
            self.layout.parameter_groups.push(self.current().start_byte);
        }
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut params = Vec::new();
        if self.take(&TokenKind::RParen) {
            return Ok(params);
        }

        if self.take(&TokenKind::Ellipsis) {
            let mode = if self.take(&TokenKind::Move) {
                PassMode::Move
            } else if self.take(&TokenKind::Copy) {
                PassMode::Copy
            } else {
                PassMode::Inferred
            };
            let name = self.expect_ident("a parameter-pack binding name")?;
            self.expect(&TokenKind::Colon, "`:` after parameter-pack binding name")?;
            let schema = self.type_expr()?;
            self.expect(
                &TokenKind::RParen,
                "`)` after parameter-pack expansion; an expansion must occupy its complete parameter group",
            )?;
            return Ok(vec![Param {
                mode,
                access: None,
                modifiers: Vec::new(),
                region: None,
                name,
                ty: Type::Named("$parameters$expand".to_owned(), vec![schema]),
            }]);
        }

        loop {
            let mut modifiers = Vec::new();
            loop {
                let followed_by_parameter = matches!(
                    self.tokens.get(self.index + 1).map(|token| &token.kind),
                    Some(TokenKind::Ident(_)) | Some(TokenKind::Copy) | Some(TokenKind::Move)
                );
                if !followed_by_parameter {
                    break;
                }
                let modifier = match self.current().kind.clone() {
                    TokenKind::Ident(name)
                        if modifier_parameters.contains(&name)
                            || matches!(name.as_str(), "copy" | "move") =>
                    {
                        name
                    }
                    TokenKind::Copy => "copy".to_owned(),
                    TokenKind::Move => "move".to_owned(),
                    _ => break,
                };
                self.advance();
                modifiers.push(modifier);
            }
            let mut mode = PassMode::Inferred;
            modifiers.retain(|modifier| match modifier.as_str() {
                "copy" => {
                    mode = PassMode::Copy;
                    false
                }
                "move" => {
                    mode = PassMode::Move;
                    false
                }
                _ => true,
            });
            let (mode, access, region) = if self.at(&TokenKind::Borrow) {
                return Err(self.error_here(
                    "borrow parameter mode was removed; write `name: borrow(T)` and pass `borrow(value)` at the call site",
                ));
            } else {
                (mode, None, None)
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
                if self.take(&TokenKind::Colon) {
                    self.type_expr()?
                } else {
                    Type::Named("self".into(), Vec::new())
                }
            } else {
                self.expect(&TokenKind::Colon, "`:` after parameter name")?;
                self.type_expr()?
            };
            params.push(Param {
                mode,
                access,
                modifiers,
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
                Some(TokenKind::Ident(_) | TokenKind::RegionName(_))
            )
        {
            return Ok(None);
        }
        self.expect(&TokenKind::LParen, "`(` before region")?;
        let token = self.current().clone();
        let name = match token.kind {
            TokenKind::Ident(name) | TokenKind::RegionName(name) => name,
            _ => {
                unreachable!("optional region lookahead was checked");
            }
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
            let name = match token.kind {
                TokenKind::Ident(name) | TokenKind::RegionName(name) => name,
                _ => {
                    return Err(self.error_at(&token, "expected a region after access argument"));
                }
            };
            self.advance();
            Some(name)
        } else {
            None
        };
        self.expect(&TokenKind::RParen, "`)` after borrow arguments")?;
        Ok((mutable, access, region))
    }

    fn validate_receiver_groups(
        &self,
        name: &str,
        groups: &[Vec<Param>],
    ) -> Result<(), ParseError> {
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
        if groups.len() < 2 && !matches!(name, "unwrap" | "raise") {
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
        let (representation, derives) = self.struct_options()?;
        self.expect(&TokenKind::LBrace, "`{` after `struct`")?;
        let fields = self.braced_type_fields()?;
        Ok(StructDef {
            name,
            compile_groups,
            representation,
            derives,
            fields,
        })
    }

    fn struct_options(&mut self) -> Result<(StructRepresentation, Vec<String>), ParseError> {
        if !self.take(&TokenKind::LParen) {
            return Ok((StructRepresentation::Salicin, Vec::new()));
        }
        let mut representation = StructRepresentation::Salicin;
        let mut derives = Vec::new();
        if self.take(&TokenKind::RParen) {
            return Ok((representation, derives));
        }
        loop {
            let option = self.expect_ident("a struct option name")?;
            if self.take(&TokenKind::Colon) {
                if option == "derive" {
                    let derive = self.expect_ident("a derive name")?;
                    if derive != "copyable" {
                        return Err(self.error_here(format!(
                            "unsupported struct derive `{derive}`; only `copyable` is supported"
                        )));
                    }
                    if derives.iter().any(|existing| existing == &derive) {
                        return Err(self.error_here(format!("duplicate struct derive `{derive}`")));
                    }
                    derives.push(derive);
                } else {
                    return Err(self.error_here(format!(
                        "unknown struct option `{option}`; expected `derive`"
                    )));
                }
            } else if option == "c" {
                if representation == StructRepresentation::C {
                    return Err(self.error_here("duplicate struct representation `c`"));
                }
                representation = StructRepresentation::C;
            } else {
                return Err(self.error_here(format!(
                    "unsupported struct representation `{option}`; only `c` is available"
                )));
            }
            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RParen) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RParen, "`)` after struct options")?;
                break;
            }
        }
        Ok((representation, derives))
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
        let mut variant_names = HashSet::new();

        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` before end of enum declaration"));
            }

            let variant_name = self.enum_variant_name()?;
            if !variant_names.insert(variant_name.clone()) {
                return Err(self.error_here(format!("duplicate enum variant `{variant_name}`")));
            }
            let fields = if self.take(&TokenKind::LParen) {
                self.skip_separators();
                if self.take(&TokenKind::RParen) {
                    VariantFields::Positional(Vec::new())
                } else if self.ident_followed_by_colon() || self.at(&TokenKind::Pub) {
                    VariantFields::Named(self.named_type_fields_after_open()?)
                } else {
                    let mut types = Vec::new();
                    loop {
                        types.push(self.type_expr()?);
                        if self.take(&TokenKind::Comma) {
                            self.skip_separators();
                            if self.take(&TokenKind::RParen) {
                                break;
                            }
                        } else {
                            self.skip_separators();
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

    fn enum_variant_name(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        let name = match token.kind {
            TokenKind::Ident(name) => name,
            TokenKind::True => "true".to_owned(),
            TokenKind::False => "false".to_owned(),
            TokenKind::Mut => "mut".to_owned(),
            _ => {
                return Err(self.error_at(
                    &token,
                    format!(
                        "expected an enum variant name, found {}",
                        describe(&token.kind)
                    ),
                ))
            }
        };
        self.advance();
        Ok(name)
    }

    fn trait_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
    ) -> Result<TraitDef, ParseError> {
        self.expect(&TokenKind::Trait, "`trait`")?;
        let self_parameter =
            if self.at(&TokenKind::LParen) && self.at_offset(1, &TokenKind::Comptime) {
                let group = self.compile_parameter_group()?;
                let [parameter] = group.as_slice() else {
                    return Err(self
                        .error_here("trait self sort must declare exactly one `self` parameter"));
                };
                if parameter.name != "self" {
                    return Err(self.error_here("trait self sort parameter must be named `self`"));
                }
                parameter.clone()
            } else {
                default_trait_self_parameter()
            };
        let where_predicates = if self.at(&TokenKind::LParen) {
            self.requires_parameter_group()?
        } else {
            Vec::new()
        };
        self.take_newlines_if_followed_by(&[TokenKind::LBrace]);
        self.expect(&TokenKind::LBrace, "`{` after `trait`")?;
        self.skip_separators();

        let mut member_effect_parameters = compile_groups
            .iter()
            .flatten()
            .filter(|parameter| parameter.kind.is_effect_classifier())
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        if self_parameter.kind.is_effect_classifier() {
            member_effect_parameters.push(self_parameter.name.clone());
        }

        let mut members = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` before end of trait declaration"));
            }
            members.push(self.trait_member(&member_effect_parameters)?);
            if !self.at(&TokenKind::RBrace) && !self.at_separator() {
                return Err(self.error_here("expected a newline or `;` after trait member"));
            }
            self.skip_separators();
        }
        self.expect(&TokenKind::RBrace, "`}` after trait members")?;

        Ok(TraitDef {
            name,
            self_parameter,
            compile_groups,
            where_predicates,
            members,
        })
    }

    fn trait_member(
        &mut self,
        outer_effect_parameters: &[String],
    ) -> Result<TraitMember, ParseError> {
        if self.at(&TokenKind::Pub) {
            return Err(self.error_here("visibility on trait members is not supported yet"));
        }
        self.expect(&TokenKind::Let, "`let` in trait body")?;
        if self.take(&TokenKind::Mut) {
            let mutable = self.previous().clone();
            return Err(self.error_at(&mutable, "trait members cannot be declared with `let mut`"));
        }
        let name = self.expect_ident("a trait member name")?;
        let (compile_groups, groups, mut effects, has_callable_boundary, _has_effect_clause) =
            self.declaration_groups(true, outer_effect_parameters)?;
        self.validate_receiver_groups(&name, &groups)?;

        let logical_result = if self.take(&TokenKind::Colon) {
            let associated_kind = if self.take(&TokenKind::Type) {
                Some(AssociatedKind::Type)
            } else if self.at_context_ident("parameters") {
                self.advance();
                Some(AssociatedKind::Parameters)
            } else {
                None
            };
            if let Some(kind) = associated_kind {
                if !groups.is_empty() {
                    return Err(self.error_here(
                        "associated declarations cannot have runtime parameter groups",
                    ));
                }
                if kind == AssociatedKind::Parameters && self.at(&TokenKind::Equal) {
                    return Err(self
                        .error_here("default associated parameter schemas are not supported yet"));
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
                    kind,
                    default,
                });
            }
            Some(self.function_result_type()?)
        } else {
            None
        };
        if !has_callable_boundary {
            effects = self.function_effect_clause()?.0;
        }
        let failure_error = effects.failure.as_deref().cloned();
        let return_type =
            logical_result.map(|result| Self::apply_failure_effect(result, failure_error));
        self.effect_parameters_in_scope.clear();

        if compile_groups.is_empty() && groups.is_empty() {
            return Err(
                self.error_here("trait function members require at least one parameter group")
            );
        }

        self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::Equal]);
        if self.at(&TokenKind::Where) {
            return Err(self.error_here(
                "colon-style trait-member predicates were removed; write `= requires(t is trait)`",
            ));
        }
        let mut where_predicates = Vec::new();
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);
        let body = if self.take(&TokenKind::Equal) {
            if self.at_context_ident("requires") {
                self.advance();
                where_predicates.extend(self.constraint_arguments("`(` after `requires`")?);
                if self.at_separator() || self.at(&TokenKind::RBrace) {
                    None
                } else {
                    if self.at_context_ident("builtin") {
                        return Err(self.error_here(
                            "trait requirements are abstract and cannot use `builtin()`",
                        ));
                    }
                    if !self.at(&TokenKind::LBrace) {
                        return Err(self.error_here(
                            "trait default closure declarations require a braced body after `requires(...)`",
                        ));
                    }
                    Some(self.block()?)
                }
            } else {
                if self.at_context_ident("builtin") {
                    return Err(self
                        .error_here("trait requirements are abstract and cannot use `builtin()`"));
                }
                if !self.at(&TokenKind::LBrace) {
                    return Err(self.error_here(
                    "trait default closure declarations require a braced body; write `= { expression }`",
                ));
                }
                Some(self.block()?)
            }
        } else {
            None
        };

        Ok(TraitMember::Function(Function {
            name,
            foreign: None,
            builtin: false,
            compile_groups,
            groups,
            return_type,
            effects,
            where_predicates,
            body,
        }))
    }

    fn function_effect_clause(
        &mut self,
    ) -> Result<(FunctionEffects, Option<Type>, bool), ParseError> {
        if !self.at_context_ident("with") {
            return Ok((FunctionEffects::default(), None, false));
        }

        self.advance();
        self.expect(&TokenKind::LParen, "`(` after `with`")?;
        if self.take(&TokenKind::RParen) {
            return Ok((FunctionEffects::default(), None, true));
        }
        let unsafety = false;
        let failure_error: Option<Type> = None;
        let mut effect_parameters = Vec::new();
        let mut custom = Vec::new();
        loop {
            if let TokenKind::Ident(name) = &self.current().kind {
                let name = name.clone();
                if self.effect_parameters_in_scope.contains(&name)
                    && !self.at_offset(1, &TokenKind::Dot)
                {
                    self.advance();
                    if effect_parameters.contains(&name) {
                        return Err(self.error_here(format!(
                            "duplicate effect parameter `{name}` in `with(...)`"
                        )));
                    }
                    effect_parameters.push(name);
                } else {
                    let mut path = vec![self.expect_ident("an effect name")?];
                    while self.take(&TokenKind::Dot) {
                        path.push(self.expect_ident("an effect path segment")?);
                    }
                    let name = path.join(".");
                    let mut arguments = Vec::new();
                    if self.take(&TokenKind::LParen) && !self.take(&TokenKind::RParen) {
                        let mut labeled = 0;
                        loop {
                            let label = if matches!(self.current().kind, TokenKind::Ident(_))
                                && self.at_offset(1, &TokenKind::Colon)
                                && !self.at_offset(2, &TokenKind::Type)
                                && !self.at_offset(2, &TokenKind::Region)
                                && !matches!(
                                    self.tokens.get(self.index + 2).map(|token| &token.kind),
                                    Some(TokenKind::Ident(kind))
                                        if matches!(kind.as_str(), "access" | "effect" | "effects")
                                ) {
                                labeled += 1;
                                let label = self.expect_ident("an effect argument label")?;
                                self.expect(&TokenKind::Colon, "`:` after effect argument label")?;
                                Some(label)
                            } else {
                                None
                            };
                            let ty = self.type_expr()?;
                            arguments.push(TypeArg { label, ty });
                            if self.take(&TokenKind::Comma) {
                                if self.take(&TokenKind::RParen) {
                                    break;
                                }
                            } else {
                                self.expect(&TokenKind::RParen, "`)` after effect arguments")?;
                                break;
                            }
                        }
                        if labeled != 0 && labeled != arguments.len() {
                            return Err(self.error_here(
                                "effect arguments must be either all labeled or all positional",
                            ));
                        }
                    }
                    let effect = if arguments.iter().any(|argument| argument.label.is_some()) {
                        Type::NamedArgs(name.clone(), arguments)
                    } else {
                        Type::Named(
                            name.clone(),
                            arguments.into_iter().map(|argument| argument.ty).collect(),
                        )
                    };
                    if custom.contains(&effect) {
                        return Err(self.error_here(format!(
                            "duplicate custom effect `{name}` in `with(...)`"
                        )));
                    }
                    custom.push(effect);
                }
            } else {
                return Err(self.error_here(
                    "expected `throwing(Error)`, `Unsafe`, an effect parameter, or a custom effect name in `with(...)`",
                ));
            }

            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RParen) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RParen, "`)` after function effects")?;
                break;
            }
        }

        effect_parameters.sort();
        Ok((
            FunctionEffects {
                unsafety,
                failure: failure_error.clone().map(Box::new),
                custom,
                parameters: effect_parameters,
            },
            failure_error,
            true,
        ))
    }

    fn apply_failure_effect(output: Type, failure_error: Option<Type>) -> Type {
        match failure_error {
            None => output,
            Some(error) => Type::Named("result".to_owned(), vec![error, output]),
        }
    }

    fn function_result_type(&mut self) -> Result<Type, ParseError> {
        if self.at_context_ident("sort")
            && self.at_offset(1, &TokenKind::LParen)
            && matches!(
                self.tokens.get(self.index + 2).map(|token| &token.kind),
                Some(TokenKind::Ident(_))
            )
            && self.at_offset(3, &TokenKind::Plus)
        {
            self.advance();
            self.advance();
            let level = self.expect_ident("a universe level parameter")?;
            self.expect(&TokenKind::Plus, "`+` in successor universe")?;
            let token = self.current().clone();
            if token.kind != TokenKind::Integer(1) {
                return Err(self.error_at(
                    &token,
                    "a universe constructor result must be `sort(level + 1)`",
                ));
            }
            self.advance();
            self.expect(&TokenKind::RParen, "`)` after successor universe")?;
            return Ok(Type::Named(
                "sort".to_owned(),
                vec![Type::Named(format!("{level}+1"), Vec::new())],
            ));
        }
        self.type_expr()
    }

    fn braced_type_fields(&mut self) -> Result<Vec<Field>, ParseError> {
        self.skip_separators();
        if self.take(&TokenKind::RBrace) {
            return Ok(Vec::new());
        }
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
                self.skip_separators();
                if self.take(&TokenKind::RBrace) {
                    break;
                }
            } else {
                self.skip_separators();
                self.expect(&TokenKind::RBrace, "`}` after fields")?;
                break;
            }
        }
        Ok(fields)
    }

    fn named_type_fields_after_open(&mut self) -> Result<Vec<Field>, ParseError> {
        self.skip_separators();
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
                self.skip_separators();
                if self.take(&TokenKind::RParen) {
                    break;
                }
            } else {
                self.skip_separators();
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
        let value_start = self.current().clone();
        let value = self.expression(true)?;
        let value_end = self.previous().clone();

        Ok(Binding {
            mutable,
            name,
            annotation,
            value,
            value_source: Some(Box::new(crate::ast::SourceSpan {
                line: value_start.line,
                column: value_start.column,
                end_line: value_end.end_line,
                end_column: value_end.end_column,
            })),
        })
    }

    fn parameter_group(&mut self) -> Result<Vec<Param>, ParseError> {
        if self.untyped_closure_parameter_group_follows() {
            self.expect(&TokenKind::LParen, "`(`")?;
            let mut parameters = Vec::new();
            loop {
                let name = self.expect_ident("a contextual closure parameter name")?;
                parameters.push(Param {
                    mode: PassMode::Inferred,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name,
                    ty: Type::Named("$context$infer".into(), Vec::new()),
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
            return Ok(parameters);
        }
        self.runtime_parameter_group(false, &HashSet::new(), false)
    }

    fn untyped_closure_parameter_group_follows(&self) -> bool {
        self.at(&TokenKind::LParen)
            && matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(_))
            )
            && matches!(
                self.tokens.get(self.index + 2).map(|token| &token.kind),
                Some(TokenKind::Comma | TokenKind::RParen)
            )
    }

    fn type_expr(&mut self) -> Result<Type, ParseError> {
        if self.at_context_ident("with") {
            let (outer, _failure_error, _has_effect_clause) = self.function_effect_clause()?;
            self.expect(
                &TokenKind::LParen,
                "`(` before the callable operand of `with(...)`",
            )?;
            let operand = self.type_expr()?;
            self.expect(
                &TokenKind::RParen,
                "`)` after the callable operand of `with(...)`",
            )?;
            let Type::Function {
                groups,
                effects: inner,
                result,
            } = operand
            else {
                return Err(self.error_here(
                    "`with(E)(F)` accepts only a callable type `F`; it cannot wrap an ordinary result type",
                ));
            };
            let effects = self.merge_function_effects(outer, inner)?;
            return Ok(Type::Function {
                groups,
                effects,
                result,
            });
        }

        if self.take(&TokenKind::LParen) {
            return self.function_type_or_unit();
        }

        if self.at(&TokenKind::Borrow) {
            return self.borrow_type();
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
        if name.split('.').next_back() == Some("array") && self.take(&TokenKind::LParen) {
            if matches!(&self.current().kind, TokenKind::Ident(label) if label == "t")
                && self.at_offset(1, &TokenKind::Colon)
            {
                self.advance();
                self.advance();
            }
            let element = self.type_expr()?;
            self.take(&TokenKind::Comma);
            self.expect(
                &TokenKind::RParen,
                "`)` after the array element type; write the length in a second group",
            )?;
            self.expect(
                &TokenKind::LParen,
                "`(` before the array length; write `array(t)(l)`",
            )?;
            if matches!(&self.current().kind, TokenKind::Ident(label) if label == "l")
                && self.at_offset(1, &TokenKind::Colon)
            {
                self.advance();
                self.advance();
            }
            let length_token = self.current().clone();
            if matches!(&length_token.kind, TokenKind::Ident(name) if name == "_") {
                return Err(self.error_at(
                    &length_token,
                    "`_` compile-time argument inference has been removed; provide an explicit array length",
                ));
            }
            let length_expression = self.expression(false)?;
            let static_expression =
                Self::static_expression(&length_expression).map_err(|message| {
                    self.error_at(
                        &length_token,
                        format!("invalid compile-time array length: {message}"),
                    )
                })?;
            let length = match static_expression {
                StaticExpr::USize(length) => USizeConst::Literal(length),
                StaticExpr::Name(name) => USizeConst::Parameter(name),
                expression => USizeConst::Expression(Box::new(expression)),
            };
            self.take(&TokenKind::Comma);
            self.expect(&TokenKind::RParen, "`)` after array length")?;
            return Ok(Type::ArrayApplication {
                constructor: name,
                element: Box::new(element),
                length,
            });
        }

        let mut arguments = Vec::new();
        let mut labeled_total = 0;
        while self.take(&TokenKind::LParen) {
            if self.take(&TokenKind::RParen) {
                break;
            }
            let group_start = arguments.len();
            let mut labeled = 0;
            loop {
                let label = if matches!(self.current().kind, TokenKind::Ident(_))
                    && self.at_offset(1, &TokenKind::Colon)
                    && !self.at_offset(2, &TokenKind::Type)
                    && !self.at_offset(2, &TokenKind::Region)
                    && !matches!(
                        self.tokens.get(self.index + 2).map(|token| &token.kind),
                        Some(TokenKind::Ident(kind))
                            if matches!(kind.as_str(), "access" | "effect")
                    ) {
                    labeled += 1;
                    let label = self.expect_ident("a type argument label")?;
                    self.expect(&TokenKind::Colon, "`:` after type argument label")?;
                    Some(label)
                } else {
                    None
                };
                let ty = self.type_expr()?;
                arguments.push(TypeArg { label, ty });
                if self.take(&TokenKind::Comma) {
                    if self.take(&TokenKind::RParen) {
                        break;
                    }
                } else {
                    self.expect(&TokenKind::RParen, "`)` after type arguments")?;
                    break;
                }
            }
            labeled_total += labeled;
            if labeled != 0 && labeled != arguments.len() - group_start {
                return Err(
                    self.error_here("type arguments must be either all labeled or all positional")
                );
            }
        }
        if labeled_total != 0 && labeled_total != arguments.len() {
            return Err(
                self.error_here("type arguments must be either all labeled or all positional")
            );
        }

        if arguments.is_empty() {
            Ok(match name.as_str() {
                "i8" => Type::I8,
                "i16" => Type::I16,
                "i32" => Type::I32,
                "i64" => Type::I64,
                "i128" => Type::I128,
                "isize" => Type::ISize,
                "u8" => Type::U8,
                "u16" => Type::U16,
                "u32" => Type::U32,
                "u64" => Type::U64,
                "u128" => Type::U128,
                "usize" => Type::USize,
                "bool" => Type::Bool,
                _ => Type::Named(name, Vec::new()),
            })
        } else if arguments.iter().any(|argument| argument.label.is_some()) {
            Ok(Type::NamedArgs(name, arguments))
        } else {
            Ok(Type::Named(
                name,
                arguments.into_iter().map(|argument| argument.ty).collect(),
            ))
        }
    }

    fn static_expression(expression: &Expr) -> Result<StaticExpr, &'static str> {
        match expression.unlocated() {
            Expr::Integer(value) => u64::try_from(*value)
                .map(StaticExpr::USize)
                .map_err(|_| "integer values must fit in `usize`"),
            Expr::Bool(value) => Ok(StaticExpr::Bool(*value)),
            Expr::Name(name) => Ok(StaticExpr::Name(name.clone())),
            Expr::Unary(operator, operand) if !matches!(operator, UnaryOp::Deref) => Ok(
                StaticExpr::Unary(*operator, Box::new(Self::static_expression(operand)?)),
            ),
            Expr::Binary(left, operator, right) => Ok(StaticExpr::Binary(
                Box::new(Self::static_expression(left)?),
                *operator,
                Box::new(Self::static_expression(right)?),
            )),
            Expr::Call(_, _) => {
                fn flatten<'a>(expression: &'a Expr, groups: &mut Vec<&'a [CallArg]>) -> &'a Expr {
                    match expression.unlocated() {
                        Expr::Call(callee, arguments) => {
                            let root = flatten(callee, groups);
                            groups.push(arguments);
                            root
                        }
                        expression => expression,
                    }
                }
                let mut groups = Vec::new();
                let root = flatten(expression, &mut groups);
                let Expr::Name(function) = root.unlocated() else {
                    return Err("static calls must name a top-level pure function");
                };
                Ok(StaticExpr::Call {
                    function: function.clone(),
                    groups: groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|argument| {
                                    Ok(StaticCallArg {
                                        label: argument.label.clone(),
                                        value: Self::static_expression(&argument.value)?,
                                    })
                                })
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
            _ => Err(
                "expected a pure expression using literals, names, operators, and top-level calls",
            ),
        }
    }

    fn borrow_type(&mut self) -> Result<Type, ParseError> {
        self.expect(&TokenKind::Borrow, "`borrow`")?;
        if !self.at(&TokenKind::LParen) {
            return Err(self.error_here(
                "borrow types are written as `borrow(T)`; borrow values are written as `borrow(value)`",
            ));
        }

        let (mutable, access, region, pointee) = if self.borrow_qualifier_group_follows() {
            let (mutable, access, mut region) = self.optional_borrow_arguments()?;
            if region.is_none() && self.borrow_type_region_group_follows() {
                region = self.optional_region()?;
            }
            let pointee = self.borrow_type_pointee_group()?;
            (mutable, access, region, pointee)
        } else {
            (false, None, None, self.borrow_type_pointee_group()?)
        };

        Ok(Type::Borrow {
            mutable,
            access,
            region,
            pointee: Box::new(pointee),
        })
    }

    fn borrow_qualifier_group_follows(&self) -> bool {
        if !self.at(&TokenKind::LParen) {
            return false;
        }
        match self.tokens.get(self.index + 1).map(|token| &token.kind) {
            Some(TokenKind::Mut | TokenKind::RegionName(_)) => true,
            Some(TokenKind::Ident(_)) => {
                self.at_offset(2, &TokenKind::Comma)
                    || (self.at_offset(2, &TokenKind::RParen)
                        && self
                            .tokens
                            .get(self.index + 3)
                            .is_some_and(|token| Self::token_can_start_borrow_operand(&token.kind)))
            }
            _ => false,
        }
    }

    fn token_can_start_borrow_operand(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Ident(_)
                | TokenKind::Root
                | TokenKind::Super
                | TokenKind::Borrow
                | TokenKind::Star
                | TokenKind::LParen
        )
    }

    fn borrow_type_region_group_follows(&self) -> bool {
        self.at(&TokenKind::LParen)
            && matches!(
                self.tokens.get(self.index + 1).map(|token| &token.kind),
                Some(TokenKind::Ident(_) | TokenKind::RegionName(_))
            )
            && self.at_offset(2, &TokenKind::RParen)
            && self.at_offset(3, &TokenKind::LParen)
    }

    fn borrow_type_pointee_group(&mut self) -> Result<Type, ParseError> {
        self.expect(&TokenKind::LParen, "`(` before borrow pointee type")?;
        if self.at(&TokenKind::RParen) {
            return Err(self.error_here("borrow pointee type cannot be empty"));
        }
        let pointee = self.type_expr()?;
        self.expect(&TokenKind::RParen, "`)` after borrow pointee type")?;
        Ok(pointee)
    }

    fn function_type_or_unit(&mut self) -> Result<Type, ParseError> {
        let mut groups = Vec::new();
        let mut group = Vec::new();
        let mut first_group_had_comma = false;
        if !self.take(&TokenKind::RParen) {
            loop {
                if self.ident_followed_by_colon() {
                    self.expect_ident("a function type parameter name")?;
                    self.expect(&TokenKind::Colon, "`:` after function type parameter name")?;
                }
                group.push(self.type_expr()?);
                if self.take(&TokenKind::Comma) {
                    first_group_had_comma = true;
                    if self.take(&TokenKind::RParen) {
                        break;
                    }
                } else {
                    self.expect(
                        &TokenKind::RParen,
                        "`)` after function type parameter group",
                    )?;
                    break;
                }
            }
        }
        groups.push(group);

        while self.at(&TokenKind::LParen) {
            self.expect(
                &TokenKind::LParen,
                "`(` before function type parameter group",
            )?;
            let mut group = Vec::new();
            if !self.take(&TokenKind::RParen) {
                loop {
                    if self.ident_followed_by_colon() {
                        self.expect_ident("a function type parameter name")?;
                        self.expect(&TokenKind::Colon, "`:` after function type parameter name")?;
                    }
                    group.push(self.type_expr()?);
                    if self.take(&TokenKind::Comma) {
                        if self.take(&TokenKind::RParen) {
                            break;
                        }
                    } else {
                        self.expect(
                            &TokenKind::RParen,
                            "`)` after function type parameter group",
                        )?;
                        break;
                    }
                }
            }
            groups.push(group);
        }

        if !self.take(&TokenKind::Colon) {
            if groups.len() == 1 {
                let mut fields = groups.pop().expect("one parenthesized type group");
                if fields.is_empty() {
                    return Ok(Type::Unit);
                }
                if first_group_had_comma {
                    return Ok(Type::Tuple(fields));
                }
                return Ok(fields.pop().expect("one grouped type"));
            }
            return Err(self.error_here("function types require `:` before the result type"));
        }
        let logical_result = self.function_result_type()?;
        let (effects, failure_error, _has_legacy_effect_clause) = self.function_effect_clause()?;
        let result = Self::apply_failure_effect(logical_result, failure_error);
        Ok(Type::Function {
            groups,
            effects,
            result: Box::new(result),
        })
    }

    fn merge_function_effects(
        &self,
        mut outer: FunctionEffects,
        inner: FunctionEffects,
    ) -> Result<FunctionEffects, ParseError> {
        outer.unsafety |= inner.unsafety;
        match (&outer.failure, inner.failure) {
            (None, failure) => outer.failure = failure,
            (Some(left), Some(right)) if **left != *right => {
                return Err(self.error_here(
                    "nested `with(...)` constructors declare incompatible failure effects",
                ));
            }
            _ => {}
        }
        for effect in inner.custom {
            if !outer.custom.contains(&effect) {
                outer.custom.push(effect);
            }
        }
        outer.parameters.extend(inner.parameters);
        outer.parameters.sort();
        outer.parameters.dedup();
        Ok(outer)
    }

    fn expression(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        self.assignment(allow_trailing_closure)
    }

    fn assignment(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        let left = self.match_expression(allow_trailing_closure)?;
        let compound = if self.take(&TokenKind::PlusEqual) {
            Some(BinaryOp::Add)
        } else if self.take(&TokenKind::MinusEqual) {
            Some(BinaryOp::Sub)
        } else if self.take(&TokenKind::StarEqual) {
            Some(BinaryOp::Mul)
        } else if self.take(&TokenKind::SlashEqual) {
            Some(BinaryOp::Div)
        } else if self.take(&TokenKind::PercentEqual) {
            Some(BinaryOp::Rem)
        } else if self.take(&TokenKind::AmpEqual) {
            Some(BinaryOp::BitAnd)
        } else if self.take(&TokenKind::PipeEqual) {
            Some(BinaryOp::BitOr)
        } else if self.take(&TokenKind::CaretEqual) {
            Some(BinaryOp::BitXor)
        } else if self.take(&TokenKind::ShlEqual) {
            Some(BinaryOp::Shl)
        } else if self.take(&TokenKind::ShrEqual) {
            Some(BinaryOp::Shr)
        } else {
            None
        };
        if self.take(&TokenKind::Equal) || compound.is_some() {
            let equals = self.previous().clone();
            let right = self.assignment(allow_trailing_closure)?;
            if Self::is_assignable_place(&left) {
                Ok(match compound {
                    Some(operator) => {
                        Expr::CompoundAssign(Box::new(left), operator, Box::new(right))
                    }
                    None => Expr::Assign(Box::new(left), Box::new(right)),
                })
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
        let mut cases = Vec::new();
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
            cases.push(Expr::PatternClosure {
                pattern,
                guard: guard.map(Box::new),
                body: Box::new(body),
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

        let mut call = Expr::Call(
            Box::new(Self::core_match_function()),
            vec![CallArg {
                label: None,
                value: scrutinee,
            }],
        );
        for case in cases {
            call = Expr::Call(
                Box::new(call),
                vec![CallArg {
                    label: None,
                    value: case,
                }],
            );
        }
        Ok(call)
    }

    fn prefix_match_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Match, "`match`")?;
        let scrutinee = self.expression(false)?;
        self.take_newlines_if_followed_by(&[TokenKind::LBrace]);
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("`match` requires at least one trailing pattern case"));
        }

        let mut cases = Vec::new();
        while self.at(&TokenKind::LBrace) {
            let open = self.current().clone();
            self.expect(&TokenKind::LBrace, "`{` before a match case")?;
            self.skip_separators();
            let pattern = self.pattern()?;
            let guard = if self.take(&TokenKind::If) {
                Some(self.expression(false)?)
            } else {
                None
            };
            self.expect(&TokenKind::Arrow, "`->` after match case pattern")?;
            let body_start_byte = self.current().start_byte;
            let body = self.block_contents()?;
            let close = self.previous().clone();
            self.layout.match_arms.push(SourceBracedRegion {
                open_byte: open.start_byte,
                close_byte: close.start_byte,
                body_start_byte,
                open_line: open.line,
                close_line: close.line,
            });
            cases.push(Expr::PatternClosure {
                pattern,
                guard: guard.map(Box::new),
                body: Box::new(body),
            });
            self.take_newlines_if_followed_by(&[TokenKind::LBrace]);
        }

        let mut call = Expr::Call(
            Box::new(Self::core_match_function()),
            vec![CallArg {
                label: None,
                value: scrutinee,
            }],
        );
        for case in cases {
            call = Expr::Call(
                Box::new(call),
                vec![CallArg {
                    label: None,
                    value: case,
                }],
            );
        }
        Ok(call)
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
                Ok(Pattern::Integer(crate::ast::IntegerPattern {
                    magnitude: value,
                    negative: false,
                }))
            }
            TokenKind::Minus => {
                self.advance();
                let integer = self.current().clone();
                let TokenKind::Integer(value) = integer.kind else {
                    return Err(self.error_at(&integer, "expected integer literal after `-`"));
                };
                self.advance();
                Ok(Pattern::Integer(crate::ast::IntegerPattern {
                    magnitude: value,
                    negative: true,
                }))
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
            TokenKind::LParen => self.tuple_or_grouped_pattern(),
            TokenKind::Ident(_) | TokenKind::Root | TokenKind::Super => self.named_pattern(),
            _ => Err(self.error_at(
                &token,
                format!("expected a pattern, found {}", describe(&token.kind)),
            )),
        }
    }

    fn tuple_or_grouped_pattern(&mut self) -> Result<Pattern, ParseError> {
        self.expect(&TokenKind::LParen, "`(` before tuple pattern")?;
        if self.take(&TokenKind::RParen) {
            return Err(self.error_here("empty tuple patterns are written as `_`"));
        }
        let first = self.pattern()?;
        if !self.take(&TokenKind::Comma) {
            self.expect(&TokenKind::RParen, "`)` after parenthesized pattern")?;
            return Ok(first);
        }
        let mut fields = vec![first];
        while !self.take(&TokenKind::RParen) {
            fields.push(self.pattern()?);
            if !self.take(&TokenKind::Comma) {
                self.expect(&TokenKind::RParen, "`)` after tuple pattern")?;
                break;
            }
        }
        Ok(Pattern::Tuple(fields))
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
        if self.async_depth > 0 && self.at_context_ident("await") {
            self.advance();
            let operand = self.unary(allow_trailing_closure)?;
            Ok(Expr::Await(Box::new(operand)))
        } else if self.take(&TokenKind::Minus) {
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
            self.borrow_expression(&borrow, allow_trailing_closure)
        } else if self.take(&TokenKind::Mut) {
            Ok(Expr::Name("mut".to_owned()))
        } else {
            self.postfix(allow_trailing_closure)
        }
    }

    fn borrow_expression(
        &mut self,
        operator: &Token,
        _allow_trailing_closure: bool,
    ) -> Result<Expr, ParseError> {
        let (mutable, access) = if self.borrow_qualifier_group_follows() {
            let (mutable, access, _) = self.optional_borrow_arguments()?;
            (mutable, access)
        } else {
            (false, None)
        };
        if self.at(&TokenKind::LParen) {
            return self.borrow_group_expression(mutable, access, operator);
        }
        Err(self.error_at(
            operator,
            "borrow expressions are written as `borrow(value)`",
        ))
    }

    fn borrow_group_expression(
        &mut self,
        mutable: bool,
        access: Option<String>,
        operator: &Token,
    ) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LParen, "`(` before borrow operand")?;
        if self.at(&TokenKind::RParen) {
            return Err(self.error_here("borrow operand cannot be empty"));
        }
        let value = self.expression(true)?;
        self.expect(&TokenKind::RParen, "`)` after borrow operand")?;
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
        let mut can_take_trailing_closure = false;

        loop {
            if self.at(&TokenKind::LParen) && Self::starts_implicit_handler_groups(&expression) {
                return Err(self.error_here(
                    "`handle` clauses use named trailing groups; omit the parenthesized argument group",
                ));
            } else if self.take(&TokenKind::LParen) {
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
                can_take_trailing_closure = true;
            } else if self.take(&TokenKind::LBracket) {
                let index = self.expression(true)?;
                self.expect(&TokenKind::RBracket, "`]` after index")?;
                expression = Expr::Index {
                    base: Box::new(expression),
                    index: Box::new(index),
                };
            } else if self.take(&TokenKind::Dot) {
                let member =
                    if self.at(&TokenKind::Super) && Self::is_super_path_expression(&expression) {
                        self.advance();
                        "super".to_owned()
                    } else if matches!(self.current().kind, TokenKind::Integer(_)) {
                        let TokenKind::Integer(index) = self.current().kind else {
                            unreachable!()
                        };
                        self.advance();
                        index.to_string()
                    } else {
                        self.expect_relative_path_segment("a member name after `.`")?
                    };
                expression = Expr::Member(Box::new(expression), member);
            } else if self.take(&TokenKind::QuestionDot) {
                let member = self.expect_ident("a member name after `?.`")?;
                expression = Expr::ChainMember(Box::new(expression), member);
            } else if self.take(&TokenKind::Bang) {
                let force = if self.take(&TokenKind::Bang) {
                    if self.at(&TokenKind::Bang) {
                        return Err(self.error_here(
                            "postfix operators accept at most two consecutive `!` tokens",
                        ));
                    }
                    true
                } else {
                    false
                };
                let method = if force { "$lang$unwrap" } else { "$lang$raise" };
                expression = Expr::Call(
                    Box::new(Expr::Member(Box::new(expression), method.to_owned())),
                    Vec::new(),
                );
            } else if self.struct_literal_follows(&expression) {
                let fields = self.struct_literal_fields()?;
                expression = Expr::StructLiteral {
                    constructor: Box::new(expression),
                    fields,
                };
            } else if allow_trailing_closure && self.at(&TokenKind::LBrace) {
                if Self::starts_implicit_handler_groups(&expression) {
                    return Err(self.error_here(
                        "a handler action must use the named trailing group `action { ... }`",
                    ));
                }
                let start_byte = self.current().start_byte;
                let closure = self.trailing_closure(start_byte)?;
                expression = Expr::Call(
                    Box::new(expression),
                    vec![CallArg {
                        label: None,
                        value: closure,
                    }],
                );
                can_take_trailing_closure = true;
            } else if allow_trailing_closure
                && (can_take_trailing_closure || Self::starts_implicit_handler_groups(&expression))
                && self.named_trailing_closure_follows()
            {
                let start_byte = self.current().start_byte;
                let label = self.expect_ident("a trailing closure label")?;
                self.expect(&TokenKind::Colon, "`:` after trailing closure label")?;
                let closure = self.trailing_closure(start_byte)?;
                let completes_handler =
                    label == "action" && Self::starts_implicit_handler_groups(&expression);
                expression = Expr::Call(
                    Box::new(expression),
                    vec![CallArg {
                        label: Some(label),
                        value: closure,
                    }],
                );
                if completes_handler {
                    break;
                }
                can_take_trailing_closure = true;
            } else if allow_trailing_closure
                && (can_take_trailing_closure || Self::starts_implicit_handler_groups(&expression))
                && self.colonless_named_trailing_closure_follows()
            {
                let start_byte = self.current().start_byte;
                let label = self.take_trailing_label()?;
                let closure = self.trailing_closure(start_byte)?;
                let completes_handler =
                    label == "action" && Self::starts_implicit_handler_groups(&expression);
                expression = Expr::Call(
                    Box::new(expression),
                    vec![CallArg {
                        label: Some(label),
                        value: closure,
                    }],
                );
                if completes_handler {
                    break;
                }
                can_take_trailing_closure = true;
            } else if allow_trailing_closure
                && (can_take_trailing_closure || Self::starts_implicit_handler_groups(&expression))
                && self.named_nested_trailing_call_follows()
            {
                let label = self.take_trailing_label()?;
                let nested = self.expression(true)?;
                expression = Expr::Call(
                    Box::new(expression),
                    vec![CallArg {
                        label: Some(label),
                        value: Expr::Closure(Vec::new(), Box::new(nested)),
                    }],
                );
                can_take_trailing_closure = true;
            } else if allow_trailing_closure && self.bare_call_argument_can_start() {
                let argument = self.unary(false)?;
                expression = Expr::Call(
                    Box::new(expression),
                    vec![CallArg {
                        label: None,
                        value: argument,
                    }],
                );
                can_take_trailing_closure = true;
            } else if allow_trailing_closure && self.at(&TokenKind::Newline) {
                let before_newlines = self.index;
                while self.take(&TokenKind::Newline) {}
                if (can_take_trailing_closure
                    && (self.at(&TokenKind::LBrace) || self.named_trailing_closure_follows()))
                    || (Self::starts_implicit_handler_groups(&expression)
                        && (self.colonless_named_trailing_closure_follows()
                            || self.named_nested_trailing_call_follows()))
                {
                    continue;
                }
                self.index = before_newlines;
                break;
            } else {
                break;
            }
        }

        Ok(expression)
    }

    fn starts_implicit_handler_groups(expression: &Expr) -> bool {
        match expression {
            Expr::Member(_, member) => member == "handle",
            Expr::Call(callee, _) => Self::starts_implicit_handler_groups(callee),
            _ => false,
        }
    }

    fn trailing_closure(&mut self, start_byte: usize) -> Result<Expr, ParseError> {
        let closure = self.closure()?;
        self.layout.trailing_closures.push(SourceTrailingClosure {
            start_byte,
            close_byte: self.previous().start_byte,
        });
        Ok(closure)
    }

    fn token_can_start_bare_call_argument(kind: &TokenKind) -> bool {
        match kind {
            TokenKind::Ident(name) => name != "match",
            TokenKind::Integer(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Copy
            | TokenKind::Move
            | TokenKind::Root
            | TokenKind::Super
            | TokenKind::LParen
            | TokenKind::LBracket => true,
            _ => false,
        }
    }

    fn bare_call_argument_can_start(&self) -> bool {
        Self::token_can_start_bare_call_argument(&self.current().kind)
    }

    fn named_trailing_closure_follows(&self) -> bool {
        self.trailing_label_at(self.index)
            .is_some_and(|label| label != "match")
            && self.at_offset(1, &TokenKind::Colon)
            && self.at_offset(2, &TokenKind::LBrace)
    }

    fn colonless_named_trailing_closure_follows(&self) -> bool {
        self.trailing_label_at(self.index)
            .is_some_and(|label| label != "match")
            && self
                .tokens
                .get(self.index + 1)
                .is_some_and(|token| token.kind == TokenKind::LBrace)
    }

    fn named_nested_trailing_call_follows(&self) -> bool {
        self.trailing_label_at(self.index)
            .is_some_and(|label| label != "match")
            && self
                .tokens
                .get(self.index + 1)
                .is_some_and(|token| Self::token_can_start_expression(&token.kind))
    }

    fn trailing_label_at(&self, index: usize) -> Option<String> {
        match self.tokens.get(index).map(|token| &token.kind) {
            Some(TokenKind::Ident(label)) => Some(label.clone()),
            Some(TokenKind::Do) => Some("do".to_owned()),
            Some(TokenKind::Else) => Some("else".to_owned()),
            Some(TokenKind::While) => Some("while".to_owned()),
            _ => None,
        }
    }

    fn take_trailing_label(&mut self) -> Result<String, ParseError> {
        let Some(label) = self.trailing_label_at(self.index) else {
            return Err(self.error_here("expected a trailing argument label"));
        };
        self.advance();
        Ok(label)
    }

    fn token_can_start_expression(token: &TokenKind) -> bool {
        matches!(
            token,
            TokenKind::Integer(_)
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Ident(_)
                | TokenKind::Root
                | TokenKind::Super
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::Minus
                | TokenKind::Bang
                | TokenKind::Borrow
                | TokenKind::If
                | TokenKind::Loop
                | TokenKind::While
                | TokenKind::For
        )
    }

    fn struct_literal_follows(&self, expression: &Expr) -> bool {
        self.at(&TokenKind::LBrace)
            && Self::expression_can_head_struct_literal(expression)
            && matches!(
                (
                    self.tokens.get(self.index + 1).map(|token| &token.kind),
                    self.tokens.get(self.index + 2).map(|token| &token.kind),
                ),
                (Some(TokenKind::Ident(_)), Some(TokenKind::Colon))
            )
    }

    fn expression_can_head_struct_literal(expression: &Expr) -> bool {
        !Self::struct_literal_root(expression).is_empty()
    }

    fn struct_literal_root(expression: &Expr) -> &str {
        match expression {
            Expr::Name(name) => name,
            Expr::Call(callee, _) => Self::struct_literal_root(callee),
            Expr::Member(_, member) => member,
            _ => "",
        }
    }

    fn struct_literal_fields(&mut self) -> Result<Vec<CallArg>, ParseError> {
        self.expect(&TokenKind::LBrace, "`{` before struct fields")?;
        let mut fields = Vec::new();
        if self.take(&TokenKind::RBrace) {
            return Ok(fields);
        }
        loop {
            let label = self.expect_ident("a struct literal field name")?;
            self.expect(&TokenKind::Colon, "`:` after struct literal field name")?;
            fields.push(CallArg {
                label: Some(label),
                value: self.expression(true)?,
            });
            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RBrace) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RBrace, "`}` after struct literal fields")?;
                break;
            }
        }
        Ok(fields)
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
            TokenKind::String(value) => {
                self.advance();
                Ok(Expr::String(value))
            }
            TokenKind::Copy => {
                self.advance();
                Ok(Expr::Name("copy".to_owned()))
            }
            TokenKind::Move => {
                self.advance();
                Ok(Expr::Name("move".to_owned()))
            }
            TokenKind::Ident(ref name)
                if name == "do" && self.at_offset(1, &TokenKind::LBrace) =>
            {
                self.advance();
                self.do_expression()
            }
            TokenKind::Ident(ref name)
                if name == "async" && self.at_offset(1, &TokenKind::LBrace) =>
            {
                self.advance();
                self.async_expression()
            }
            TokenKind::Ident(ref name)
                if name == "try" && self.at_offset(1, &TokenKind::LBrace) =>
            {
                self.advance();
                if !self.at(&TokenKind::LBrace) {
                    return Err(self.error_at(&token, "expected a block after `try`"));
                }
                Ok(Expr::Try(Box::new(self.block()?)))
            }
            TokenKind::Ident(ref name)
                if name == "unsafe" && self.at_offset(1, &TokenKind::LBrace) =>
            {
                self.advance();
                if !self.at(&TokenKind::LBrace) {
                    if self.at(&TokenKind::Comma) || self.at(&TokenKind::RParen) {
                        return Ok(Expr::Name("unsafe".to_owned()));
                    }
                    return Err(self.error_here(
                        "expected a trailing closure after `unsafe`; write `unsafe { ... }`",
                    ));
                }
                Ok(Expr::Unsafe(Box::new(Expr::DoBlock {
                    body: Box::new(self.block()?),
                })))
            }
            TokenKind::Ident(ref name)
                if name == "throw"
                    && (self.at_offset(1, &TokenKind::LParen)
                        || Self::token_can_start_bare_call_argument(
                            &self.tokens[self.index + 1].kind,
                        )) =>
            {
                self.advance();
                Ok(Self::core_control_function("throw"))
            }
            TokenKind::Ident(ref name) if name == "return" => {
                self.return_expression(allow_trailing_closure)
            }
            TokenKind::Ident(ref name) if name == "if" => self.if_expression(),
            TokenKind::Ident(ref name)
                if name == "while" && self.at_offset(1, &TokenKind::LParen) =>
            {
                self.advance();
                Ok(Expr::Name("while".to_owned()))
            }
            TokenKind::Ident(ref name) if name == "while" => self.while_expression(),
            TokenKind::Ident(ref name) if name == "for" => self.for_expression(),
            TokenKind::Ident(ref name) if name == "match" => self.prefix_match_expression(),
            TokenKind::Ident(ref name)
                if name == "loop" && self.at_offset(1, &TokenKind::LBrace) =>
            {
                self.loop_expression()
            }
            TokenKind::Ident(ref name) if name == "break" => {
                self.break_expression(allow_trailing_closure)
            }
            TokenKind::Ident(ref name) if name == "continue" => {
                self.continue_expression()
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
                let first = self.expression(true)?;
                if !self.take(&TokenKind::Comma) {
                    self.expect(&TokenKind::RParen, "`)` after parenthesized expression")?;
                    return Ok(first);
                }
                let mut fields = vec![first];
                while !self.take(&TokenKind::RParen) {
                    fields.push(self.expression(true)?);
                    if !self.take(&TokenKind::Comma) {
                        self.expect(&TokenKind::RParen, "`)` after tuple literal")?;
                        break;
                    }
                }
                Ok(Expr::Tuple(fields))
            }
            TokenKind::LBracket => self.array_literal(),
            TokenKind::Do => {
                self.advance();
                self.do_expression()
            }
            TokenKind::Try => {
                self.advance();
                if !self.at(&TokenKind::LBrace) {
                    return Err(self.error_at(&token, "expected a block after `try`"));
                }
                Ok(Expr::Try(Box::new(self.block()?)))
            }
            TokenKind::Unsafe => {
                self.advance();
                if !self.at(&TokenKind::LBrace) {
                    if self.at(&TokenKind::Comma) || self.at(&TokenKind::RParen) {
                        return Ok(Expr::Name("unsafe".to_owned()));
                    }
                    return Err(self.error_here(
                        "expected a trailing closure after `unsafe`; write `unsafe { ... }`",
                    ));
                }
                Ok(Expr::Unsafe(Box::new(Expr::DoBlock {
                    body: Box::new(self.block()?),
                })))
            }
            TokenKind::If => self.if_expression(),
            TokenKind::Match => self.prefix_match_expression(),
            TokenKind::Return => self.return_expression(allow_trailing_closure),
            TokenKind::Throw => {
                self.advance();
                if !self.at(&TokenKind::LParen) && !self.at_control_expression_boundary() {
                    return Err(self.error_at(
                        &token,
                        "`throw` is a function; write `throw(error)`",
                    ));
                }
                Ok(Self::core_control_function("throw"))
            }
            TokenKind::While if self.at_offset(1, &TokenKind::LParen) => {
                self.advance();
                Ok(Expr::Name("while".to_owned()))
            }
            TokenKind::While => self.while_expression(),
            TokenKind::For => self.for_expression(),
            TokenKind::Loop => self.loop_expression(),
            TokenKind::Break => self.break_expression(allow_trailing_closure),
            TokenKind::Continue => {
                self.continue_expression()
            }
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
        if self.named_trailing_closure_follows() || self.colonless_named_trailing_closure_follows()
        {
            let label = self.take_trailing_label()?;
            if label != "then" {
                return Err(self.error_here("the first named `if` trailing closure must be `then`"));
            }
            self.take(&TokenKind::Colon);
        }
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("expected `{` after `if` condition"));
        }
        let then_branch = self.block()?;
        let else_branch = self
            .optional_else_branch()?
            .map_or(Expr::Unit, |branch| *branch);

        Ok(Expr::Call(
            Box::new(Expr::Call(
                Box::new(Expr::Call(
                    Box::new(Self::core_if_function()),
                    vec![CallArg {
                        label: None,
                        value: condition,
                    }],
                )),
                vec![CallArg {
                    label: None,
                    value: Expr::Closure(Vec::new(), Box::new(then_branch)),
                }],
            )),
            vec![CallArg {
                label: None,
                value: Expr::Closure(Vec::new(), Box::new(else_branch)),
            }],
        ))
    }

    fn optional_else_branch(&mut self) -> Result<Option<Box<Expr>>, ParseError> {
        // A second trailing closure is the lazy else branch. It may begin on
        // the next logical line. If absent, restore the newlines so the
        // containing block can still see its separator.
        let before_newlines = self.index;
        while self.take(&TokenKind::Newline) {}
        if self.at(&TokenKind::LBrace) {
            return Ok(Some(Box::new(self.block()?)));
        }
        if self.trailing_label_at(self.index).as_deref() == Some("else") {
            self.advance();
            self.take(&TokenKind::Colon);
            if self.at(&TokenKind::LBrace) {
                return Ok(Some(Box::new(self.block()?)));
            }
            if Self::token_can_start_expression(&self.current().kind) {
                return Ok(Some(Box::new(self.expression(true)?)));
            }
            return Err(self.error_here("expected a closure or nested trailing call after `else`"));
        }
        self.index = before_newlines;
        Ok(None)
    }

    fn return_expression(&mut self, allow_trailing_closure: bool) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Return, "`return`")?;
        if !self.take(&TokenKind::LParen) {
            if self.at_control_expression_boundary() {
                return Err(self.error_here(
                    "`return` requires one value or an explicit empty group `return()`",
                ));
            }
            return Ok(Expr::Return(Some(Box::new(
                self.expression(allow_trailing_closure)?,
            ))));
        }
        if self.take(&TokenKind::RParen) {
            return Ok(Expr::Return(None));
        }
        let value = self.expression(allow_trailing_closure)?;
        self.expect(&TokenKind::RParen, "`)` after `return` argument")?;
        Ok(Expr::Return(Some(Box::new(value))))
    }

    fn while_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::While, "`while`")?;
        while self.take(&TokenKind::Newline) {}
        if self.at(&TokenKind::LBrace)
            || matches!(&self.current().kind, TokenKind::Ident(name) if name == "condition")
                && (self.named_trailing_closure_follows()
                    || self.colonless_named_trailing_closure_follows())
        {
            if self.named_trailing_closure_follows()
                || self.colonless_named_trailing_closure_follows()
            {
                let label = self.take_trailing_label()?;
                if label != "condition" {
                    return Err(self.error_here(
                        "the first named `while` trailing closure must be `condition`",
                    ));
                }
                self.take(&TokenKind::Colon);
            }
            let condition = self.zero_parameter_trailing_closure("while condition")?;
            while self.take(&TokenKind::Newline) {}
            if self.named_trailing_closure_follows()
                || self.colonless_named_trailing_closure_follows()
            {
                let label = self.take_trailing_label()?;
                if label != "do" {
                    return Err(
                        self.error_here("the second named `while` trailing closure must be `do`")
                    );
                }
                self.take(&TokenKind::Colon);
            }
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here("expected a second trailing closure for `while`"));
            }
            let body = self.zero_parameter_trailing_closure("while body")?;
            return Ok(Expr::While {
                condition: Box::new(condition),
                body: Box::new(body),
                post_test: false,
            });
        }
        Err(self.error_here(
            "`while` requires condition and `do` closures; write `while { condition } do { body }`",
        ))
    }

    fn do_expression(&mut self) -> Result<Expr, ParseError> {
        let body = self.block()?;
        let before_newlines = self.index;
        while self.take(&TokenKind::Newline) {}
        if self.trailing_label_at(self.index).as_deref() == Some("while")
            && (self.named_trailing_closure_follows()
                || self.colonless_named_trailing_closure_follows())
        {
            self.advance();
            self.take(&TokenKind::Colon);
            let condition = self.zero_parameter_trailing_closure("do-while condition")?;
            return Ok(Expr::While {
                condition: Box::new(condition),
                body: Box::new(body),
                post_test: true,
            });
        }
        self.index = before_newlines;
        Ok(Expr::DoBlock {
            body: Box::new(body),
        })
    }

    fn zero_parameter_trailing_closure(&mut self, context: &str) -> Result<Expr, ParseError> {
        // `while` and `do ... while` use trailing-closure surface syntax, but
        // their blocks are control-flow scopes rather than closure boundaries.
        // Preserve an enclosing async context so contextual `await` remains
        // available in the condition and body.
        let closure = self.closure_inner()?;
        let Expr::Closure(parameters, body) = closure else {
            unreachable!("a closure parser always returns an outer closure")
        };
        if !parameters.is_empty() {
            return Err(self.error_here(format!("{context} must be a zero-parameter closure")));
        }
        Ok(*body)
    }

    fn for_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::For, "`for`")?;
        let iterable = self.expression(false)?;
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here(
                "`for` requires a trailing pattern closure; write `for iterable { pattern -> body }`",
            ));
        }
        self.expect(&TokenKind::LBrace, "`{` before the `for` body pattern")?;
        self.skip_separators();
        let pattern = self.pattern()?;
        self.expect(&TokenKind::Arrow, "`->` after the `for` body pattern")?;
        let body = self.block_contents()?;

        let id = self.next_control_binding;
        self.next_control_binding += 1;
        let iterator = format!("$for$iterator${id}");
        let loop_result = format!("$for$result${id}");
        let into_iter = Expr::Call(
            Box::new(Expr::Member(
                Box::new(iterable),
                "$lang$into_iter".to_owned(),
            )),
            Vec::new(),
        );
        let next = Expr::Call(
            Box::new(Expr::Member(
                Box::new(Expr::Name(iterator.clone())),
                "$lang$next".to_owned(),
            )),
            Vec::new(),
        );
        let loop_body = Expr::Match {
            scrutinee: Box::new(next),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Constructor {
                        path: vec!["some".to_owned()],
                        fields: PatternFields::Positional(vec![pattern]),
                    },
                    guard: None,
                    body,
                },
                MatchArm {
                    pattern: Pattern::Constructor {
                        path: vec!["none".to_owned()],
                        fields: PatternFields::Unit,
                    },
                    guard: None,
                    body: Expr::Break(None),
                },
            ],
        };

        Ok(Expr::Block(
            vec![
                Stmt::Let(Binding {
                    value_source: None,
                    mutable: true,
                    name: iterator,
                    annotation: None,
                    value: into_iter,
                }),
                Stmt::Let(Binding {
                    value_source: None,
                    mutable: false,
                    name: loop_result,
                    annotation: Some(Type::Unit),
                    value: Expr::Loop {
                        body: Box::new(loop_body),
                    },
                }),
            ],
            None,
        ))
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
        if !self.take(&TokenKind::LParen) {
            if self.at_control_expression_boundary() {
                return Err(self.error_here(
                    "`break` requires one value or an explicit empty group `break()`",
                ));
            }
            return Ok(Expr::Break(Some(Box::new(
                self.expression(allow_trailing_closure)?,
            ))));
        }
        if self.take(&TokenKind::RParen) {
            return Ok(Expr::Break(None));
        }
        let value = self.expression(allow_trailing_closure)?;
        self.expect(&TokenKind::RParen, "`)` after `break` argument")?;
        Ok(Expr::Break(Some(Box::new(value))))
    }

    fn continue_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::Continue, "`continue`")?;
        self.expect(&TokenKind::LParen, "`(` after `continue`")?;
        self.expect(&TokenKind::RParen, "`)` after `continue`")?;
        Ok(Expr::Continue)
    }

    fn block(&mut self) -> Result<Expr, ParseError> {
        let open = self.current().clone();
        let body_start_byte = self.body_start_byte(self.index + 1);
        self.expect(&TokenKind::LBrace, "`{`")?;
        let body = self.block_contents()?;
        let close = self.previous().clone();
        self.layout.blocks.push(SourceBracedRegion {
            open_byte: open.start_byte,
            close_byte: close.start_byte,
            body_start_byte,
            open_line: open.line,
            close_line: close.line,
        });
        Ok(body)
    }

    fn core_control_function(name: &str) -> Expr {
        let module = if name == "throw" { "error" } else { "control" };
        Expr::Member(
            Box::new(Expr::Member(
                Box::new(Expr::Name("core".to_owned())),
                module.to_owned(),
            )),
            name.to_owned(),
        )
    }

    fn core_match_function() -> Expr {
        Expr::Name("$lang$match".to_owned())
    }

    fn core_if_function() -> Expr {
        Expr::Name("$lang$if".to_owned())
    }

    fn located_expression(start: &Token, end: &Token, value: Expr) -> Expr {
        Expr::Located {
            line: start.line,
            column: start.column,
            end_line: end.end_line,
            end_column: end.end_column,
            value: Box::new(value),
        }
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

            let expression_start = self.current().clone();
            let expression = self.expression(true)?;
            let expression_end = self.previous().clone();
            let expression =
                Self::located_expression(&expression_start, &expression_end, expression);
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
        let async_depth = self.async_depth;
        self.async_depth = 0;
        let result = self.closure_inner();
        self.async_depth = async_depth;
        result
    }

    fn closure_inner(&mut self) -> Result<Expr, ParseError> {
        let open = self.current().clone();
        let body_start_byte = self.body_start_byte(self.index + 1);
        let closure = self.closure_inner_untracked()?;
        let close = self.previous().clone();
        self.layout.closures.push(SourceBracedRegion {
            open_byte: open.start_byte,
            close_byte: close.start_byte,
            body_start_byte,
            open_line: open.line,
            close_line: close.line,
        });
        Ok(closure)
    }

    fn closure_inner_untracked(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_separators();
        if self.take(&TokenKind::RBrace) {
            return Ok(Expr::Closure(
                Vec::new(),
                Box::new(Expr::Block(Vec::new(), None)),
            ));
        }

        if self.at(&TokenKind::Arrow) {
            return Err(
                self.error_here("zero-parameter closures do not use `->`; write `{ expression }`")
            );
        }

        if !self.at(&TokenKind::LParen) {
            let pattern_start = self.index;
            if let Ok(pattern) = self.pattern() {
                let guard = if self.take(&TokenKind::If) {
                    Some(Box::new(self.expression(false)?))
                } else {
                    None
                };
                if self.take(&TokenKind::Arrow) {
                    return Ok(Expr::PatternClosure {
                        pattern,
                        guard,
                        body: Box::new(self.block_contents()?),
                    });
                }
            }
            self.index = pattern_start;
        }

        let mut groups = Vec::new();
        if self.closure_parameter_arrow_follows() {
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

    fn body_start_byte(&self, mut index: usize) -> usize {
        while matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Newline | TokenKind::Semicolon)
        ) {
            index += 1;
        }
        self.tokens
            .get(index)
            .map_or_else(|| self.current().end_byte, |token| token.start_byte)
    }

    fn async_expression(&mut self) -> Result<Expr, ParseError> {
        self.async_depth += 1;
        let body = self.block();
        self.async_depth -= 1;
        body.map(|body| Expr::Async {
            body: Box::new(body),
        })
    }

    fn closure_parameter_arrow_follows(&self) -> bool {
        let mut index = self.index;
        let mut saw_group = false;

        while matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::LParen)
        ) {
            saw_group = true;
            let mut depth = 0_usize;
            loop {
                let Some(token) = self.tokens.get(index) else {
                    return false;
                };
                match token.kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => {
                        depth -= 1;
                        if depth == 0 {
                            index += 1;
                            break;
                        }
                    }
                    TokenKind::Eof => return false,
                    _ => {}
                }
                index += 1;
            }
            while matches!(
                self.tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::Newline)
            ) {
                index += 1;
            }
        }

        saw_group
            && matches!(
                self.tokens.get(index).map(|token| &token.kind),
                Some(TokenKind::Arrow)
            )
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

    fn at_context_ident(&self, expected: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Ident(name) if name == expected)
    }

    fn at_offset(&self, offset: usize, kind: &TokenKind) -> bool {
        self.kind_at(self.index + offset, kind)
    }

    fn kind_at(&self, index: usize, kind: &TokenKind) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| token_kind_matches(&token.kind, kind))
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
            TokenKind::Mut => {
                self.advance();
                Ok("mut".to_owned())
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
        token_kind_matches(&self.current().kind, kind)
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
            start_byte: token.start_byte,
            end_byte: token.end_byte,
            line: token.line,
            column: token.column,
        }
    }
}

fn contextual_spelling(kind: &TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::Mut => "mut",
        TokenKind::Copy => "copy",
        TokenKind::Move => "move",
        TokenKind::Comptime => "comptime",
        TokenKind::Borrow => "borrow",
        TokenKind::Type => "type",
        TokenKind::Region => "region",
        TokenKind::Unsafe => "unsafe",
        TokenKind::Do => "do",
        TokenKind::Throw => "throw",
        TokenKind::Try => "try",
        TokenKind::If => "if",
        TokenKind::Else => "else",
        TokenKind::Return => "return",
        TokenKind::While => "while",
        TokenKind::For => "for",
        TokenKind::Loop => "loop",
        TokenKind::Break => "break",
        TokenKind::Continue => "continue",
        TokenKind::Match => "match",
        _ => return None,
    })
}

fn token_kind_matches(actual: &TokenKind, expected: &TokenKind) -> bool {
    match (actual, contextual_spelling(expected)) {
        (TokenKind::Ident(actual), Some(expected)) => actual == expected,
        _ => std::mem::discriminant(actual) == std::mem::discriminant(expected),
    }
}

fn foreign_link_name_is_valid(link_name: &str) -> bool {
    let mut bytes = link_name.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'.' | b'$'));
    link_name.is_ascii()
        && valid_start
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$'))
}

fn describe(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Let => "`let`",
        TokenKind::Pub => "`pub`",
        TokenKind::Package => "`package`",
        TokenKind::Root => "`root`",
        TokenKind::Super => "`super`",
        TokenKind::Mut => "`mut`",
        TokenKind::Copy => "`copy`",
        TokenKind::Move => "`move`",
        TokenKind::Comptime => "`comptime`",
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
        TokenKind::For => "`for`",
        TokenKind::In => "`in`",
        TokenKind::Loop => "`loop`",
        TokenKind::Break => "`break`",
        TokenKind::Continue => "`continue`",
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
        TokenKind::String(_) => "a string",
        TokenKind::Integer(_) => "an integer",
        TokenKind::LParen => "`(`",
        TokenKind::RParen => "`)`",
        TokenKind::LBracket => "`[`",
        TokenKind::RBracket => "`]`",
        TokenKind::LBrace => "`{`",
        TokenKind::RBrace => "`}`",
        TokenKind::Colon => "`:`",
        TokenKind::Dot => "`.`",
        TokenKind::Ellipsis => "`...`",
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
        TokenKind::PlusEqual => "`+=`",
        TokenKind::Minus => "`-`",
        TokenKind::MinusEqual => "`-=`",
        TokenKind::Star => "`*`",
        TokenKind::StarEqual => "`*=`",
        TokenKind::Slash => "`/`",
        TokenKind::SlashEqual => "`/=`",
        TokenKind::Percent => "`%`",
        TokenKind::PercentEqual => "`%=`",
        TokenKind::Less => "`<`",
        TokenKind::LessEqual => "`<=`",
        TokenKind::Greater => "`>`",
        TokenKind::GreaterEqual => "`>=`",
        TokenKind::AndAnd => "`&&`",
        TokenKind::OrOr => "`||`",
        TokenKind::Amp => "`&`",
        TokenKind::AmpEqual => "`&=`",
        TokenKind::Pipe => "`|`",
        TokenKind::PipeEqual => "`|=`",
        TokenKind::Caret => "`^`",
        TokenKind::CaretEqual => "`^=`",
        TokenKind::Shl => "`<<`",
        TokenKind::ShlEqual => "`<<=`",
        TokenKind::Shr => "`>>`",
        TokenKind::ShrEqual => "`>>=`",
        TokenKind::QuestionQuestion => "`??`",
        TokenKind::QuestionDot => "`?.`",
        TokenKind::Eof => "end of file",
    }
}

#[cfg(test)]
mod tests;
