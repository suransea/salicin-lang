//! Multi-file package module resolution.
//!
//! This first module layer deliberately has no imports. Source files provide
//! their module path out of band; declarations are collected package-wide and
//! then flattened to canonical names understood by the existing codegen.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{
    Binding, EnumDef, Expr, ExtendDef, ExtendMember, Field, Function, Item, MatchArm, Param,
    Pattern, PatternField, PatternFields, Program, Stmt, StructDef, TraitDef, TraitMember, Type,
    VariantFields, Visibility,
};
use crate::{lexer, parser};

/// One source file and the module assigned to it by package discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceUnit {
    /// User-facing source path used in diagnostics.
    pub path: String,
    /// Package-relative module path. The package root uses an empty path.
    pub module_path: Vec<String>,
    pub source: String,
    pub is_root: bool,
}

/// File-module path segments use a deliberately smaller identifier subset
/// than source declarations so paths are portable and always spellable.
pub fn is_valid_module_segment(segment: &str) -> bool {
    if segment == "_" || lexer::is_keyword(segment) {
        return false;
    }
    let mut bytes = segment.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

/// Parse, resolve, and flatten all source files in one package target.
///
/// Unknown names remain unchanged so the normal semantic analyzer can report
/// built-in, associated-member, and genuinely unresolved-name diagnostics.
pub fn resolve_sources(sources: &[SourceUnit]) -> Result<Program, Vec<String>> {
    let mut diagnostics = validate_source_layout(sources);
    let mut parsed = Vec::with_capacity(sources.len());

    for source in sources {
        match parser::parse(&source.source) {
            Ok(program) => parsed.push(ParsedUnit { source, program }),
            Err(error) => diagnostics.push(format!("{}: error: {error}", source.path)),
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let (symbols, module_paths, collection_diagnostics) = collect_symbols(&parsed);
    if !collection_diagnostics.is_empty() {
        return Err(collection_diagnostics);
    }

    let mut resolver = Resolver {
        symbols,
        module_paths,
        diagnostics: Vec::new(),
    };
    let mut items = Vec::new();
    let mut item_visibilities = Vec::new();

    for ParsedUnit { source, program } in parsed {
        let Program {
            items: unit_items,
            item_visibilities: unit_visibilities,
        } = program;
        debug_assert_eq!(unit_items.len(), unit_visibilities.len());

        let context = ResolveContext {
            source_path: &source.path,
            module_path: &source.module_path,
        };
        for (mut item, visibility) in unit_items.into_iter().zip(unit_visibilities) {
            resolver.rewrite_item(&mut item, context);
            items.push(item);
            item_visibilities.push(visibility);
        }
    }

    if resolver.diagnostics.is_empty() {
        Ok(Program::with_visibilities(items, item_visibilities))
    } else {
        Err(resolver.diagnostics)
    }
}

struct ParsedUnit<'a> {
    source: &'a SourceUnit,
    program: Program,
}

#[derive(Clone, Debug)]
struct Symbol {
    canonical: String,
    module_path: Vec<String>,
    visibility: Visibility,
    source_path: String,
}

type SymbolTable = HashMap<Vec<String>, Symbol>;

fn validate_source_layout(sources: &[SourceUnit]) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let roots: Vec<_> = sources.iter().filter(|source| source.is_root).collect();
    if roots.len() != 1 {
        diagnostics.push(format!(
            "<package>: error: package target must have exactly one root source, found {}",
            roots.len()
        ));
    }

    let mut modules: HashMap<Vec<String>, &str> = HashMap::new();
    for source in sources {
        if source.is_root && !source.module_path.is_empty() {
            diagnostics.push(format!(
                "{}: error: root source must use the empty module path",
                source.path
            ));
        }
        if !source.is_root && source.module_path.is_empty() {
            diagnostics.push(format!(
                "{}: error: non-root source must have a non-empty module path",
                source.path
            ));
        }
        for segment in &source.module_path {
            if !is_valid_module_segment(segment) {
                diagnostics.push(format!(
                    "{}: error: module path segment `{segment}` must be a non-reserved ASCII snake_case identifier",
                    source.path
                ));
            }
        }

        if let Some(previous) = modules.insert(source.module_path.clone(), &source.path) {
            diagnostics.push(format!(
                "{}: error: duplicate module `{}`; it is already provided by {previous}",
                source.path,
                display_module(&source.module_path)
            ));
        }
    }
    diagnostics
}

fn collect_symbols(parsed: &[ParsedUnit<'_>]) -> (SymbolTable, HashSet<Vec<String>>, Vec<String>) {
    let mut symbols: SymbolTable = HashMap::new();
    let mut module_paths = HashSet::new();
    let mut diagnostics = Vec::new();
    let mut module_children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();

    for unit in parsed {
        for length in 0..=unit.source.module_path.len() {
            module_paths.insert(unit.source.module_path[..length].to_vec());
        }
        for index in 0..unit.source.module_path.len() {
            module_children
                .entry(unit.source.module_path[..index].to_vec())
                .or_default()
                .insert(unit.source.module_path[index].clone());
        }
    }

    for unit in parsed {
        if unit.program.items.len() != unit.program.item_visibilities.len() {
            diagnostics.push(format!(
                "{}: error: internal error: parsed item visibility count does not match item count",
                unit.source.path
            ));
            continue;
        }

        for (item, visibility) in unit
            .program
            .items
            .iter()
            .zip(&unit.program.item_visibilities)
        {
            let Some(name) = declaration_name(item) else {
                continue;
            };
            let mut logical_path = unit.source.module_path.clone();
            logical_path.push(name.to_owned());
            let symbol = Symbol {
                canonical: canonical_name(&unit.source.module_path, name),
                module_path: unit.source.module_path.clone(),
                visibility: *visibility,
                source_path: unit.source.path.clone(),
            };

            if let Some(previous) = symbols.get(&logical_path) {
                diagnostics.push(format!(
                    "{}: error: duplicate declaration `{name}` in module `{}`; first declared in {}",
                    unit.source.path,
                    display_module(&unit.source.module_path),
                    previous.source_path
                ));
            } else {
                symbols.insert(logical_path, symbol);
            }

            if module_children
                .get(&unit.source.module_path)
                .is_some_and(|children| children.contains(name))
            {
                diagnostics.push(format!(
                    "{}: error: declaration `{name}` conflicts with child module `{}`",
                    unit.source.path,
                    canonical_name(&unit.source.module_path, name)
                ));
            }
        }
    }

    (symbols, module_paths, diagnostics)
}

fn declaration_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(function) => Some(&function.name),
        Item::Global(binding) => Some(&binding.name),
        Item::Struct(definition) => Some(&definition.name),
        Item::Enum(definition) => Some(&definition.name),
        Item::Trait(definition) => Some(&definition.name),
        Item::Extend(_) => None,
    }
}

