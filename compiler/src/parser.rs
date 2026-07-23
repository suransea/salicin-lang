use std::{collections::HashSet, fmt};

use crate::ast::{
    default_trait_self_parameter, AssociatedTypeBinding, BinaryOp, Binding, CallArg, CompileParam,
    CompileParamDefault, CompileParamKind, DomainDef, EffectDef, EnumDef, Expr, ExtendDef,
    ExtendMember, Field, Function, FunctionEffects, Item, MatchArm, Param, PassMode, Pattern,
    PatternField, PatternFields, Program, Stmt, StructDef, TraitDef, TraitMember, Type,
    TypeAliasDef, TypeArg, TypeFormDef, UnaryOp, UseDecl, VariantDef, VariantFields, Visibility,
    WherePredicate,
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
    Parser {
        tokens,
        index: 0,
        effect_parameters_in_scope: HashSet::new(),
        next_control_binding: 0,
    }
    .program()
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    effect_parameters_in_scope: HashSet<String>,
    next_control_binding: usize,
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

        if let Err(message) = normalize_and_validate_scopes(&mut items) {
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
        let name = self.declaration_name()?;

        let (compile_groups, groups) = self.declaration_groups(false, &[])?;

        if mutable && (!compile_groups.is_empty() || !groups.is_empty()) {
            return Err(self.error_here("`let mut` cannot declare a function"));
        }

        if groups.is_empty() && self.at(&TokenKind::Colon) {
            if self.at_offset(1, &TokenKind::Type) {
                self.advance();
                self.advance();
                self.take_newlines_if_followed_by(&[TokenKind::Equal]);
                return if self.at(&TokenKind::Equal) {
                    self.type_alias(name, compile_groups, mutable)
                } else {
                    self.type_form_definition(name, compile_groups, mutable)
                };
            }
            if self.type_constructor_signature_follows() {
                self.advance();
                let mut alias_groups = Vec::new();
                while self.group_starts_with_compile_parameter() {
                    alias_groups.push(self.compile_parameter_group()?);
                }
                self.expect(&TokenKind::Colon, "`:` before type-constructor result kind")?;
                self.expect(&TokenKind::Type, "`type` as type-constructor result kind")?;
                return self.type_constructor_alias(name, alias_groups, mutable);
            }
        }

        let logical_result = if self.take(&TokenKind::Colon) {
            Some(self.function_result_type()?)
        } else {
            None
        };
        let (effects, throws_error, has_effect_group) = self.function_effect_clause()?;
        if throws_error.is_some() && logical_result.is_none() {
            return Err(self.error_here(
                "`Throws(Error)` requires an explicit logical return type before `with(...)`",
            ));
        }
        let annotation =
            logical_result.map(|result| Self::apply_throws_effect(result, throws_error));
        self.effect_parameters_in_scope.clear();

        if !compile_groups.is_empty() || !groups.is_empty() {
            self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::Equal]);
        }

        let where_predicates = self.where_clause()?;
        if !where_predicates.is_empty() && compile_groups.is_empty() {
            return Err(self.error_here("`where` requires compile-time parameters"));
        }
        self.take_newlines_if_followed_by(&[TokenKind::Equal]);

        if !self.at(&TokenKind::Equal) && (!compile_groups.is_empty() || !groups.is_empty()) {
            return Ok(Item::Function(Function {
                name,
                compile_groups,
                groups,
                return_type: annotation,
                effects,
                where_predicates,
                body: None,
            }));
        }

        self.expect(&TokenKind::Equal, "`=`")?;

        if self.at_context_ident("effect") {
            if mutable
                || annotation.is_some()
                || has_effect_group
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
            if mutable
                || annotation.is_some()
                || has_effect_group
                || !compile_groups.is_empty()
                || !groups.is_empty()
                || !where_predicates.is_empty()
            {
                return Err(self.error_here(
                    "domain declarations cannot be mutable, generic, annotated, or have parameters",
                ));
            }
            self.advance();
            return self.domain_definition(name).map(Item::Domain);
        }

        if self.at(&TokenKind::Struct) || self.at(&TokenKind::Enum) || self.at(&TokenKind::Trait) {
            if mutable || annotation.is_some() || has_effect_group || !groups.is_empty() {
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
            if has_effect_group {
                return Err(self.error_here("effect annotations require a function declaration"));
            }
            let value = self.expression(true)?;
            Ok(Item::Global(Binding {
                mutable,
                name,
                annotation,
                value,
            }))
        } else {
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here(
                    "named closure declarations require a braced body; write `= { expression }`",
                ));
            }
            let body = self.block()?;
            Ok(Item::Function(Function {
                name,
                compile_groups,
                groups,
                return_type: annotation,
                effects,
                where_predicates,
                body: Some(body),
            }))
        }
    }

    fn effect_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
    ) -> Result<EffectDef, ParseError> {
        if !effect_name_is_uppercase(&name) {
            return Err(self.error_here("effect declarations must use an uppercase nominal name"));
        }
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
            let (operation_compile_groups, groups) = self.declaration_groups(false, &[])?;
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
            let (effects, throws_error, _) = self.function_effect_clause()?;
            let return_type = Some(Self::apply_throws_effect(logical_result, throws_error));
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
            && matches!(
                self.tokens.get(self.index + 2).map(|token| &token.kind),
                Some(TokenKind::Ident(_))
            )
            && self.at_offset(3, &TokenKind::Colon)
            && self.at_offset(4, &TokenKind::Type)
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
        if compile_groups
            .iter()
            .flatten()
            .any(|parameter| parameter.kind != CompileParamKind::Type)
        {
            return Err(self
                .error_here("type aliases currently accept only `type` compile-time parameters"));
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
    ) -> Result<Item, ParseError> {
        if mutable {
            return Err(self.error_here("type forms cannot be declared with `let mut`"));
        }
        if compile_groups.is_empty() {
            return Err(self.error_here("type form declarations require compile-time parameters"));
        }
        Ok(Item::TypeForm(TypeFormDef {
            name,
            compile_groups,
        }))
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

    fn domain_definition(&mut self, name: String) -> Result<DomainDef, ParseError> {
        let members = if self.take(&TokenKind::LBrace) {
            self.skip_separators();
            let mut members = Vec::new();
            let mut seen = HashSet::new();
            while !self.take(&TokenKind::RBrace) {
                let member = self.domain_member_name()?;
                if !seen.insert(member.clone()) {
                    return Err(self.error_here(format!("duplicate domain member `{member}`")));
                }
                members.push(member);

                self.take(&TokenKind::Comma);
                self.skip_separators();
            }
            Some(members)
        } else {
            None
        };
        Ok(DomainDef { name, members })
    }

    fn domain_member_name(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        let name = match token.kind {
            TokenKind::Ident(name) if name != "_" => name,
            TokenKind::Mut => "mut".to_owned(),
            TokenKind::Copy => "copy".to_owned(),
            TokenKind::Move => "move".to_owned(),
            TokenKind::Type => "type".to_owned(),
            TokenKind::Region => "region".to_owned(),
            _ => {
                return Err(self.error_at(
                    &token,
                    format!(
                        "expected a domain member name, found {}",
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
        let (compile_groups, runtime_groups) = self.declaration_groups(false, &[])?;
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

        let (compile_groups, groups) = self.declaration_groups(true, &[])?;
        self.validate_receiver_groups(&groups)?;

        let logical_result = if self.take(&TokenKind::Colon) {
            Some(self.function_result_type()?)
        } else {
            None
        };
        let (effects, throws_error, has_effect_group) = self.function_effect_clause()?;
        if throws_error.is_some() && logical_result.is_none() {
            return Err(self.error_here(
                "`Throws(Error)` requires an explicit logical return type before `with(...)`",
            ));
        }
        let annotation =
            logical_result.map(|result| Self::apply_throws_effect(result, throws_error));
        self.effect_parameters_in_scope.clear();
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
            if has_effect_group {
                return Err(self.error_here("effect annotations require a function member"));
            }
            Ok(ExtendMember::Const(Binding {
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
                compile_groups,
                groups,
                return_type: annotation,
                effects,
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
        let mut labeled = 0;
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
                    let label = if matches!(self.current().kind, TokenKind::Ident(_))
                        && self.at_offset(1, &TokenKind::Colon)
                        && !self.at_offset(2, &TokenKind::Type)
                        && !self.at_offset(2, &TokenKind::Region)
                        && !matches!(
                            self.tokens.get(self.index + 2).map(|token| &token.kind),
                            Some(TokenKind::Ident(kind))
                                if matches!(kind.as_str(), "access" | "passing" | "effect")
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
                    self.effect_parameters_in_scope.extend(
                        params
                            .iter()
                            .filter(|parameter| parameter.kind == CompileParamKind::Effect)
                            .map(|parameter| parameter.name.clone()),
                    );
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
            && self.compile_parameter_kind_starts_at(3)
    }

    fn current_starts_compile_parameter(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Ident(_) | TokenKind::RegionName(_)
        ) && self.at_offset(1, &TokenKind::Colon)
            && self.compile_parameter_kind_starts_at(2)
    }

    fn compile_parameter_kind_starts_at(&self, offset: usize) -> bool {
        self.at_offset(offset, &TokenKind::Type)
            || self.at_offset(offset, &TokenKind::Region)
            || self.constructor_compile_parameter_kind_starts_at(offset)
            || matches!(
                self.tokens.get(self.index + offset).map(|token| &token.kind),
                Some(TokenKind::Ident(name))
                    if matches!(name.as_str(), "access" | "passing" | "effect")
            )
    }

    fn constructor_compile_parameter_kind_starts_at(&self, offset: usize) -> bool {
        let mut index = self.index + offset;
        let mut groups = 0;
        while matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::LParen)
        ) {
            groups += 1;
            index += 1;
            loop {
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
                if !matches!(
                    self.tokens.get(index).map(|token| &token.kind),
                    Some(TokenKind::Type)
                ) {
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
        matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Type)
        ) || matches!(
            self.tokens.get(index).map(|token| &token.kind),
            Some(TokenKind::Ident(name)) if name == "effect"
        )
    }

    fn compile_parameter_kind(
        &mut self,
        name_token: &Token,
        name: &str,
        region_name: bool,
    ) -> Result<CompileParamKind, ParseError> {
        if region_name {
            return Err(self.error_at(
                name_token,
                "region literals cannot be compile-time parameter names; write `R: region` for a region parameter",
            ));
        }

        if self.take(&TokenKind::Type) {
            if matches!(name, "_" | "i32" | "i64" | "u32" | "u64" | "bool" | "Never") {
                return Err(self.error_at(
                    name_token,
                    format!(
                        "reserved type name `{name}` cannot be used as a compile-time parameter"
                    ),
                ));
            }
            return Ok(CompileParamKind::Type);
        }

        if let TokenKind::Ident(kind) = self.current().kind.clone() {
            let parameter_kind = match kind.as_str() {
                "access" => Some(CompileParamKind::Access),
                "passing" => Some(CompileParamKind::Passing),
                "effect" => Some(CompileParamKind::Effect),
                _ => None,
            };
            if let Some(parameter_kind) = parameter_kind {
                self.advance();
                return Ok(parameter_kind);
            }
        }

        if self.at(&TokenKind::LParen) {
            if matches!(name, "_" | "i32" | "i64" | "u32" | "u64" | "bool" | "Never") {
                return Err(self.error_at(
                    name_token,
                    format!(
                        "reserved type name `{name}` cannot be used as a compile-time parameter"
                    ),
                ));
            }
            return self.constructor_compile_parameter_kind();
        }

        self.expect(
            &TokenKind::Region,
            "`type`, `access`, `passing`, `effect`, a constructor kind, or `region`",
        )?;
        if name == "static" {
            return Err(self.error_at(
                name_token,
                "region entity `'static` is predefined and cannot be redeclared",
            ));
        }
        if !name.chars().next().is_some_and(char::is_uppercase) {
            return Err(self.error_at(
                name_token,
                "region compile-time parameters must use ordinary uppercase names like `R: region`; region literals use names like `'r`",
            ));
        }
        Ok(CompileParamKind::Region)
    }

    fn constructor_compile_parameter_kind(&mut self) -> Result<CompileParamKind, ParseError> {
        let mut parameter_count = 0;
        while self.at(&TokenKind::LParen) {
            parameter_count += self.constructor_kind_parameter_group()?;
        }
        self.expect(&TokenKind::Colon, "`:` before constructor result kind")?;
        if self.take(&TokenKind::Type) {
            return Ok(CompileParamKind::TypeConstructor { parameter_count });
        }
        if matches!(&self.current().kind, TokenKind::Ident(name) if name == "effect") {
            self.advance();
            return Ok(CompileParamKind::EffectConstructor { parameter_count });
        }
        Err(self.error_here("expected constructor result kind `type` or `effect`"))
    }

    fn constructor_kind_parameter_group(&mut self) -> Result<usize, ParseError> {
        self.expect(&TokenKind::LParen, "`(` in constructor kind")?;
        if self.take(&TokenKind::RParen) {
            return Err(self.error_here("constructor kind parameter groups cannot be empty"));
        }

        let mut parameter_count = 0;
        loop {
            let name_token = self.current().clone();
            let name = self.expect_ident("a constructor kind parameter name")?;
            if matches!(
                name.as_str(),
                "_" | "i32" | "i64" | "u32" | "u64" | "bool" | "Never"
            ) {
                return Err(self.error_at(
                    &name_token,
                    format!(
                        "reserved type name `{name}` cannot be used as a constructor kind parameter"
                    ),
                ));
            }
            self.expect(
                &TokenKind::Colon,
                "`:` after constructor kind parameter name",
            )?;
            if !self.take(&TokenKind::Type) {
                return Err(
                    self.error_here("constructor kind parameters currently must have kind `type`")
                );
            }
            parameter_count += 1;

            if self.take(&TokenKind::Comma) {
                if self.take(&TokenKind::RParen) {
                    break;
                }
            } else {
                self.expect(&TokenKind::RParen, "`)` after constructor kind parameters")?;
                break;
            }
        }

        Ok(parameter_count)
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
            let kind = self.compile_parameter_kind(&name_token, &name, region_name)?;
            let default = self.compile_parameter_default(kind)?;
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
        kind: CompileParamKind,
    ) -> Result<Option<CompileParamDefault>, ParseError> {
        if !self.take(&TokenKind::Equal) {
            return Ok(None);
        }

        let default = match kind {
            CompileParamKind::Access => {
                let name = self.compile_parameter_default_name("an access default")?;
                if !matches!(name.as_str(), "shared" | "mut") {
                    return Err(
                        self.error_here("access parameter defaults must be `shared` or `mut`")
                    );
                }
                CompileParamDefault::Name(name)
            }
            CompileParamKind::Passing => {
                let name = self.compile_parameter_default_name("a passing default")?;
                if !matches!(name.as_str(), "auto" | "copy" | "move") {
                    return Err(self.error_here(
                        "passing parameter defaults must be `auto`, `copy`, or `move`",
                    ));
                }
                CompileParamDefault::Name(name)
            }
            CompileParamKind::Effect => {
                CompileParamDefault::Name(self.compile_parameter_default_name("an effect default")?)
            }
            CompileParamKind::Region => {
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
            CompileParamKind::Type
            | CompileParamKind::TypeConstructor { .. }
            | CompileParamKind::EffectConstructor { .. } => {
                return Err(self.error_here(
                    "defaults for type and constructor parameters are not supported yet",
                ));
            }
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
            } else if self.at(&TokenKind::Borrow) {
                return Err(self.error_here(
                    "borrow parameter mode was removed; write `name: borrow(T)` and pass `borrow(value)` at the call site",
                ));
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
                if self.take(&TokenKind::Colon) {
                    self.type_expr()?
                } else {
                    Type::Named("Self".into(), Vec::new())
                }
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
        let derives = self.struct_options()?;
        self.expect(&TokenKind::LBrace, "`{` after `struct`")?;
        let fields = self.braced_type_fields()?;
        Ok(StructDef {
            name,
            compile_groups,
            derives,
            fields,
        })
    }

    fn struct_options(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.take(&TokenKind::LParen) {
            return Ok(Vec::new());
        }
        let mut derives = Vec::new();
        if self.take(&TokenKind::RParen) {
            return Ok(derives);
        }
        loop {
            let option = self.expect_ident("a struct option name")?;
            self.expect(&TokenKind::Colon, "`:` after struct option name")?;
            match option.as_str() {
                "derive" => {
                    let derive = self.expect_ident("a derive name")?;
                    if derive != "Copy" {
                        return Err(self.error_here(format!(
                            "unsupported struct derive `{derive}`; only `Copy` is supported"
                        )));
                    }
                    if derives.iter().any(|existing| existing == &derive) {
                        return Err(self.error_here(format!("duplicate struct derive `{derive}`")));
                    }
                    derives.push(derive);
                }
                _ => {
                    return Err(self.error_here(format!(
                        "unknown struct option `{option}`; expected `derive`"
                    )));
                }
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
        Ok(derives)
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

    fn trait_definition(
        &mut self,
        name: String,
        compile_groups: Vec<Vec<CompileParam>>,
    ) -> Result<TraitDef, ParseError> {
        self.expect(&TokenKind::Trait, "`trait`")?;
        let self_parameter = if self.at(&TokenKind::LParen) {
            let group = self.compile_parameter_group()?;
            let [parameter] = group.as_slice() else {
                return Err(
                    self.error_here("trait self kind must declare exactly one `Self` parameter")
                );
            };
            if parameter.name != "Self" {
                return Err(self.error_here("trait self kind parameter must be named `Self`"));
            }
            parameter.clone()
        } else {
            default_trait_self_parameter()
        };
        self.take_newlines_if_followed_by(&[TokenKind::Where, TokenKind::LBrace]);
        let where_predicates = self.where_clause()?;
        self.take_newlines_if_followed_by(&[TokenKind::LBrace]);
        self.expect(&TokenKind::LBrace, "`{` after `trait`")?;
        self.skip_separators();

        let mut member_effect_parameters = compile_groups
            .iter()
            .flatten()
            .filter(|parameter| parameter.kind == CompileParamKind::Effect)
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        if self_parameter.kind == CompileParamKind::Effect {
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
        let (compile_groups, groups) = self.declaration_groups(true, outer_effect_parameters)?;
        self.validate_receiver_groups(&groups)?;

        let logical_result = if self.take(&TokenKind::Colon) {
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
            Some(self.function_result_type()?)
        } else {
            None
        };
        let (effects, throws_error, _has_effect_group) = self.function_effect_clause()?;
        if throws_error.is_some() && logical_result.is_none() {
            return Err(self.error_here(
                "`Throws(Error)` requires an explicit logical return type before `with(...)`",
            ));
        }
        let return_type =
            logical_result.map(|result| Self::apply_throws_effect(result, throws_error));
        self.effect_parameters_in_scope.clear();

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
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here(
                    "trait default closure declarations require a braced body; write `= { expression }`",
                ));
            }
            Some(self.block()?)
        } else {
            None
        };

        Ok(TraitMember::Function(Function {
            name,
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
        let unsafe_effect = false;
        let throws_error: Option<Type> = None;
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
                    if !path
                        .last()
                        .is_some_and(|segment| effect_name_is_uppercase(segment))
                    {
                        return Err(
                            self.error_here("effect names in `with(...)` must end with an uppercase nominal segment")
                        );
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
                                        if matches!(kind.as_str(), "access" | "passing" | "effect")
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
                    "expected `Throws(Error)`, `Unsafe`, an effect parameter, or a custom effect name in `with(...)`",
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
                unsafe_effect,
                throws: throws_error.clone().map(Box::new),
                custom,
                parameters: effect_parameters,
            },
            throws_error,
            true,
        ))
    }

    fn apply_throws_effect(output: Type, throws_error: Option<Type>) -> Type {
        match throws_error {
            None => output,
            Some(error) => Type::Named("Result".to_owned(), vec![error, output]),
        }
    }

    fn function_result_type(&mut self) -> Result<Type, ParseError> {
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
        let value = self.expression(true)?;

        Ok(Binding {
            mutable,
            name,
            annotation,
            value,
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
                    passing: None,
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
        self.runtime_parameter_group(false, &HashSet::new())
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
                            if matches!(kind.as_str(), "access" | "passing" | "effect")
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
                "i32" => Type::I32,
                "i64" => Type::I64,
                "u32" => Type::U32,
                "u64" => Type::U64,
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
            if groups.len() == 1 && groups[0].is_empty() {
                return Ok(Type::Unit);
            }
            return Err(self.error_here("function types require `:` before the result type"));
        }
        let logical_result = self.function_result_type()?;
        let (effects, throws_error, _has_effect_clause) = self.function_effect_clause()?;
        let result = Self::apply_throws_effect(logical_result, throws_error);
        Ok(Type::Function {
            groups,
            effects,
            result: Box::new(result),
        })
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
                let member =
                    if self.at(&TokenKind::Super) && Self::is_super_path_expression(&expression) {
                        self.advance();
                        "super".to_owned()
                    } else {
                        self.expect_relative_path_segment("a member name after `.`")?
                    };
                expression = Expr::Member(Box::new(expression), member);
            } else if self.take(&TokenKind::QuestionDot) {
                let member = self.expect_ident("a member name after `?.`")?;
                expression = Expr::ChainMember(Box::new(expression), member);
            } else if self.struct_literal_follows(&expression) {
                let fields = self.struct_literal_fields()?;
                expression = Expr::StructLiteral {
                    constructor: Box::new(expression),
                    fields,
                };
                has_call_group = false;
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

    fn struct_literal_follows(&self, expression: &Expr) -> bool {
        self.at(&TokenKind::LBrace)
            && Self::expression_can_head_struct_literal(expression)
            && (self.at_offset(1, &TokenKind::RBrace)
                || matches!(
                    (
                        self.tokens.get(self.index + 1).map(|token| &token.kind),
                        self.tokens.get(self.index + 2).map(|token| &token.kind),
                    ),
                    (Some(TokenKind::Ident(_)), Some(TokenKind::Colon))
                ))
    }

    fn expression_can_head_struct_literal(expression: &Expr) -> bool {
        let root = Self::struct_literal_root(expression);
        root.chars().next().is_some_and(char::is_uppercase)
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
                Ok(Expr::DoBlock {
                    body: Box::new(self.block()?),
                })
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
            TokenKind::While => self.while_expression(),
            TokenKind::For => self.for_expression(),
            TokenKind::Loop => self.loop_expression(),
            TokenKind::Break => self.break_expression(allow_trailing_closure),
            TokenKind::Continue => {
                self.advance();
                Ok(Expr::Continue)
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
        if self.take(&TokenKind::Let) {
            let pattern = self.pattern()?;
            self.expect(&TokenKind::Equal, "`=` after the if-let pattern")?;
            let scrutinee = self.expression(false)?;
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here("expected `{` after `if let` scrutinee"));
            }
            let then_branch = self.block()?;
            let else_branch = self
                .optional_else_branch()?
                .map_or_else(|| Expr::Block(Vec::new(), None), |branch| *branch);
            return Ok(Expr::Match {
                scrutinee: Box::new(scrutinee),
                arms: vec![
                    MatchArm {
                        pattern,
                        guard: None,
                        body: then_branch,
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        guard: None,
                        body: else_branch,
                    },
                ],
            });
        }
        let condition = self.expression(false)?;
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("expected `{` after `if` condition"));
        }
        let then_branch = self.block()?;
        let else_branch = self.optional_else_branch()?;

        Ok(Expr::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch,
        })
    }

    fn optional_else_branch(&mut self) -> Result<Option<Box<Expr>>, ParseError> {
        // `else` may begin on the next logical line. If it is absent, restore
        // the newlines so the containing block can still see its separator.
        let before_newlines = self.index;
        while self.take(&TokenKind::Newline) {}
        if self.take(&TokenKind::Else) {
            if self.at(&TokenKind::If) {
                Ok(Some(Box::new(self.if_expression()?)))
            } else if self.at(&TokenKind::LBrace) {
                Ok(Some(Box::new(self.block()?)))
            } else {
                Err(self.error_here("expected `if` or `{` after `else`"))
            }
        } else {
            self.index = before_newlines;
            Ok(None)
        }
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

    fn while_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::While, "`while`")?;
        if self.take(&TokenKind::Let) {
            let pattern = self.pattern()?;
            self.expect(&TokenKind::Equal, "`=` after the while-let pattern")?;
            let scrutinee = self.expression(false)?;
            if !self.at(&TokenKind::LBrace) {
                return Err(self.error_here("expected `{` after `while let` scrutinee"));
            }
            let body = self.block()?;
            let loop_body = Expr::Match {
                scrutinee: Box::new(scrutinee),
                arms: vec![
                    MatchArm {
                        pattern,
                        guard: None,
                        body,
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        guard: None,
                        body: Expr::Break(None),
                    },
                ],
            };
            let id = self.next_control_binding;
            self.next_control_binding += 1;
            return Ok(Expr::Block(
                vec![Stmt::Let(Binding {
                    mutable: false,
                    name: format!("$while$let$result${id}"),
                    annotation: Some(Type::Unit),
                    value: Expr::Loop {
                        body: Box::new(loop_body),
                    },
                })],
                None,
            ));
        }
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

    fn for_expression(&mut self) -> Result<Expr, ParseError> {
        self.expect(&TokenKind::For, "`for`")?;
        let pattern = self.pattern()?;
        if !matches!(pattern, Pattern::Binding(_) | Pattern::Wildcard) {
            return Err(
                self.error_here("`for` currently requires an irrefutable name or `_` pattern")
            );
        }
        self.expect(&TokenKind::In, "`in` after the for pattern")?;
        let iterable = self.expression(false)?;
        if !self.at(&TokenKind::LBrace) {
            return Err(self.error_here("expected `{` after `for` iterable"));
        }
        let body = self.block()?;

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
                        path: vec!["Some".to_owned()],
                        fields: PatternFields::Positional(vec![pattern]),
                    },
                    guard: None,
                    body,
                },
                MatchArm {
                    pattern: Pattern::Constructor {
                        path: vec!["None".to_owned()],
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
                    mutable: true,
                    name: iterator,
                    annotation: None,
                    value: into_iter,
                }),
                Stmt::Let(Binding {
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

    fn core_control_function(name: &str) -> Expr {
        Expr::Member(
            Box::new(Expr::Member(
                Box::new(Expr::Name("core".to_owned())),
                "control".to_owned(),
            )),
            name.to_owned(),
        )
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

        if self.at(&TokenKind::Arrow) {
            return Err(
                self.error_here("zero-parameter closures do not use `->`; write `{ expression }`")
            );
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

fn normalize_and_validate_scopes(items: &mut [Item]) -> Result<(), String> {
    let empty = HashSet::new();
    for item in items {
        match item {
            Item::Function(function) => validate_function_scopes(function, &empty, &empty, &empty)?,
            Item::Global(binding) => validate_binding_scopes(binding, &empty, &empty)?,
            Item::TypeAlias(definition) => {
                let mut names = HashSet::new();
                for parameter in definition.compile_groups.iter().flatten() {
                    if !names.insert(parameter.name.clone()) {
                        return Err(format!(
                            "duplicate compile-time parameter `{}`",
                            parameter.name
                        ));
                    }
                }
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                normalize_type_region_qualifiers(&mut definition.target, &regions, &accesses)?;
                validate_type_regions(&definition.target, &regions)?;
                validate_type_accesses(&definition.target, &accesses)?;
            }
            Item::Effect(definition) => {
                reject_passing_parameters(
                    &definition.compile_groups,
                    &format!("effect `{}`", definition.name),
                )?;
                reject_effect_parameters(
                    &definition.compile_groups,
                    &format!("effect `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for operation in &mut definition.operations {
                    validate_function_scopes(operation, &regions, &accesses, &empty)?;
                }
            }
            Item::Domain(_) => {}
            Item::TypeForm(definition) => {
                let mut names = HashSet::new();
                for parameter in definition.compile_groups.iter().flatten() {
                    if !names.insert(parameter.name.clone()) {
                        return Err(format!(
                            "duplicate compile-time parameter `{}`",
                            parameter.name
                        ));
                    }
                }
                let _regions = declared_regions(&definition.compile_groups, &empty)?;
                let _accesses = declared_accesses(&definition.compile_groups, &empty)?;
            }
            Item::Struct(definition) => {
                reject_passing_parameters(
                    &definition.compile_groups,
                    &format!("struct `{}`", definition.name),
                )?;
                reject_effect_parameters(
                    &definition.compile_groups,
                    &format!("struct `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for field in &mut definition.fields {
                    normalize_type_region_qualifiers(&mut field.ty, &regions, &accesses)?;
                    validate_type_regions(&field.ty, &regions)?;
                    validate_type_accesses(&field.ty, &accesses)?;
                }
            }
            Item::Enum(definition) => {
                reject_passing_parameters(
                    &definition.compile_groups,
                    &format!("enum `{}`", definition.name),
                )?;
                reject_effect_parameters(
                    &definition.compile_groups,
                    &format!("enum `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for variant in &mut definition.variants {
                    match &mut variant.fields {
                        VariantFields::Unit => {}
                        VariantFields::Positional(types) => {
                            for ty in types {
                                normalize_type_region_qualifiers(ty, &regions, &accesses)?;
                                validate_type_regions(ty, &regions)?;
                                validate_type_accesses(ty, &accesses)?;
                            }
                        }
                        VariantFields::Named(fields) => {
                            for field in fields {
                                normalize_type_region_qualifiers(
                                    &mut field.ty,
                                    &regions,
                                    &accesses,
                                )?;
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
                reject_effect_parameters(
                    &definition.compile_groups,
                    &format!("trait `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                let mut effects = HashSet::new();
                if definition.self_parameter.kind == CompileParamKind::Effect {
                    effects.insert(definition.self_parameter.name.clone());
                }
                for member in &mut definition.members {
                    match member {
                        TraitMember::Function(function) => {
                            validate_function_scopes(function, &regions, &accesses, &effects)?
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
                            reject_effect_parameters(
                                compile_groups,
                                &format!("associated type `{}`", name),
                            )?;
                            let member_regions = declared_regions(compile_groups, &regions)?;
                            let member_accesses = declared_accesses(compile_groups, &accesses)?;
                            if let Some(default) = default {
                                normalize_type_region_qualifiers(
                                    default,
                                    &member_regions,
                                    &member_accesses,
                                )?;
                                validate_type_regions(default, &member_regions)?;
                                validate_type_accesses(default, &member_accesses)?;
                            }
                        }
                    }
                }
            }
            Item::Extend(extension) => {
                reject_passing_parameters(&extension.compile_groups, "extend header")?;
                reject_effect_parameters(&extension.compile_groups, "extend header")?;
                let regions = declared_regions(&extension.compile_groups, &empty)?;
                let accesses = declared_accesses(&extension.compile_groups, &empty)?;
                normalize_type_region_qualifiers(&mut extension.target, &regions, &accesses)?;
                validate_type_regions(&extension.target, &regions)?;
                validate_type_accesses(&extension.target, &accesses)?;
                if let Some(trait_ref) = &mut extension.trait_ref {
                    normalize_type_region_qualifiers(trait_ref, &regions, &accesses)?;
                    validate_type_regions(trait_ref, &regions)?;
                    validate_type_accesses(trait_ref, &accesses)?;
                }
                for predicate in &mut extension.where_predicates {
                    normalize_type_region_qualifiers(&mut predicate.subject, &regions, &accesses)?;
                    normalize_type_region_qualifiers(
                        &mut predicate.trait_ref,
                        &regions,
                        &accesses,
                    )?;
                    validate_type_regions(&predicate.subject, &regions)?;
                    validate_type_regions(&predicate.trait_ref, &regions)?;
                    for binding in &mut predicate.associated_types {
                        normalize_type_region_qualifiers(&mut binding.ty, &regions, &accesses)?;
                        validate_type_regions(&binding.ty, &regions)?;
                    }
                }
                for member in &mut extension.members {
                    match member {
                        ExtendMember::Function(function) => {
                            validate_function_scopes(function, &regions, &accesses, &empty)?
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

fn reject_effect_parameters(groups: &[Vec<CompileParam>], owner: &str) -> Result<(), String> {
    if groups
        .iter()
        .flatten()
        .any(|parameter| parameter.kind == CompileParamKind::Effect)
    {
        Err(format!(
            "{owner} cannot declare an `effect` parameter; effect parameters belong to functions"
        ))
    } else {
        Ok(())
    }
}

fn effect_name_is_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
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
            return Err(
                "region entity `'static` is predefined and cannot be redeclared".to_owned(),
            );
        }
        if !regions.insert(parameter.name.clone()) {
            return Err(format!("duplicate region parameter `{}`", parameter.name));
        }
    }
    Ok(regions)
}

fn validate_function_scopes(
    function: &mut Function,
    outer_regions: &HashSet<String>,
    outer_accesses: &HashSet<String>,
    outer_effects: &HashSet<String>,
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
    let mut effects = outer_effects.clone();
    for parameter in function
        .compile_groups
        .iter()
        .flatten()
        .filter(|parameter| parameter.kind == CompileParamKind::Effect)
    {
        if !effects.insert(parameter.name.clone()) {
            return Err(format!("duplicate effect parameter `{}`", parameter.name));
        }
    }
    let mut compile_names = HashSet::new();
    for parameter in function.compile_groups.iter().flatten() {
        if !compile_names.insert(parameter.name.clone()) {
            return Err(format!(
                "duplicate compile-time parameter `{}`",
                parameter.name
            ));
        }
    }
    for parameter in function.groups.iter_mut().flatten() {
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
        normalize_type_region_qualifiers(&mut parameter.ty, &regions, &accesses)?;
        validate_type_regions(&parameter.ty, &regions)?;
        validate_type_accesses(&parameter.ty, &accesses)?;
        validate_type_effects(&parameter.ty, &effects)?;
    }
    if let Some(return_type) = &mut function.return_type {
        normalize_type_region_qualifiers(return_type, &regions, &accesses)?;
        validate_type_regions(return_type, &regions)?;
        validate_type_accesses(return_type, &accesses)?;
        validate_type_effects(return_type, &effects)?;
    }
    for parameter in &function.effects.parameters {
        if !effects.contains(parameter) {
            return Err(format!("use of undeclared effect parameter `{parameter}`"));
        }
    }
    normalize_function_effect_region_qualifiers(&mut function.effects, &regions, &accesses)?;
    for effect in &function.effects.custom {
        validate_type_regions(effect, &regions)?;
        validate_type_accesses(effect, &accesses)?;
        validate_type_effects(effect, &effects)?;
    }
    for predicate in &mut function.where_predicates {
        normalize_type_region_qualifiers(&mut predicate.subject, &regions, &accesses)?;
        normalize_type_region_qualifiers(&mut predicate.trait_ref, &regions, &accesses)?;
        validate_type_regions(&predicate.subject, &regions)?;
        validate_type_regions(&predicate.trait_ref, &regions)?;
        validate_type_effects(&predicate.subject, &effects)?;
        validate_type_effects(&predicate.trait_ref, &effects)?;
        for binding in &mut predicate.associated_types {
            normalize_type_region_qualifiers(&mut binding.ty, &regions, &accesses)?;
            validate_type_regions(&binding.ty, &regions)?;
            validate_type_effects(&binding.ty, &effects)?;
        }
    }
    if let Some(body) = &mut function.body {
        normalize_expr_region_qualifiers(body, &regions, &accesses)?;
        validate_expr_regions(body, &regions)?;
        validate_expr_accesses(body, &accesses)?;
    }
    Ok(())
}

fn normalize_borrow_region_qualifier(
    access: &mut Option<String>,
    region: &mut Option<String>,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    let Some(name) = access.as_ref() else {
        return Ok(());
    };
    if region.is_none() && regions.contains(name) {
        if accesses.contains(name) {
            return Err(format!(
                "borrow qualifier `{name}` is ambiguous between access and region parameters"
            ));
        }
        *region = access.take();
    }
    Ok(())
}

fn normalize_type_region_qualifiers(
    ty: &mut Type,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    match ty {
        Type::Borrow {
            access,
            region,
            pointee,
            ..
        } => {
            normalize_borrow_region_qualifier(access, region, regions, accesses)?;
            normalize_type_region_qualifiers(pointee, regions, accesses)
        }
        Type::Array(element, _) => normalize_type_region_qualifiers(element, regions, accesses),
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter_mut().flatten() {
                normalize_type_region_qualifiers(ty, regions, accesses)?;
            }
            normalize_function_effect_region_qualifiers(effects, regions, accesses)?;
            normalize_type_region_qualifiers(result, regions, accesses)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                normalize_type_region_qualifiers(argument, regions, accesses)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                normalize_type_region_qualifiers(&mut argument.ty, regions, accesses)?;
            }
            Ok(())
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => Ok(()),
    }
}

fn normalize_function_effect_region_qualifiers(
    effects: &mut FunctionEffects,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    if let Some(error) = &mut effects.throws {
        normalize_type_region_qualifiers(error, regions, accesses)?;
    }
    for effect in &mut effects.custom {
        normalize_type_region_qualifiers(effect, regions, accesses)?;
    }
    Ok(())
}

fn normalize_expr_region_qualifiers(
    expression: &mut Expr,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    match expression {
        Expr::Type(ty) => normalize_type_region_qualifiers(ty, regions, accesses),
        Expr::Borrow { access, value, .. } => {
            if let Some(access) = access {
                if regions.contains(access) && !accesses.contains(access) {
                    return Err(format!(
                        "region parameter `{access}` cannot be used as a borrow expression access"
                    ));
                }
            }
            normalize_expr_region_qualifiers(value, regions, accesses)
        }
        Expr::Unary(_, value) | Expr::Try(value) | Expr::Throw(value) | Expr::Unsafe(value) => {
            normalize_expr_region_qualifiers(value, regions, accesses)
        }
        Expr::DoBlock { body } => normalize_expr_region_qualifiers(body, regions, accesses),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            normalize_expr_region_qualifiers(left, regions, accesses)?;
            normalize_expr_region_qualifiers(right, regions, accesses)
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            normalize_expr_region_qualifiers(scrutinee, regions, accesses)?;
            normalize_expr_region_qualifiers(success, regions, accesses)?;
            normalize_expr_region_qualifiers(fallback, regions, accesses)
        }
        Expr::HandlerChainCall(chain) => {
            normalize_expr_region_qualifiers(&mut chain.scrutinee, regions, accesses)?;
            for argument in chain.groups.iter_mut().flatten() {
                normalize_expr_region_qualifiers(&mut argument.value, regions, accesses)?;
            }
            normalize_expr_region_qualifiers(&mut chain.success, regions, accesses)?;
            normalize_expr_region_qualifiers(&mut chain.residual, regions, accesses)
        }
        Expr::Call(callee, arguments) => {
            normalize_expr_region_qualifiers(callee, regions, accesses)?;
            for argument in arguments {
                normalize_expr_region_qualifiers(&mut argument.value, regions, accesses)?;
            }
            Ok(())
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            normalize_expr_region_qualifiers(constructor, regions, accesses)?;
            for field in fields {
                normalize_expr_region_qualifiers(&mut field.value, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            normalize_expr_region_qualifiers(base, regions, accesses)
        }
        Expr::Array(elements) => {
            for element in elements {
                normalize_expr_region_qualifiers(element, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            normalize_expr_region_qualifiers(base, regions, accesses)?;
            normalize_expr_region_qualifiers(index, regions, accesses)
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &mut binding.annotation {
                            normalize_type_region_qualifiers(annotation, regions, accesses)?;
                        }
                        normalize_expr_region_qualifiers(&mut binding.value, regions, accesses)?;
                    }
                    Stmt::Expr(expression) => {
                        normalize_expr_region_qualifiers(expression, regions, accesses)?
                    }
                }
            }
            if let Some(tail) = tail {
                normalize_expr_region_qualifiers(tail, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                normalize_type_region_qualifiers(&mut parameter.ty, regions, accesses)?;
            }
            normalize_expr_region_qualifiers(body, regions, accesses)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            normalize_expr_region_qualifiers(condition, regions, accesses)?;
            normalize_expr_region_qualifiers(then_branch, regions, accesses)?;
            if let Some(else_branch) = else_branch {
                normalize_expr_region_qualifiers(else_branch, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                normalize_expr_region_qualifiers(value, regions, accesses)?;
            }
            Ok(())
        }
        Expr::While { condition, body } => {
            normalize_expr_region_qualifiers(condition, regions, accesses)?;
            normalize_expr_region_qualifiers(body, regions, accesses)
        }
        Expr::Loop { body } => normalize_expr_region_qualifiers(body, regions, accesses),
        Expr::Match { scrutinee, arms } => {
            normalize_expr_region_qualifiers(scrutinee, regions, accesses)?;
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    normalize_expr_region_qualifiers(guard, regions, accesses)?;
                }
                normalize_expr_region_qualifiers(&mut arm.body, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Name(_) | Expr::Continue => Ok(()),
    }
}

fn validate_type_effects(ty: &Type, effects: &HashSet<String>) -> Result<(), String> {
    match ty {
        Type::Named(name, _) if effects.contains(name) => Err(format!(
            "effect parameter `{name}` cannot be used as a runtime type"
        )),
        Type::NamedArgs(name, _) if effects.contains(name) => Err(format!(
            "effect parameter `{name}` cannot be used as a runtime type"
        )),
        Type::Borrow { pointee, .. } | Type::Array(pointee, _) => {
            validate_type_effects(pointee, effects)
        }
        Type::Function {
            groups,
            effects: function_effects,
            result,
        } => {
            for parameter in &function_effects.parameters {
                if !effects.contains(parameter) {
                    return Err(format!("use of undeclared effect parameter `{parameter}`"));
                }
            }
            for ty in groups.iter().flatten() {
                validate_type_effects(ty, effects)?;
            }
            if let Some(error) = &function_effects.throws {
                validate_type_effects(error, effects)?;
            }
            validate_type_effects(result, effects)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_effects(argument, effects)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                validate_type_effects(&argument.ty, effects)?;
            }
            Ok(())
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => Ok(()),
    }
}

fn validate_access_name(access: &str, accesses: &HashSet<String>) -> Result<(), String> {
    if accesses.contains(access) {
        Ok(())
    } else {
        Err(format!(
            "use of undeclared access or region parameter `{access}`"
        ))
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
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter().flatten() {
                validate_type_accesses(ty, accesses)?;
            }
            if let Some(error) = &effects.throws {
                validate_type_accesses(error, accesses)?;
            }
            validate_type_accesses(result, accesses)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_accesses(argument, accesses)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                validate_type_accesses(&argument.ty, accesses)?;
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
        Expr::DoBlock { body } => validate_expr_accesses(body, accesses),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            validate_expr_accesses(left, accesses)?;
            validate_expr_accesses(right, accesses)
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            validate_expr_accesses(scrutinee, accesses)?;
            validate_expr_accesses(success, accesses)?;
            validate_expr_accesses(fallback, accesses)
        }
        Expr::HandlerChainCall(chain) => {
            validate_expr_accesses(&chain.scrutinee, accesses)?;
            for argument in chain.groups.iter().flatten() {
                validate_expr_accesses(&argument.value, accesses)?;
            }
            validate_expr_accesses(&chain.success, accesses)?;
            validate_expr_accesses(&chain.residual, accesses)
        }
        Expr::Call(callee, arguments) => {
            validate_expr_accesses(callee, accesses)?;
            for argument in arguments {
                validate_expr_accesses(&argument.value, accesses)?;
            }
            Ok(())
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            validate_expr_accesses(constructor, accesses)?;
            for field in fields {
                validate_expr_accesses(&field.value, accesses)?;
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
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => Ok(()),
    }
}

fn validate_binding_scopes(
    binding: &mut Binding,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    if let Some(annotation) = &mut binding.annotation {
        normalize_type_region_qualifiers(annotation, regions, accesses)?;
        validate_type_regions(annotation, regions)?;
        validate_type_accesses(annotation, accesses)?;
    }
    normalize_expr_region_qualifiers(&mut binding.value, regions, accesses)?;
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
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter().flatten() {
                validate_type_regions(ty, regions)?;
            }
            if let Some(error) = &effects.throws {
                validate_type_regions(error, regions)?;
            }
            validate_type_regions(result, regions)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_regions(argument, regions)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                validate_type_regions(&argument.ty, regions)?;
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
        Err(format!(
            "use of undeclared region {}",
            display_region_name(region)
        ))
    }
}

fn display_region_name(region: &str) -> String {
    if region.chars().next().is_some_and(char::is_uppercase) {
        format!("`{region}`")
    } else {
        format!("`'{region}`")
    }
}

fn validate_expr_regions(expression: &Expr, regions: &HashSet<String>) -> Result<(), String> {
    match expression {
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => Ok(()),
        Expr::Unary(_, value)
        | Expr::Try(value)
        | Expr::Throw(value)
        | Expr::Unsafe(value)
        | Expr::Borrow { value, .. } => validate_expr_regions(value, regions),
        Expr::DoBlock { body } => validate_expr_regions(body, regions),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            validate_expr_regions(left, regions)?;
            validate_expr_regions(right, regions)
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            validate_expr_regions(scrutinee, regions)?;
            validate_expr_regions(success, regions)?;
            validate_expr_regions(fallback, regions)
        }
        Expr::HandlerChainCall(chain) => {
            validate_expr_regions(&chain.scrutinee, regions)?;
            for argument in chain.groups.iter().flatten() {
                validate_expr_regions(&argument.value, regions)?;
            }
            validate_expr_regions(&chain.success, regions)?;
            validate_expr_regions(&chain.residual, regions)
        }
        Expr::Call(callee, arguments) => {
            validate_expr_regions(callee, regions)?;
            for argument in arguments {
                validate_expr_regions(&argument.value, regions)?;
            }
            Ok(())
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            validate_expr_regions(constructor, regions)?;
            for field in fields {
                validate_expr_regions(&field.value, regions)?;
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
mod tests {
    use super::*;

    fn function_tail(function: &Function) -> &Expr {
        let Some(Expr::Block(_, Some(tail))) = &function.body else {
            panic!("expected function body block with a tail value");
        };
        tail
    }

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
    fn parses_function_effects_and_rejects_them_on_values() {
        let program =
            parse("let read(pointer: Ptr(i32)): i32 with(Unsafe) = { *pointer }\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(!function.effects.unsafe_effect);
        assert_eq!(
            function.effects.custom,
            vec![Type::Named("Unsafe".to_owned(), Vec::new())]
        );

        let error = parse("let answer(unsafe): i32 = { 42 }\n").unwrap_err();
        assert!(
            error.message.contains("expected a parameter name"),
            "{}",
            error.message
        );
        assert!(!error.message.contains("was removed"));

        let error = parse("let answer(try): i32 = { 42 }\n").unwrap_err();
        assert!(
            error.message.contains("expected a parameter name"),
            "{}",
            error.message
        );
        assert!(!error.message.contains("was removed"));

        let error = parse("let f(): i32 ! unsafe = { 42 }\n").unwrap_err();
        assert!(error.message.contains("expected a newline or `;`"));

        let program =
            parse("let fallible(): i32 with(Throws(bool), Unsafe) = { throw(true) }\n").unwrap();
        let Item::Function(fallible) = &program.items[0] else {
            panic!("expected fallible function");
        };
        assert_eq!(fallible.return_type, Some(Type::I32));
        assert!(!fallible.effects.unsafe_effect);
        assert_eq!(fallible.effects.throws, None);
        assert_eq!(
            fallible.effects.custom,
            vec![
                Type::Named("Throws".to_owned(), vec![Type::Bool]),
                Type::Named("Unsafe".to_owned(), Vec::new())
            ]
        );

        for source in [
            "let f(): i32 with(Unsafe, Unsafe) = { 0 }\n",
            "let f(): i32 with(Throws(bool), Throws(bool)) = { 0 }\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(error.message.contains("duplicate"));
        }

        for source in [
            "let f(): i32 with(unsafe) = { 0 }\n",
            "let f(): i32 with(try(bool)) = { 0 }\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(error
                .message
                .contains("expected `Throws(Error)`, `Unsafe`, an effect parameter"));
            assert!(!error.message.contains("was removed"));
        }
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
            "let bad(): foo.root.Value = { 0 }\n",
            "let bad(): i32 = { root.super.value }\n",
            "let bad(value: root.Option): i32 = { value match { root.super.Option.None => 0 } }\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(error.message.contains("first path segment"), "{error:?}");
        }

        parse(
            "let ok(value: super.super.model.Value): i32 = { super.super.api.read(root.self.value) }\n",
        )
        .unwrap();
    }

    #[test]
    fn accepts_root_super_and_contextual_self_in_ordinary_paths() {
        let program = parse(
            "let resolve(value: root.model.Value): super.model.Result = { root.api.call(super.value) }\n\
             let unwrap(value: root.Option): i32 = { value match { root.Option.Some(self) => self } }\n",
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
            function_tail(resolve),
            Expr::Call(callee, arguments)
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
        let Expr::Match { arms, .. } = function_tail(unwrap) else {
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
            parse("let convert(value: net.http.Point): net.http.Result(core.Status) = { value }\n")
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
    fn separates_compile_time_and_runtime_parameter_groups() {
        let program = parse(
            "let identity(T: type)(value: T): T = { value }\n\
             let staged(T: type)(U: type)(value: T): U = { value }\n",
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
    fn void_is_not_a_unit_type_alias() {
        let program = parse("let invalid(value: void): () = { () }\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            function.groups[0][0].ty,
            Type::Named("void".to_owned(), Vec::new())
        );
    }

    #[test]
    fn preserves_multiple_compile_parameters_in_one_group() {
        let program = parse("let choose(T: type, U: type)(value: T): U = { value }\n").unwrap();
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
            "let Cell(T: type) = struct { value: T }\n\
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
               let f(self: borrow(Self))(x: i32): i32;\n\
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
        assert_eq!(function.groups[0][0].mode, PassMode::Inferred);
        assert_eq!(
            function.groups[0][0].ty,
            Type::Borrow {
                mutable: false,
                access: None,
                region: None,
                pointee: Box::new(Type::Named("Self".into(), Vec::new())),
            }
        );
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
               let convert(U: type)(self: borrow(Self))(value: U): T = { value }\n\
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
        assert_eq!(function_tail(function), &Expr::Name("value".into()));

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
            "let value: Cell(_) = Cell(i32) { value: 20 }\n",
            "let value = Cell(_) { value: 20 }\n",
            "let value = Cell(T: _) { value: 20 }\n",
            "let value = Cell(Cell(_)) { value: Cell(i32) { value: 20 } }\n",
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
            "let main(): i32 = {\n  let mut value = 41\n  let pointer = MutPtr(borrow(mut)(value))\n  unsafe {\n    *pointer = *pointer + 1\n  }\n  value\n}\n",
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
                if matches!(body.as_ref(), Expr::DoBlock { body } if matches!(
                    body.as_ref(), Expr::Block(_, Some(tail)) if matches!(
                        tail.as_ref(),
                        Expr::Assign(left, _)
                            if matches!(left.as_ref(), Expr::Unary(UnaryOp::Deref, _))
                    )
                ))
        ));

        let error = parse("let main(): () = { unsafe do {} }\n").unwrap_err();
        assert!(error.message.contains("trailing closure after `unsafe`"));
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
            "let cell = Cell(i32) { value: 42 }\n\
             let nested = Cell(Cell(i32)) { value: 42 }\n\
             let some = Maybe(i32).Some(42)\n\
             let none = Maybe(i32).None\n",
        )
        .unwrap();

        let Item::Global(cell) = &program.items[0] else {
            panic!("expected cell binding");
        };
        assert_eq!(
            cell.value,
            Expr::StructLiteral {
                constructor: Box::new(type_head("Cell", Expr::Name("i32".into()))),
                fields: vec![argument(Some("value"), Expr::Integer(42))],
            }
        );

        let Item::Global(nested) = &program.items[1] else {
            panic!("expected nested cell binding");
        };
        assert_eq!(
            nested.value,
            Expr::StructLiteral {
                constructor: Box::new(type_head(
                    "Cell",
                    type_head("Cell", Expr::Name("i32".into())),
                )),
                fields: vec![argument(Some("value"), Expr::Integer(42))],
            }
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
                "let bad(value: i32)(T: type): i32 = { value }\n",
                "must precede runtime",
            ),
            (
                "let bad(T: type, value: T): T = { value }\n",
                "cannot be mixed",
            ),
            (
                "let bad(value: T, U: type): T = { value }\n",
                "cannot be mixed",
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
    fn rejects_reserved_compile_parameter_names() {
        for name in ["_", "i32", "i64", "u32", "u64", "bool", "Never"] {
            let source = format!("let invalid({name}: type)(value: i32): i32 = {{ value }}\n");
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
        let data = parse("let Bad(T: type)(value: T) = struct { value: T }\n").unwrap_err();
        assert!(data.message.contains("runtime parameters"));

        let extension = parse("extend(value: i32) Cell(i32) {}\n").unwrap_err();
        assert!(extension.message.contains("only compile-time parameters"));
    }

    #[test]
    fn keeps_call_groups_nested() {
        let program = parse("let main(): i32 = { add(1)(2) }\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Call(inner, second) = function_tail(function) else {
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
               = { x + y }\n",
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
    fn parses_arithmetic_compound_assignments() {
        let program = parse(
            "let main(): () = {\n\
               let mut value = 1\n\
               value += 2\n\
               value -= 3\n\
               value *= 4\n\
               value /= 5\n\
               value %= 6\n\
               value &= 7\n\
               value |= 8\n\
               value ^= 9\n\
               value <<= 1\n\
               value >>= 1\n\
               ()\n\
             }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Block(statements, Some(tail)) = function.body.as_ref().unwrap() else {
            panic!("expected block");
        };
        assert_eq!(tail.as_ref(), &Expr::Unit);
        for (statement, operator) in statements[1..].iter().zip([
            BinaryOp::Add,
            BinaryOp::Sub,
            BinaryOp::Mul,
            BinaryOp::Div,
            BinaryOp::Rem,
            BinaryOp::BitAnd,
            BinaryOp::BitOr,
            BinaryOp::BitXor,
            BinaryOp::Shl,
            BinaryOp::Shr,
        ]) {
            assert!(matches!(
                statement,
                Stmt::Expr(Expr::CompoundAssign(_, found, _)) if *found == operator
            ));
        }
    }

    #[test]
    fn parses_do_if_else_and_return() {
        let program = parse(
            "let choose(flag: bool): i32 = { do {\n\
               if flag { return 1 }\n\
               else { 2 }\n\
             } }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(matches!(function_tail(function), Expr::DoBlock { .. }));
    }

    #[test]
    fn desugars_if_let_to_a_match_with_fallback() {
        let program = parse(
            "let choose(value: Option(i32)): i32 = {\n\
               if let Some(found) = value { found } else { 0 }\n\
             }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Match { arms, .. } = function_tail(function) else {
            panic!("expected if-let to desugar to match");
        };
        assert!(matches!(
            &arms[0].pattern,
            Pattern::Constructor { path, .. } if path == &["Some"]
        ));
        assert_eq!(arms[1].pattern, Pattern::Wildcard);
    }

    #[test]
    fn parses_throw_as_a_core_control_function() {
        let program = parse("let fail(): Result(bool)(i32) = { throw(false) }\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            function_tail(function),
            &Expr::Call(
                Box::new(Expr::Member(
                    Box::new(Expr::Member(
                        Box::new(Expr::Name("core".to_owned())),
                        "control".to_owned(),
                    )),
                    "throw".to_owned(),
                )),
                vec![CallArg {
                    label: None,
                    value: Expr::Bool(false),
                }],
            )
        );

        let error = parse("let fail(): Result(bool)(i32) = { throw false }\n").unwrap_err();
        assert!(error.message.contains("`throw` is a function"));
    }

    #[test]
    fn parses_do_and_try_as_distinct_immediate_handlers() {
        let program = parse(
            "let main(): Result(bool)(i32) = { try { 42 } }\n\
             let other(): i32 = { do { 42 } }\n",
        )
        .unwrap();
        let Item::Function(main) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(matches!(function_tail(main), Expr::Try(_)));
        let Item::Function(other) = &program.items[1] else {
            panic!("expected function");
        };
        assert!(matches!(function_tail(other), Expr::DoBlock { .. }));

        let old =
            parse("let unwrap(value: Result(bool)(i32)): i32 with(Throws(bool)) = { value.try }\n")
                .unwrap_err();
        assert!(old.message.contains("expected a member name after `.`"));

        let malformed = parse("let value: Result(bool)(i32) = try do { 42 }\n").unwrap_err();
        assert!(malformed.message.contains("expected a block after `try`"));
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
    fn every_brace_expression_is_a_closure() {
        let program = parse(
            "let answer = { 42 }\n\
             let parenthesized = { (40 + 2) }\n\
             let successor = { (value: i32) -> value + 1 }\n",
        )
        .unwrap();

        for item in &program.items[..2] {
            let Item::Global(binding) = item else {
                panic!("expected closure-valued global");
            };
            assert!(matches!(
                binding.value,
                Expr::Closure(ref parameters, _) if parameters.is_empty()
            ));
        }
        let Item::Global(successor) = &program.items[2] else {
            panic!("expected parameterized closure");
        };
        assert!(matches!(
            successor.value,
            Expr::Closure(ref parameters, _) if parameters.len() == 1
        ));

        let removed = parse("let old = { -> 42 }\n").unwrap_err();
        assert!(removed.message.contains("do not use `->`"));
    }

    #[test]
    fn named_closure_declarations_require_braced_bodies() {
        for source in [
            "let answer(): i32 = 42\n",
            "extend Cell { let read(self: borrow(Self))(): i32 = self.value }\n",
            "let Read = trait { let read(self: borrow(Self))(): i32 = 42 }\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(error.message.contains("require a braced body"), "{error:?}");
        }

        parse("let answer = 42\nlet read(): i32 = { 42 }\n").unwrap();
    }

    #[test]
    fn parses_structs_and_enum_field_shapes() {
        let program = parse(
            "let Point = struct { x: i32, pub(package) y: i32, pub z: i32 }\n\
             let Documented = struct {\n\
               /// A field with a documentation comment.\n\
               value: i32,\n\
             }\n\
             let Shape = enum {\n\
               Circle(pub radius: i32, pub(package) center: Point, label: i32),\n\
               Record(\n\
                 /// A named variant field with a documentation comment.\n\
                 item: i32,\n\
               ),\n\
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

        let Item::Struct(documented) = &program.items[1] else {
            panic!("expected documented struct");
        };
        assert_eq!(documented.fields.len(), 1);
        assert_eq!(documented.fields[0].name, "value");

        let Item::Enum(shape) = &program.items[2] else {
            panic!("expected enum");
        };
        assert_eq!(shape.variants.len(), 4);
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
            VariantFields::Named(fields) if fields.len() == 1 && fields[0].name == "item"
        ));
        assert!(matches!(
            &shape.variants[2].fields,
            VariantFields::Positional(types) if types == &vec![Type::I32, Type::I32]
        ));
        assert_eq!(shape.variants[3].fields, VariantFields::Unit);
    }

    #[test]
    fn parses_labeled_construction_member_access_and_assignment() {
        let program = parse(
            "let Point = struct { x: i32, y: i32 }\n\
             let main(): i32 = {\n\
               let mut point = Point { x: 1, y: 2 }\n\
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
            Expr::StructLiteral { fields, .. }
                if fields.iter().map(|argument| argument.label.as_deref()).collect::<Vec<_>>()
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
            "let classify(shape: Shape): i32 = { shape match {\n\
               Shape.Circle(radius: value) if value > 0 => value,\n\
               Shape.Unit => 0,\n\
               _ => -1,\n\
             } }\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Match { scrutinee, arms } = function_tail(function) else {
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
        let error = parse("let value = call(x: 1, 2)\n").unwrap_err();
        assert!(error.message.contains("cannot be mixed"));
    }

    #[test]
    fn parses_shared_and_mutable_borrow_places() {
        let program = parse(
            "let main(): () = {\n\
               let shared = borrow(value.field)\n\
               let exclusive = borrow(mut)(value)\n\
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
        let error = parse("let invalid = borrow(make())\n").unwrap_err();
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
               let item = borrow(values[0])\n\
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
               let shared: borrow(i32) = borrow(value)\n\
               let mutable: borrow(mut)(i32) = borrow(mut)(value)\n\
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
            "let invalid(mut value: borrow(i32)): i32 = { value }\n",
            "let invalid(value: mut borrow(i32)): i32 = { value }\n",
            "let invalid(value: i32): borrow(i32) = { mut borrow(value) }\n",
        ] {
            let error = parse(source).unwrap_err();
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn parses_region_parameters_and_borrow_regions() {
        let program = parse(
            "let choose(R: region)(value: borrow(R)(i32)): borrow(R)(i32) = { borrow(value) }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(function.compile_groups[0][0].name, "R");
        assert_eq!(function.compile_groups[0][0].kind, CompileParamKind::Region);
        assert_eq!(
            function.groups[0][0].ty,
            Type::Borrow {
                mutable: false,
                access: None,
                region: Some("R".to_owned()),
                pointee: Box::new(Type::I32),
            }
        );
        assert_eq!(
            function.return_type,
            Some(Type::Borrow {
                mutable: false,
                access: None,
                region: Some("R".to_owned()),
                pointee: Box::new(Type::I32),
            })
        );
    }

    #[test]
    fn parses_access_parameters_in_borrow_modes_types_and_expressions() {
        let program = parse(
            "let identity(A: access, R: region, T: type)\n\
               (value: borrow(A)(R)(T)): borrow(A)(R)(T) = { borrow(A)(value) }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(function.compile_groups[0][0].kind, CompileParamKind::Access);
        assert_eq!(
            function.groups[0][0].ty,
            Type::Borrow {
                mutable: false,
                access: Some("A".to_owned()),
                region: Some("R".to_owned()),
                pointee: Box::new(Type::Named("T".into(), Vec::new())),
            }
        );
        assert!(matches!(
            function.return_type,
            Some(Type::Borrow {
                mutable: false,
                access: Some(ref access),
                region: Some(ref region),
                ..
            }) if access == "A" && region == "R"
        ));
        assert!(matches!(
            function_tail(function),
            Expr::Borrow {
                mutable: false,
                access: Some(ref access),
                ..
            } if access == "A"
        ));
    }

    #[test]
    fn parses_passing_parameters_in_keyword_position() {
        let program =
            parse("let identity(P: passing, T: type)(P value: T): T = { value }\n").unwrap();
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
    fn parses_effect_parameters_in_with_clauses() {
        let program = parse(
            "let tagged(E: effect)(value: i32): i32 with(E) = { value }\n\
             let combined(E: effect)(value: i32): i32 with(Unsafe, E) = { value }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(function.compile_groups[0][0].kind, CompileParamKind::Effect);
        assert_eq!(function.effects.parameters, vec!["E"]);
        let Item::Function(combined) = &program.items[1] else {
            panic!("expected function");
        };
        assert_eq!(
            combined.effects.custom,
            vec![Type::Named("Unsafe".to_owned(), Vec::new())]
        );
        assert_eq!(combined.effects.parameters, vec!["E"]);

        let error = parse("let Box(E: effect) = struct { value: i32 }\n").unwrap_err();
        assert!(error
            .message
            .contains("effect parameters belong to functions"));

        let error = parse("let bad(E: effect)(value: E): i32 with(E) = { 0 }\n").unwrap_err();
        assert!(error.message.contains("cannot be used as a runtime type"));

        let error = parse("let old(E: effect)(value: i32): i32(E) = { value }\n").unwrap_err();
        assert!(
            error
                .message
                .contains("effect parameter `E` cannot be used as a runtime type"),
            "{}",
            error.message
        );
        assert!(!error.message.contains("was removed"));
    }

    #[test]
    fn parses_trait_self_effect_parameter_in_member_rows() {
        let program = parse(
            "let Handle = trait(Self: effect) {\n\
               let Clauses(Value: type, Answer: type): type\n\
               let handle(Value: type, Answer: type, Rest: effect)(move clauses: Clauses(Value, Answer))(move action: (): Value with(Self, Rest)): Answer with(Rest)\n\
             }\n",
        )
        .unwrap();
        let Item::Trait(definition) = &program.items[0] else {
            panic!("expected trait");
        };
        let TraitMember::Function(function) = &definition.members[1] else {
            panic!("expected handle member");
        };
        assert_eq!(function.effects.parameters, vec!["Rest"]);
        let Type::Function { effects, .. } = &function.groups[1][0].ty else {
            panic!("expected action callable parameter");
        };
        assert_eq!(effects.parameters, vec!["Rest", "Self"]);
        assert!(effects.custom.is_empty());
    }

    #[test]
    fn parses_compiler_provided_domain_and_control_contract_declarations() {
        let program = parse(
            "pub let Unsafe = effect {}\n\
             pub let Throws(Error: type) = effect { let raise(move error: Error): Never }\n\
             pub let type = domain\n\
             pub let effect = domain\n\
             pub let access = domain {\n\
               /// Shared read-only access.\n\
               shared\n\
               /// Exclusive mutable access.\n\
               mut\n\
             }\n\
             pub let do(E: effect, T: type)(move action: (): T with(E)): T with(E)\n",
        )
        .unwrap();
        assert!(matches!(
            &program.items[0],
            Item::Effect(effect) if effect.compile_groups.is_empty()
        ));
        assert!(matches!(
            &program.items[1],
            Item::Effect(effect) if effect.compile_groups.len() == 1 && effect.operations.len() == 1
        ));
        assert!(matches!(
            &program.items[2],
            Item::Domain(domain) if domain.name == "type" && domain.members.is_none()
        ));
        assert!(matches!(
            &program.items[3],
            Item::Domain(domain) if domain.name == "effect" && domain.members.is_none()
        ));
        assert!(matches!(
            &program.items[4],
            Item::Domain(domain) if domain.name == "access"
                && domain.members.as_ref().is_some_and(|members| members.len() == 2
                    && members[0] == "shared"
                    && members[1] == "mut")
        ));
        assert!(matches!(
            &program.items[5],
            Item::Function(function) if function.name == "do" && function.body.is_none()
        ));
    }

    #[test]
    fn parses_nominal_marker_effect_declarations_and_callable_rows() {
        let program = parse(
            "pub let UI = effect\n\
             let render(): i32 with(UI) = { 0 }\n\
             let invoke(action: (): i32 with(UI)): i32 with(UI) = { action() }\n",
        )
        .unwrap();

        assert!(matches!(&program.items[0], Item::Effect(effect) if effect.name == "UI"));
        let Item::Function(render) = &program.items[1] else {
            panic!("expected render function");
        };
        assert_eq!(
            render.effects.custom,
            [Type::Named("UI".into(), Vec::new())]
        );
        let Item::Function(invoke) = &program.items[2] else {
            panic!("expected invoke function");
        };
        assert!(matches!(
            &invoke.groups[0][0].ty,
            Type::Function { effects, .. }
                if effects.custom == [Type::Named("UI".into(), Vec::new())]
        ));

        let duplicate = parse("let f(): i32 with(UI, UI) = { 0 }\n").unwrap_err();
        assert!(duplicate.message.contains("duplicate custom effect `UI`"));

        let lowercase_declaration = parse("let ui = effect\n").unwrap_err();
        assert!(lowercase_declaration
            .message
            .contains("uppercase nominal name"));

        let lowercase_use = parse("let f(): i32 with(core.effect.ui) = { 0 }\n").unwrap_err();
        assert!(lowercase_use.message.contains("uppercase nominal segment"));
    }

    #[test]
    fn parses_parameterized_algebraic_effect_operations() {
        let program = parse(
            "let State(S: type) = effect {\n\
               let get(): S\n\
               let put(move value: S): ()\n\
             }\n\
             let program(): i32 with(State(i32)) = { 0 }\n",
        )
        .unwrap();
        let Item::Effect(state) = &program.items[0] else {
            panic!("expected State effect");
        };
        assert_eq!(state.compile_groups[0][0].name, "S");
        assert_eq!(state.operations.len(), 2);
        assert_eq!(state.operations[0].name, "get");
        assert_eq!(
            state.operations[0].return_type,
            Some(Type::Named("S".into(), Vec::new()))
        );
        assert_eq!(state.operations[1].groups[0][0].mode, PassMode::Move);

        let Item::Function(program) = &program.items[1] else {
            panic!("expected program function");
        };
        assert_eq!(
            program.effects.custom,
            [Type::Named("State".into(), vec![Type::I32])]
        );
    }

    #[test]
    fn permits_effect_operation_overloads_only_by_parameter_names() {
        let program = parse(
            "let Ask = effect {\n\
               let value(left: i32): i32\n\
               let value(right: i32): i32\n\
             }\n",
        )
        .expect("distinct operation labels should form an overload set");
        let Item::Effect(ask) = &program.items[0] else {
            panic!("expected Ask effect");
        };
        assert_eq!(ask.operations.len(), 2);

        let duplicate = parse(
            "let Ask = effect {\n\
               let value(input: i32): i32\n\
               let value(input: i64): i64\n\
             }\n",
        )
        .expect_err("types must not participate in operation overload selection");
        assert!(duplicate.message.contains("same parameter names"));
    }

    #[test]
    fn parses_function_shaped_handlers_with_contextual_clause_parameters() {
        let program = parse(
            "let State(S: type) = effect { let get(): S }\n\
             let main(): i32 = {\n\
               State(i32).handle(get: { (resume) -> resume(42) }) {\n\
                 State(i32).get()\n\
               }\n\
             }\n",
        )
        .unwrap();
        let Item::Function(main) = &program.items[1] else {
            panic!("expected main function");
        };
        let Expr::Call(_, trailing) = function_tail(main) else {
            panic!("expected trailing handler call group");
        };
        assert_eq!(trailing.len(), 1);
    }

    #[test]
    fn parses_effects_as_part_of_callable_signatures() {
        let program = parse(
            "let apply(E: effect)(action: (i32): i32 with(E))(value: i32): i32 with(E) = { value }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(matches!(
            &function.groups[0][0].ty,
            Type::Function { groups, effects, result }
                if groups == &vec![vec![Type::I32]]
                    && effects.parameters == vec!["E"]
                    && result.as_ref() == &Type::I32
        ));

        let old =
            parse("let apply(E: effect)(action: (i32): i32(E))(value: i32): i32 = { value }\n")
                .unwrap_err();
        assert!(
            old.message
                .contains("effect parameter `E` cannot be used as a runtime type"),
            "{}",
            old.message
        );
        assert!(!old.message.contains("was removed"));
    }

    #[test]
    fn rejects_passing_parameters_on_data_declarations() {
        let error = parse("let Wrapper(P: passing) = struct { value: i32 }\n").unwrap_err();
        assert!(error
            .message
            .contains("passing parameters belong to functions"));
    }

    #[test]
    fn rejects_undeclared_access_parameters() {
        let error = parse("let invalid(value: borrow(A)(i32)): i32 = { value }\n").unwrap_err();
        assert!(error
            .message
            .contains("undeclared access or region parameter `A`"));
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
    fn desugars_while_let_to_a_unit_loop_match() {
        let program =
            parse("let main(): () = { while let Some(value) = next() { consume(value) } }\n")
                .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Block(_, Some(while_let)) = function.body.as_ref().unwrap() else {
            panic!("expected function block");
        };
        let Expr::Block(statements, None) = while_let.as_ref() else {
            panic!("expected desugared while-let block");
        };
        assert!(matches!(
            &statements[0],
            Stmt::Let(Binding {
                annotation: Some(Type::Unit),
                value: Expr::Loop { body },
                ..
            }) if matches!(body.as_ref(), Expr::Match { arms, .. } if arms.len() == 2)
        ));
    }

    #[test]
    fn parses_loop_with_break_value() {
        let program = parse("let main(): i32 = { loop {\n  break 40 + 2\n} }\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Loop { body } = function_tail(function) else {
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
    fn desugars_for_to_iteration_lang_item_calls() {
        let program =
            parse("let main(): () = { for value in values() { consume(value) } }\n").unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let Expr::Block(_, Some(for_loop)) = function.body.as_ref().unwrap() else {
            panic!("expected function block");
        };
        let Expr::Block(statements, None) = for_loop.as_ref() else {
            panic!("expected desugared for block");
        };
        assert!(matches!(
            &statements[0],
            Stmt::Let(Binding { mutable: true, value: Expr::Call(callee, _), .. })
                if matches!(callee.as_ref(), Expr::Member(_, member) if member == "$lang$into_iter")
        ));
        assert!(matches!(
            &statements[1],
            Stmt::Let(Binding {
                annotation: Some(Type::Unit),
                value: Expr::Loop { .. },
                ..
            })
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
        let error = parse("let main(values: Array(i32, -1)): i32 = { 0 }\n").unwrap_err();
        assert!(error.message.contains("non-negative decimal integer"));
    }

    #[test]
    fn parses_extend_methods_associated_functions_constants_and_trait_refs() {
        let program = parse(
            "let A = struct { value: i32 }\n\
             extend A: Foo {\n\
               let reset(self: borrow(mut)(Self))(): () = {}\n\
               let answer: i32 = 42\n\
               let make(value: i32): A = { A { value: value } }\n\
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
        assert_eq!(reset.groups[0][0].mode, PassMode::Inferred);
        assert_eq!(
            reset.groups[0][0].ty,
            Type::Borrow {
                mutable: true,
                access: None,
                region: None,
                pointee: Box::new(Type::Named("Self".into(), Vec::new())),
            }
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
               let convert(T: type)(self: borrow(Self))(value: T): T = { value }\n\
               let make(T: type)(value: T): T = { value }\n\
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
            "let Cell(T: type) = struct { value: T }\n\
             extend(T: type) Cell(T)\n\
             where T: Copy {\n\
               let get(self: borrow(Self))(): T = { self.value }\n}\n",
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
                   T: Marker(i32, Item = T), = { value }\n",
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
               let inspect(self: borrow(Self))(): i32 = { self.value }\n\
               let replace(move self)(value: i32)(other: i32): A = { A { value: value + other } }\n\
             }\n",
        )
        .unwrap();

        let Item::Extend(extension) = &program.items[0] else {
            panic!("expected extend declaration");
        };
        let ExtendMember::Function(inspect) = &extension.members[0] else {
            panic!("expected method");
        };
        assert_eq!(inspect.groups[0][0].mode, PassMode::Inferred);
        assert_eq!(
            inspect.groups[0][0].ty,
            Type::Borrow {
                mutable: false,
                access: None,
                region: None,
                pointee: Box::new(Type::Named("Self".into(), Vec::new())),
            }
        );
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

        let data = parse("extend A { let Nested = struct { value: i32 } }\n").unwrap_err();
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
    fn parses_type_families_and_type_constructor_aliases() {
        let program = parse(
            "let Family(T: type): type = Box(T)\n\
             let Constructor: (Element: type): type = Box\n\
             let Scalar: type = i32\n",
        )
        .unwrap();

        let Item::TypeAlias(family) = &program.items[0] else {
            panic!("expected type-family alias");
        };
        assert_eq!(family.compile_groups[0][0].name, "T");
        assert_eq!(
            family.target,
            Type::Named("Box".into(), vec![Type::Named("T".into(), Vec::new())])
        );

        let Item::TypeAlias(constructor) = &program.items[1] else {
            panic!("expected type-constructor alias");
        };
        assert_eq!(constructor.compile_groups[0][0].name, "Element");
        assert_eq!(
            constructor.target,
            Type::Named(
                "Box".into(),
                vec![Type::Named("Element".into(), Vec::new())]
            )
        );

        assert!(matches!(
            &program.items[2],
            Item::TypeAlias(alias) if alias.compile_groups.is_empty() && alias.target == Type::I32
        ));
    }

    #[test]
    fn parses_constructor_compile_parameter_kinds() {
        let program = parse(
            "let Use(F: (Element: type): type)(move value: F(i32)): F(i32) = { value }\n\
             let Effects(E: (Error: type): effect)(move action: (): i32 with(E(bool))): i32 with(E(bool)) = { action() }\n\
             let Functor = trait(Self: (Value: type): type) {\n\
               let map(E: effect, A: type, B: type)(move self: Self(A))(move transform: (A): B with(E)): Self(B) with(E)\n\
             }\n\
             let Applicative = trait(Self: (Value: type): type)\n\
             where Self: Functor {\n\
               let pure(A: type)(move value: A): Self(A)\n}\n",
        )
        .unwrap();

        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            function.compile_groups[0][0].kind,
            CompileParamKind::TypeConstructor { parameter_count: 1 }
        );
        assert_eq!(
            function.groups[0][0].ty,
            Type::Named("F".into(), vec![Type::I32])
        );

        let Item::Function(effects) = &program.items[1] else {
            panic!("expected effect-constructor function");
        };
        assert_eq!(
            effects.compile_groups[0][0].kind,
            CompileParamKind::EffectConstructor { parameter_count: 1 }
        );
        assert_eq!(
            effects.effects.custom,
            vec![Type::Named("E".into(), vec![Type::Bool])]
        );

        let Item::Trait(trait_def) = &program.items[2] else {
            panic!("expected trait");
        };
        assert_eq!(
            trait_def.self_parameter.kind,
            CompileParamKind::TypeConstructor { parameter_count: 1 }
        );
        assert!(trait_def.compile_groups.is_empty());

        let Item::Trait(applicative) = &program.items[3] else {
            panic!("expected inherited trait");
        };
        assert_eq!(applicative.where_predicates.len(), 1);
        assert_eq!(
            applicative.where_predicates[0].subject,
            Type::Named("Self".into(), Vec::new())
        );
        assert_eq!(
            applicative.where_predicates[0].trait_ref,
            Type::Named("Functor".into(), Vec::new())
        );
    }

    #[test]
    fn parses_labeled_type_arguments_without_reordering() {
        let program = parse(
            "let consume(value: Pair(V: bool, K: i32)): Result(E: bool)(T: i32) = { value }\n",
        )
        .unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert_eq!(
            function.groups[0][0].ty,
            Type::NamedArgs(
                "Pair".into(),
                vec![
                    TypeArg {
                        label: Some("V".into()),
                        ty: Type::Bool,
                    },
                    TypeArg {
                        label: Some("K".into()),
                        ty: Type::I32,
                    },
                ],
            )
        );
        assert_eq!(
            function.return_type,
            Some(Type::NamedArgs(
                "Result".into(),
                vec![
                    TypeArg {
                        label: Some("E".into()),
                        ty: Type::Bool,
                    },
                    TypeArg {
                        label: Some("T".into()),
                        ty: Type::I32,
                    },
                ],
            ))
        );
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