fn canonical_name(module_path: &[String], name: &str) -> String {
    if module_path.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{name}", module_path.join("::"))
    }
}

fn display_module(module_path: &[String]) -> String {
    if module_path.is_empty() {
        "<root>".to_owned()
    } else {
        module_path.join("::")
    }
}

#[derive(Clone, Copy)]
struct ResolveContext<'a> {
    source_path: &'a str,
    module_path: &'a [String],
}

struct Resolver {
    symbols: SymbolTable,
    module_paths: HashSet<Vec<String>>,
    diagnostics: Vec<String>,
}

impl Resolver {
    fn rewrite_item(&mut self, item: &mut Item, context: ResolveContext<'_>) {
        match item {
            Item::Function(function) => {
                function.name = canonical_name(context.module_path, &function.name);
                self.rewrite_function(function, context, &HashSet::new());
            }
            Item::Global(binding) => {
                binding.name = canonical_name(context.module_path, &binding.name);
                self.rewrite_binding(binding, context, &HashSet::new(), &HashSet::new());
            }
            Item::Struct(definition) => self.rewrite_struct(definition, context),
            Item::Enum(definition) => self.rewrite_enum(definition, context),
            Item::Trait(definition) => self.rewrite_trait(definition, context),
            Item::Extend(extension) => self.rewrite_extend(extension, context),
        }
    }

    fn rewrite_struct(&mut self, definition: &mut StructDef, context: ResolveContext<'_>) {
        definition.name = canonical_name(context.module_path, &definition.name);
        let type_scope = compile_parameter_names(&definition.compile_groups, &HashSet::new());
        for field in &mut definition.fields {
            self.rewrite_field(field, context, &type_scope);
        }
    }

    fn rewrite_enum(&mut self, definition: &mut EnumDef, context: ResolveContext<'_>) {
        definition.name = canonical_name(context.module_path, &definition.name);
        let type_scope = compile_parameter_names(&definition.compile_groups, &HashSet::new());
        for variant in &mut definition.variants {
            match &mut variant.fields {
                VariantFields::Unit => {}
                VariantFields::Positional(types) => {
                    for ty in types {
                        self.rewrite_type(ty, context, &type_scope);
                    }
                }
                VariantFields::Named(fields) => {
                    for field in fields {
                        self.rewrite_field(field, context, &type_scope);
                    }
                }
            }
        }
    }

    fn rewrite_trait(&mut self, definition: &mut TraitDef, context: ResolveContext<'_>) {
        definition.name = canonical_name(context.module_path, &definition.name);
        let mut trait_types = compile_parameter_names(&definition.compile_groups, &HashSet::new());
        trait_types.insert("Self".to_owned());
        trait_types.extend(definition.members.iter().filter_map(|member| match member {
            TraitMember::AssociatedType { name, .. } => Some(name.clone()),
            TraitMember::Function(_) => None,
        }));
        for member in &mut definition.members {
            match member {
                TraitMember::Function(function) => {
                    self.rewrite_function(function, context, &trait_types);
                }
                TraitMember::AssociatedType {
                    compile_groups,
                    default,
                    ..
                } => {
                    let type_scope = compile_parameter_names(compile_groups, &trait_types);
                    if let Some(default) = default {
                        self.rewrite_type(default, context, &type_scope);
                    }
                }
            }
        }
    }

    fn rewrite_extend(&mut self, extension: &mut ExtendDef, context: ResolveContext<'_>) {
        let header_type_scope = HashSet::new();
        self.rewrite_type(&mut extension.target, context, &header_type_scope);
        if let Some(trait_ref) = &mut extension.trait_ref {
            self.rewrite_type(trait_ref, context, &header_type_scope);
        }

        let mut member_type_scope = HashSet::from(["Self".to_owned()]);
        if extension.trait_ref.is_some() {
            member_type_scope.extend(extension.members.iter().filter_map(|member| match member {
                ExtendMember::Const(binding) => Some(binding.name.clone()),
                ExtendMember::Function(_) => None,
            }));
        }
        for member in &mut extension.members {
            match member {
                ExtendMember::Function(function) => {
                    self.rewrite_function(function, context, &member_type_scope);
                }
                ExtendMember::Const(binding) => {
                    self.rewrite_binding(binding, context, &member_type_scope, &member_type_scope);
                }
            }
        }
    }

    fn rewrite_function(
        &mut self,
        function: &mut Function,
        context: ResolveContext<'_>,
        outer_types: &HashSet<String>,
    ) {
        let type_scope = compile_parameter_names(&function.compile_groups, outer_types);
        let mut value_scope = type_scope.clone();
        for group in &mut function.groups {
            for parameter in group {
                self.rewrite_parameter(parameter, context, &type_scope);
                value_scope.insert(parameter.name.clone());
            }
        }
        if let Some(return_type) = &mut function.return_type {
            self.rewrite_type(return_type, context, &type_scope);
        }
        if let Some(body) = &mut function.body {
            self.rewrite_expr(body, context, &type_scope, &value_scope);
        }
    }

    fn rewrite_parameter(
        &mut self,
        parameter: &mut Param,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
    ) {
        self.rewrite_type(&mut parameter.ty, context, type_scope);
    }

    fn rewrite_field(
        &mut self,
        field: &mut Field,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
    ) {
        self.rewrite_type(&mut field.ty, context, type_scope);
    }

    fn rewrite_binding(
        &mut self,
        binding: &mut Binding,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
        value_scope: &HashSet<String>,
    ) {
        if let Some(annotation) = &mut binding.annotation {
            self.rewrite_type(annotation, context, type_scope);
        }
        self.rewrite_expr(&mut binding.value, context, type_scope, value_scope);
    }

    fn rewrite_type(
        &mut self,
        ty: &mut Type,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
    ) {
        match ty {
            Type::Array(element, _) => self.rewrite_type(element, context, type_scope),
            Type::Named(name, arguments) => {
                for argument in arguments {
                    self.rewrite_type(argument, context, type_scope);
                }
                let segments: Vec<String> = name.split('.').map(str::to_owned).collect();
                if segments.len() == 1 && type_scope.contains(&segments[0]) {
                    return;
                }
                if let Some(canonical) = self.resolve_logical_path(&segments, context) {
                    *name = canonical;
                }
            }
            Type::I32
            | Type::I64
            | Type::U32
            | Type::U64
            | Type::Bool
            | Type::Unit
            | Type::Infer => {}
        }
    }

    fn rewrite_expr(
        &mut self,
        expression: &mut Expr,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
        value_scope: &HashSet<String>,
    ) {
        match expression {
            Expr::Name(name) => {
                if !value_scope.contains(name) {
                    let logical = vec![name.clone()];
                    if let Some(canonical) = self.resolve_logical_path(&logical, context) {
                        *name = canonical;
                    }
                }
            }
            Expr::Unary(_, operand)
            | Expr::Try(operand)
            | Expr::Throw(operand)
            | Expr::Borrow { value: operand, .. } => {
                self.rewrite_expr(operand, context, type_scope, value_scope);
            }
            Expr::Binary(left, _, right)
            | Expr::Coalesce(left, right)
            | Expr::Assign(left, right) => {
                self.rewrite_expr(left, context, type_scope, value_scope);
                self.rewrite_expr(right, context, type_scope, value_scope);
            }
            Expr::Call(callee, arguments) => {
                self.rewrite_expr(callee, context, type_scope, value_scope);
                for argument in arguments {
                    self.rewrite_expr(&mut argument.value, context, type_scope, value_scope);
                }
            }
            Expr::Member(_, _) => {
                self.rewrite_member_chain(expression, context, type_scope, value_scope);
            }
            Expr::ChainMember(base, _) => {
                self.rewrite_expr(base, context, type_scope, value_scope);
            }
            Expr::Array(elements) => {
                for element in elements {
                    self.rewrite_expr(element, context, type_scope, value_scope);
                }
            }
            Expr::Index { base, index } => {
                self.rewrite_expr(base, context, type_scope, value_scope);
                self.rewrite_expr(index, context, type_scope, value_scope);
            }
            Expr::Block(statements, tail) => {
                let mut block_scope = value_scope.clone();
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) => {
                            self.rewrite_binding(binding, context, type_scope, &block_scope);
                            block_scope.insert(binding.name.clone());
                        }
                        Stmt::Expr(expression) => {
                            self.rewrite_expr(expression, context, type_scope, &block_scope);
                        }
                    }
                }
                if let Some(tail) = tail {
                    self.rewrite_expr(tail, context, type_scope, &block_scope);
                }
            }
            Expr::Closure(parameters, body) => {
                let mut closure_scope = value_scope.clone();
                for parameter in parameters {
                    self.rewrite_parameter(parameter, context, type_scope);
                    closure_scope.insert(parameter.name.clone());
                }
                self.rewrite_expr(body, context, type_scope, &closure_scope);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.rewrite_expr(condition, context, type_scope, value_scope);
                self.rewrite_expr(then_branch, context, type_scope, value_scope);
                if let Some(else_branch) = else_branch {
                    self.rewrite_expr(else_branch, context, type_scope, value_scope);
                }
            }
            Expr::Return(value) | Expr::Break(value) => {
                if let Some(value) = value {
                    self.rewrite_expr(value, context, type_scope, value_scope);
                }
            }
            Expr::While { condition, body } => {
                self.rewrite_expr(condition, context, type_scope, value_scope);
                self.rewrite_expr(body, context, type_scope, value_scope);
            }
            Expr::Loop { body } => {
                self.rewrite_expr(body, context, type_scope, value_scope);
            }
            Expr::Match { scrutinee, arms } => {
                self.rewrite_expr(scrutinee, context, type_scope, value_scope);
                for arm in arms {
                    self.rewrite_match_arm(arm, context, type_scope, value_scope);
                }
            }
            Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Infer => {}
        }
    }

    fn rewrite_match_arm(
        &mut self,
        arm: &mut MatchArm,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
        value_scope: &HashSet<String>,
    ) {
        let mut bindings = HashSet::new();
        self.rewrite_pattern(&mut arm.pattern, context, value_scope, &mut bindings);
        let mut arm_scope = value_scope.clone();
        arm_scope.extend(bindings);
        if let Some(guard) = &mut arm.guard {
            self.rewrite_expr(guard, context, type_scope, &arm_scope);
        }
        self.rewrite_expr(&mut arm.body, context, type_scope, &arm_scope);
    }

    fn rewrite_pattern(
        &mut self,
        pattern: &mut Pattern,
        context: ResolveContext<'_>,
        value_scope: &HashSet<String>,
        bindings: &mut HashSet<String>,
    ) {
        match pattern {
            Pattern::Binding(name) => {
                bindings.insert(name.clone());
            }
            Pattern::Constructor { path, fields } => {
                if !path
                    .first()
                    .is_some_and(|first| value_scope.contains(first))
                {
                    if let Some((canonical, consumed)) = self.resolve_longest_prefix(path, context)
                    {
                        let mut resolved = vec![canonical];
                        resolved.extend(path[consumed..].iter().cloned());
                        *path = resolved;
                    }
                }
                match fields {
                    PatternFields::Unit => {}
                    PatternFields::Positional(patterns) => {
                        for pattern in patterns {
                            self.rewrite_pattern(pattern, context, value_scope, bindings);
                        }
                    }
                    PatternFields::Named(fields) => {
                        for PatternField { pattern, .. } in fields {
                            self.rewrite_pattern(pattern, context, value_scope, bindings);
                        }
                    }
                }
            }
            Pattern::Wildcard | Pattern::Integer(_) | Pattern::Bool(_) => {}
        }
    }

    fn rewrite_member_chain(
        &mut self,
        expression: &mut Expr,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
        value_scope: &HashSet<String>,
    ) {
        let mut segments = Vec::new();
        if collect_member_segments(expression, &mut segments)
            && !segments
                .first()
                .is_some_and(|first| value_scope.contains(first))
        {
            if let Some((canonical, consumed)) = self.resolve_longest_prefix(&segments, context) {
                let mut resolved = Expr::Name(canonical);
                for member in &segments[consumed..] {
                    resolved = Expr::Member(Box::new(resolved), member.clone());
                }
                *expression = resolved;
                return;
            }

            // Preserve the complete qualified spelling of an unknown name
            // under a known module. A plain Member tree would otherwise be
            // interpreted as a method receiver by the current analyzer and
            // lose its module prefix in the eventual diagnostic.
            if self.longest_module_prefix(&segments, context.module_path) > 0 {
                *expression = Expr::Name(segments.join("."));
                return;
            }
        }

        let Expr::Member(base, _) = expression else {
            unreachable!("member-chain rewriting is called only for member expressions")
        };
        self.rewrite_expr(base, context, type_scope, value_scope);
    }

    fn resolve_longest_prefix(
        &mut self,
        segments: &[String],
        context: ResolveContext<'_>,
    ) -> Option<(String, usize)> {
        for length in (1..=segments.len()).rev() {
            if let Some(canonical) = self.resolve_logical_path(&segments[..length], context) {
                return Some((canonical, length));
            }
        }
        None
    }

    fn resolve_logical_path(
        &mut self,
        logical_path: &[String],
        context: ResolveContext<'_>,
    ) -> Option<String> {
        let symbol = self
            .find_symbol(logical_path, context.module_path)
            .map(|symbol| {
                (
                    symbol.canonical.clone(),
                    symbol.module_path.clone(),
                    symbol.visibility,
                )
            })?;

        if symbol.2 == Visibility::Private && !context.module_path.starts_with(symbol.1.as_slice())
        {
            self.diagnostics.push(format!(
                "{}: error: `{}` is private to module `{}`",
                context.source_path,
                logical_path.join("."),
                display_module(&symbol.1)
            ));
        }
        Some(symbol.0)
    }

    fn find_symbol(&self, logical_path: &[String], module_path: &[String]) -> Option<&Symbol> {
        for depth in (0..=module_path.len()).rev() {
            let mut candidate = module_path[..depth].to_vec();
            candidate.extend_from_slice(logical_path);
            if let Some(symbol) = self.symbols.get(&candidate) {
                return Some(symbol);
            }
        }
        None
    }

    fn longest_module_prefix(&self, segments: &[String], module_path: &[String]) -> usize {
        for length in (1..=segments.len()).rev() {
            for depth in (0..=module_path.len()).rev() {
                let mut candidate = module_path[..depth].to_vec();
                candidate.extend_from_slice(&segments[..length]);
                if self.module_paths.contains(&candidate) {
                    return length;
                }
            }
        }
        0
    }
}

fn compile_parameter_names(
    groups: &[Vec<crate::ast::CompileParam>],
    outer: &HashSet<String>,
) -> HashSet<String> {
    let mut names = outer.clone();
    names.extend(
        groups
            .iter()
            .flatten()
            .map(|parameter| parameter.name.clone()),
    );
    names
}

fn collect_member_segments(expression: &Expr, segments: &mut Vec<String>) -> bool {
    match expression {
        Expr::Name(name) => {
            segments.push(name.clone());
            true
        }
        Expr::Member(base, member) if collect_member_segments(base, segments) => {
            segments.push(member.clone());
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(path: &str, module_path: &[&str], source: &str, is_root: bool) -> SourceUnit {
        SourceUnit {
            path: path.to_owned(),
            module_path: module_path
                .iter()
                .map(|segment| (*segment).to_owned())
                .collect(),
            source: source.to_owned(),
            is_root,
        }
    }

    fn function<'a>(program: &'a Program, name: &str) -> &'a Function {
        program
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(function) if function.name == name => Some(function),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing function `{name}`"))
    }

    #[test]
    fn flattens_modules_and_rewrites_calls_and_dotted_types() {
        let program = resolve_sources(&[
            unit(
                "src/main.sali",
                &[],
                "let main(): geometry.Point = geometry.make()\n",
                true,
            ),
            unit(
                "src/geometry.sali",
                &["geometry"],
                "pub(package) let Point = struct(x: i32, y: i32)\n\
                 pub(package) let make(): Point = Point(x: 1, y: 2)\n",
                false,
            ),
        ])
        .unwrap();

        assert!(program.items.iter().any(
            |item| matches!(item, Item::Struct(definition) if definition.name == "geometry::Point")
        ));
        let main = function(&program, "main");
        assert_eq!(
            main.return_type,
            Some(Type::Named("geometry::Point".into(), Vec::new()))
        );
        assert!(matches!(
            main.body,
            Some(Expr::Call(ref callee, ref arguments))
                if arguments.is_empty()
                    && callee.as_ref() == &Expr::Name("geometry::make".into())
        ));

        let make = function(&program, "geometry::make");
        assert_eq!(
            make.return_type,
            Some(Type::Named("geometry::Point".into(), Vec::new()))
        );
        assert!(matches!(
            make.body,
            Some(Expr::Call(ref callee, _))
                if callee.as_ref() == &Expr::Name("geometry::Point".into())
        ));
    }

    #[test]
    fn resolves_longest_declaration_prefix_and_preserves_fields() {
        let program = resolve_sources(&[
            unit(
                "src/main.sali",
                &[],
                "let main(): i32 = data.origin.x\n",
                true,
            ),
            unit(
                "src/data.sali",
                &["data"],
                "pub(package) let Point = struct(x: i32)\n\
                 pub(package) let origin = Point(x: 1)\n",
                false,
            ),
        ])
        .unwrap();

        assert!(matches!(
            function(&program, "main").body,
            Some(Expr::Member(ref base, ref field))
                if field == "x" && base.as_ref() == &Expr::Name("data::origin".into())
        ));
    }

    #[test]
    fn local_parameters_blocks_closures_and_match_bindings_shadow_modules() {
        let program = resolve_sources(&[
            unit(
                "src/main.sali",
                &[],
                "let keep(math: i32): i32 = {\n\
                   let local = { (math: i32) -> math }\n\
                   Option.Some(math) match { Option.Some(math) => local(math), _ => math }\n\
                 }\n",
                true,
            ),
            unit(
                "src/math.sali",
                &["math"],
                "pub(package) let value = 42\n",
                false,
            ),
        ])
        .unwrap();

        let Some(Expr::Block(statements, Some(tail))) = &function(&program, "keep").body else {
            panic!("expected block body");
        };
        let Stmt::Let(local) = &statements[0] else {
            panic!("expected local closure");
        };
        let Expr::Closure(_, closure_body) = &local.value else {
            panic!("expected closure");
        };
        assert!(matches!(
            closure_body.as_ref(),
            Expr::Block(_, Some(value)) if value.as_ref() == &Expr::Name("math".into())
        ));
        let Expr::Match { arms, .. } = tail.as_ref() else {
            panic!("expected match");
        };
        assert!(matches!(
            &arms[0].body,
            Expr::Call(_, arguments)
                if arguments[0].value == Expr::Name("math".into())
        ));
    }

    #[test]
    fn reports_private_sibling_access_but_allows_descendants() {
        let error = resolve_sources(&[
            unit("src/main.sali", &[], "let main(): i32 = b.read()\n", true),
            unit("src/a.sali", &["a"], "let secret(): i32 = 1\n", false),
            unit(
                "src/a/child.sali",
                &["a", "child"],
                "pub(package) let read(): i32 = secret()\n",
                false,
            ),
            unit(
                "src/b.sali",
                &["b"],
                "pub(package) let read(): i32 = a.secret()\n",
                false,
            ),
        ])
        .unwrap_err();

        assert_eq!(error.len(), 1, "{error:?}");
        assert!(error[0].contains("private to module `a`"), "{error:?}");
    }

    #[test]
    fn preserves_self_and_associated_types_inside_traits_and_extensions() {
        let program = resolve_sources(&[
            unit("src/main.sali", &[], "let main(): i32 = 0\n", true),
            unit(
                "src/api.sali",
                &["api"],
                "pub(package) let Self = struct(value: i32)\n\
                 pub(package) let Output = struct(value: i32)\n\
                 pub(package) let A = struct(value: i32)\n\
                 pub(package) let Convert = trait {\n\
                   let Output: type\n\
                   let A: type\n\
                   let B: type\n\
                   let convert(borrow self)(value: Self): Output\n\
                 }\n\
                 pub(package) let Number = struct(value: i32)\n\
                 extend Number: Convert {\n\
                   let Output = i32\n\
                   let A = Self\n\
                   let B = A\n\
                   let convert(borrow self)(value: Self): Output = value.value\n\
                 }\n",
                false,
            ),
        ])
        .unwrap();

        let trait_definition = program
            .items
            .iter()
            .find_map(|item| match item {
                Item::Trait(definition) if definition.name == "api::Convert" => Some(definition),
                _ => None,
            })
            .expect("missing resolved trait");
        let trait_method = trait_definition
            .members
            .iter()
            .find_map(|member| match member {
                TraitMember::Function(function) => Some(function),
                TraitMember::AssociatedType { .. } => None,
            })
            .expect("missing trait method");
        assert_eq!(
            trait_method.groups[0][0].ty,
            Type::Named("Self".into(), Vec::new())
        );
        assert_eq!(
            trait_method.groups[1][0].ty,
            Type::Named("Self".into(), Vec::new())
        );
        assert_eq!(
            trait_method.return_type,
            Some(Type::Named("Output".into(), Vec::new()))
        );

        let extension = program
            .items
            .iter()
            .find_map(|item| match item {
                Item::Extend(extension) => Some(extension),
                _ => None,
            })
            .expect("missing resolved extension");
        let implementation = extension
            .members
            .iter()
            .find_map(|member| match member {
                ExtendMember::Function(function) => Some(function),
                ExtendMember::Const(_) => None,
            })
            .expect("missing implementation method");
        assert_eq!(
            implementation.groups[0][0].ty,
            Type::Named("Self".into(), Vec::new())
        );
        assert_eq!(
            implementation.groups[1][0].ty,
            Type::Named("Self".into(), Vec::new())
        );
        assert_eq!(
            implementation.return_type,
            Some(Type::Named("Output".into(), Vec::new()))
        );
        let associated_values = extension
            .members
            .iter()
            .filter_map(|member| match member {
                ExtendMember::Const(binding) => {
                    Some((binding.name.as_str(), binding.value.clone()))
                }
                ExtendMember::Function(_) => None,
            })
            .collect::<HashMap<_, _>>();
        assert_eq!(associated_values["A"], Expr::Name("Self".into()));
        assert_eq!(associated_values["B"], Expr::Name("A".into()));
    }

    #[test]
    fn leaves_unknown_names_for_semantic_analysis() {
        let program = resolve_sources(&[unit(
            "src/main.sali",
            &[],
            "let main(): i32 = missing.value()\n",
            true,
        )])
        .unwrap();

        assert!(matches!(
            function(&program, "main").body,
            Some(Expr::Call(ref callee, _))
                if matches!(callee.as_ref(), Expr::Member(base, field)
                    if base.as_ref() == &Expr::Name("missing".into()) && field == "value")
        ));
    }

    #[test]
    fn rejects_duplicate_modules_declarations_and_module_name_conflicts() {
        let duplicate_module = resolve_sources(&[
            unit("root.sali", &[], "let main() = {}\n", true),
            unit("one.sali", &["net"], "let one = 1\n", false),
            unit("two.sali", &["net"], "let two = 2\n", false),
        ])
        .unwrap_err();
        assert!(duplicate_module
            .iter()
            .any(|diagnostic| diagnostic.contains("duplicate module `net`")));

        let duplicate_declaration = resolve_sources(&[unit(
            "root.sali",
            &[],
            "let value = 1\nlet value = 2\n",
            true,
        )])
        .unwrap_err();
        assert!(duplicate_declaration
            .iter()
            .any(|diagnostic| diagnostic.contains("duplicate declaration `value`")));

        let conflict = resolve_sources(&[
            unit("root.sali", &[], "let net = 1\n", true),
            unit("net.sali", &["net"], "let value = 2\n", false),
        ])
        .unwrap_err();
        assert!(conflict
            .iter()
            .any(|diagnostic| diagnostic.contains("conflicts with child module")));
    }

    #[test]
    fn requires_exactly_one_root_source() {
        let no_root =
            resolve_sources(&[unit("a.sali", &["a"], "let value = 1\n", false)]).unwrap_err();
        assert!(no_root[0].contains("exactly one root source"));

        let two_roots = resolve_sources(&[
            unit("a.sali", &[], "let a = 1\n", true),
            unit("b.sali", &["b"], "let b = 2\n", true),
        ])
        .unwrap_err();
        assert!(two_roots
            .iter()
            .any(|diagnostic| diagnostic.contains("exactly one root source")));
    }

    #[test]
    fn rejects_module_segments_that_are_unspellable_or_canonicalize_ambiguously() {
        for segment in ["", "_", "let", "Upper", "has-dash", "a.b", "a::b"] {
            let error = resolve_sources(&[
                unit("root.sali", &[], "let main(): i32 = 0\n", true),
                unit("bad.sali", &[segment], "let value = 1\n", false),
            ])
            .unwrap_err();
            assert!(
                error
                    .iter()
                    .any(|diagnostic| diagnostic.contains("module path segment")),
                "segment `{segment}` was not rejected: {error:?}"
            );
        }
    }
}
