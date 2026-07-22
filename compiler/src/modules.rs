//! Multi-file package module resolution.
//!
//! Source files provide their module path out of band; declarations and
//! module-level imports are collected package-wide and then flattened to
//! canonical names understood by the existing codegen.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{
    Binding, CompileParamKind, EnumDef, Expr, ExtendDef, ExtendMember, Field, Function, Item,
    ItemOrigin, MatchArm, Param, Pattern, PatternField, PatternFields, Program, Stmt, StructDef,
    TraitDef, TraitMember, Type, UseDecl, VariantFields, Visibility,
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

/// Stable identity of one package within a compiler invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageId(pub usize);

impl PackageId {
    /// Compiler-owned package identity used by the edition-pinned `core`
    /// bundle. Source package graphs must never claim this identity.
    pub const CORE: Self = Self(usize::MAX);
    /// Compiler-owned package identity used by the edition-pinned `alloc`
    /// bundle. Source package graphs must never claim this identity.
    pub const ALLOC: Self = Self(usize::MAX - 1);
}

/// All source files and direct dependency aliases belonging to one package.
///
/// Package IDs provide definition identity; aliases are local to the package
/// declaring them. This keeps shared dependencies nominally identical and
/// prevents transitive dependencies from becoming accidentally spellable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePackage {
    pub id: PackageId,
    pub is_primary: bool,
    pub dependencies: BTreeMap<String, PackageId>,
    pub sources: Vec<SourceUnit>,
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
    resolve_packages(&[SourcePackage {
        id: PackageId(0),
        is_primary: true,
        dependencies: BTreeMap::new(),
        sources: sources.to_vec(),
    }])
}

/// Parse, resolve, and flatten a complete package dependency graph.
///
/// Exactly one package must be primary. Dependency definitions receive an
/// internal, non-source-spellable namespace while each package keeps its own
/// `root`, `super`, dependency aliases, and `pub(package)` boundary.
pub fn resolve_packages(packages: &[SourcePackage]) -> Result<Program, Vec<String>> {
    resolve_packages_impl(packages, true)
}

/// Resolve compiler-owned source modules without exposing the standard
/// library namespace to the bundle itself.
pub(crate) fn resolve_embedded_sources(sources: &[SourceUnit]) -> Result<Program, Vec<String>> {
    resolve_packages_impl(
        &[SourcePackage {
            id: PackageId(0),
            is_primary: true,
            dependencies: BTreeMap::new(),
            sources: sources.to_vec(),
        }],
        false,
    )
}

fn resolve_packages_impl(
    packages: &[SourcePackage],
    expose_standard_library: bool,
) -> Result<Program, Vec<String>> {
    let (prepared, dependencies, mut diagnostics) =
        validate_package_layout(packages, !expose_standard_library);
    let mut parsed = Vec::with_capacity(prepared.len());

    for unit in prepared {
        match parser::parse(&unit.source.source) {
            Ok(program) => parsed.push(ParsedUnit {
                source: unit.source,
                module_path: unit.module_path,
                package_root: unit.package_root,
                package_id: unit.package_id,
                program,
            }),
            Err(error) => diagnostics.push(format!("{}: error: {error}", unit.source.path)),
        }
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    let (mut symbols, mut module_paths, mut collection_diagnostics) =
        collect_symbols(&parsed, &dependencies);
    let required_imports = if expose_standard_library {
        install_standard_namespaces(
            &parsed,
            &mut symbols,
            &mut module_paths,
            &mut collection_diagnostics,
        )
    } else {
        HashMap::new()
    };
    if !collection_diagnostics.is_empty() {
        return Err(collection_diagnostics);
    }

    let (aliases, import_diagnostics) =
        collect_imports(&parsed, &symbols, &module_paths, &dependencies);
    if !import_diagnostics.is_empty() {
        return Err(import_diagnostics);
    }

    let mut resolver = Resolver {
        symbols,
        module_paths,
        aliases,
        dependencies,
        required_imports,
        diagnostics: Vec::new(),
    };
    let mut items = Vec::new();
    let mut item_visibilities = Vec::new();
    let mut item_origins = Vec::new();
    let mut item_source_paths = Vec::new();

    for ParsedUnit {
        source,
        module_path,
        package_root,
        package_id,
        program,
    } in parsed
    {
        let Program {
            items: unit_items,
            item_visibilities: unit_visibilities,
            item_origins: _,
            uses: _,
        } = program;
        debug_assert_eq!(unit_items.len(), unit_visibilities.len());

        let context = ResolveContext {
            source_path: &source.path,
            module_path: &module_path,
            package_root: &package_root,
        };
        for (mut item, visibility) in unit_items.into_iter().zip(unit_visibilities) {
            resolver.rewrite_item(&mut item, context);
            items.push(item);
            item_visibilities.push(visibility);
            item_origins.push(ItemOrigin {
                package: package_id.0,
                module_path: source.module_path.clone(),
            });
            item_source_paths.push(source.path.clone());
        }
    }

    if resolver.diagnostics.is_empty() {
        let program = Program::with_metadata(items, item_visibilities, item_origins, Vec::new());
        let diagnostics = validate_api_visibility(&program, &item_source_paths);
        if diagnostics.is_empty() {
            Ok(program)
        } else {
            Err(diagnostics)
        }
    } else {
        Err(resolver.diagnostics)
    }
}

struct ParsedUnit<'a> {
    source: &'a SourceUnit,
    /// Absolute module path in the flattened dependency graph.
    module_path: Vec<String>,
    /// Absolute namespace at which this unit's package is mounted.
    package_root: Vec<String>,
    package_id: PackageId,
    program: Program,
}

struct PreparedUnit<'a> {
    source: &'a SourceUnit,
    module_path: Vec<String>,
    package_root: Vec<String>,
    package_id: PackageId,
}

#[derive(Clone, Debug)]
struct Symbol {
    canonical: String,
    module_path: Vec<String>,
    package_root: Vec<String>,
    visibility: Visibility,
    source_path: String,
}

#[derive(Clone, Debug)]
enum AliasTarget {
    Declaration(String),
    Module(Vec<String>),
}

#[derive(Clone, Debug)]
struct ResolvedAlias {
    target: AliasTarget,
    visibility: Visibility,
    module_path: Vec<String>,
    package_root: Vec<String>,
}

type AliasTable = HashMap<Vec<String>, ResolvedAlias>;

type SymbolTable = HashMap<Vec<String>, Symbol>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum DeclarationNamespace {
    Function,
    Type,
    Other,
}

/// Direct dependency aliases keyed by the internal root of the declaring
/// package. Target roots are internal and cannot be written in source code.
type DependencyTable = HashMap<Vec<String>, BTreeMap<String, Vec<String>>>;

/// Public declarations supplied by the edition-pinned `alloc` bundle.
///
/// Their canonical names intentionally match the names used inside the
/// bootstrap bundle. The module resolver is the language boundary: source
/// code reaches these declarations through `alloc.<module>.<name>`, while the
/// analyzer continues to consume the flattened canonical name.
const ALLOC_EXPORTS: &[(&str, &str)] = &[
    ("boxed", "Box"),
    ("boxed", "box_new"),
    ("boxed", "box_ptr"),
    ("boxed", "box_read"),
    ("boxed", "box_write"),
    ("boxed", "box_into_inner"),
    ("boxed", "box_replace"),
    ("boxed", "box_as_ref"),
    ("vec", "Vec"),
    ("vec", "vec_new"),
    ("vec", "vec_with_capacity"),
    ("vec", "vec_len"),
    ("vec", "vec_capacity"),
    ("vec", "vec_at"),
    ("vec", "vec_reserve"),
    ("vec", "vec_push"),
    ("vec", "vec_replace"),
    ("vec", "vec_pop"),
    ("vec", "vec_truncate"),
    ("vec", "vec_clear"),
    ("vec", "vec_is_empty"),
    ("vec", "vec_swap_remove"),
    ("vec", "vec_swap"),
    ("vec", "vec_reverse"),
    ("vec", "vec_insert"),
    ("vec", "vec_remove"),
    ("vec", "vec_append"),
    ("vec", "vec_shrink_to_fit"),
    ("vec", "vec_read"),
    ("vec", "vec_write"),
];

const CORE_PRELUDE_EXPORTS: &[&str] = &["Option", "Result", "Never", "Copy", "Drop"];
const CORE_OPS_EXPORTS: &[&str] = &[
    "Add",
    "Sub",
    "Mul",
    "Div",
    "Rem",
    "AddAssign",
    "SubAssign",
    "MulAssign",
    "DivAssign",
    "RemAssign",
    "BitAndAssign",
    "BitOrAssign",
    "BitXorAssign",
    "ShlAssign",
    "ShrAssign",
    "Eq",
    "PartialOrdering",
    "PartialOrd",
    "Neg",
    "Not",
    "BitAnd",
    "BitOr",
    "BitXor",
    "Shl",
    "Shr",
    "Chain",
    "Coalesce",
];
const CORE_EFFECTS_EXPORTS: &[&str] = &["Unsafe", "Throws", "Async"];
const CORE_ACCESS_EXPORTS: &[&str] = &["Shared", "Mutable"];
const CORE_CONTROL_EXPORTS: &[&str] = &[
    "Continuation",
    "EffectCallable",
    "do",
    "try",
    "throw",
    "unsafe",
    "loop",
];
const CORE_ITER_EXPORTS: &[&str] = &["Iterator", "IntoIterator"];
const CORE_ALGEBRA_EXPORTS: &[&str] = &["Semigroup", "Monoid"];
const CORE_FUNCTIONAL_EXPORTS: &[&str] = &["Functor", "Applicative", "Monad", "ResultWith"];

fn validate_package_layout(
    packages: &[SourcePackage],
    allow_standard_namespaces: bool,
) -> (Vec<PreparedUnit<'_>>, DependencyTable, Vec<String>) {
    let mut diagnostics = Vec::new();
    let primary_count = packages.iter().filter(|package| package.is_primary).count();
    if primary_count != 1 {
        diagnostics.push(format!(
            "<packages>: error: dependency graph must have exactly one primary package, found {primary_count}"
        ));
    }

    let mut package_roots = HashMap::new();
    for package in packages {
        if package.id == PackageId::CORE || package.id == PackageId::ALLOC {
            diagnostics.push(format!(
                "<packages>: error: package ID {} is reserved for compiler {}",
                package.id.0,
                if package.id == PackageId::CORE {
                    "core"
                } else {
                    "alloc"
                }
            ));
        }
        let root = if package.is_primary {
            Vec::new()
        } else {
            vec![format!("@{}", package.id.0)]
        };
        if package_roots.insert(package.id, root).is_some() {
            diagnostics.push(format!(
                "<packages>: error: duplicate package ID {}",
                package.id.0
            ));
        }
    }
    diagnostics.extend(validate_dependency_graph(packages));

    let mut dependencies = DependencyTable::new();
    for package in packages {
        let Some(package_root) = package_roots.get(&package.id) else {
            continue;
        };
        let mut aliases = BTreeMap::new();
        for (alias, target_id) in &package.dependencies {
            if matches!(alias.as_str(), "core" | "alloc") && !allow_standard_namespaces {
                diagnostics.push(format!(
                    "<package {}>: error: dependency alias `{alias}` conflicts with the standard-library namespace",
                    package.id.0,
                ));
                continue;
            }
            if !is_valid_module_segment(alias) {
                diagnostics.push(format!(
                    "<package {}>: error: dependency alias `{alias}` must be a non-reserved ASCII snake_case identifier",
                    package.id.0
                ));
                continue;
            }
            if *target_id == package.id {
                diagnostics.push(format!(
                    "<package {}>: error: dependency alias `{alias}` refers to the package itself",
                    package.id.0
                ));
                continue;
            }
            let Some(target_root) = package_roots.get(target_id) else {
                diagnostics.push(format!(
                    "<package {}>: error: dependency alias `{alias}` refers to unknown package ID {}",
                    package.id.0, target_id.0
                ));
                continue;
            };
            aliases.insert(alias.clone(), target_root.clone());
        }
        dependencies.insert(package_root.clone(), aliases);
    }

    let mut module_owners: HashMap<Vec<String>, (PackageId, &str)> = HashMap::new();
    let mut prepared = Vec::new();

    for package in packages {
        let package_name = format!("#{}", package.id.0);
        let package_root = package_roots
            .get(&package.id)
            .cloned()
            .unwrap_or_else(|| vec![format!("@{}", package.id.0)]);

        let roots = package
            .sources
            .iter()
            .filter(|source| source.is_root)
            .count();
        if roots != 1 {
            diagnostics.push(format!(
                "<package {package_name}>: error: package target must have exactly one root source, found {roots}"
            ));
        }

        let mut relative_modules: HashMap<Vec<String>, &str> = HashMap::new();
        for source in &package.sources {
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
            if let Some(first) = source.module_path.first() {
                if matches!(first.as_str(), "core" | "alloc") && !allow_standard_namespaces {
                    diagnostics.push(format!(
                        "{}: error: top-level module `{first}` conflicts with the standard-library namespace",
                        source.path,
                    ));
                }
                if package.dependencies.contains_key(first) {
                    diagnostics.push(format!(
                        "{}: error: top-level module `{first}` conflicts with dependency alias `{first}`",
                        source.path
                    ));
                }
            }

            if let Some(previous) =
                relative_modules.insert(source.module_path.clone(), &source.path)
            {
                diagnostics.push(format!(
                    "{}: error: duplicate module `{}`; it is already provided by {previous}",
                    source.path,
                    display_module(&source.module_path)
                ));
            }

            let mut module_path = package_root.clone();
            module_path.extend(source.module_path.iter().cloned());
            for length in package_root.len()..=module_path.len() {
                let claimed = module_path[..length].to_vec();
                if let Some((owner, previous)) = module_owners.get(&claimed) {
                    if *owner != package.id {
                        diagnostics.push(format!(
                            "{}: error: module `{}` conflicts with package `{}` provided by {previous}",
                            source.path,
                            display_module(&claimed),
                            owner.0
                        ));
                    }
                } else {
                    module_owners.insert(claimed, (package.id, source.path.as_str()));
                }
            }

            prepared.push(PreparedUnit {
                source,
                module_path,
                package_root: package_root.clone(),
                package_id: package.id,
            });
        }
    }

    (prepared, dependencies, diagnostics)
}

fn validate_dependency_graph(packages: &[SourcePackage]) -> Vec<String> {
    fn visit(
        id: PackageId,
        packages: &HashMap<PackageId, &SourcePackage>,
        states: &mut HashMap<PackageId, u8>,
        stack: &mut Vec<PackageId>,
        diagnostics: &mut Vec<String>,
    ) {
        match states.get(&id) {
            Some(1) => {
                let start = stack.iter().position(|entry| *entry == id).unwrap_or(0);
                let mut cycle = stack[start..]
                    .iter()
                    .map(|entry| format!("#{}", entry.0))
                    .collect::<Vec<_>>();
                cycle.push(format!("#{}", id.0));
                diagnostics.push(format!(
                    "<packages>: error: cyclic package dependencies: {}",
                    cycle.join(" -> ")
                ));
                return;
            }
            Some(2) => return,
            _ => {}
        }

        states.insert(id, 1);
        stack.push(id);
        if let Some(package) = packages.get(&id) {
            for target in package.dependencies.values() {
                if packages.contains_key(target) {
                    visit(*target, packages, states, stack, diagnostics);
                }
            }
        }
        stack.pop();
        states.insert(id, 2);
    }

    let mut diagnostics = Vec::new();
    let mut by_id = HashMap::new();
    for package in packages {
        by_id.entry(package.id).or_insert(package);
    }

    let mut states = HashMap::new();
    let mut stack = Vec::new();
    let mut ids = by_id.keys().copied().collect::<Vec<_>>();
    ids.sort();
    for id in ids {
        visit(id, &by_id, &mut states, &mut stack, &mut diagnostics);
    }

    if let [primary] = packages
        .iter()
        .filter(|package| package.is_primary)
        .collect::<Vec<_>>()
        .as_slice()
    {
        let mut reachable = HashSet::new();
        let mut pending = vec![primary.id];
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            if let Some(package) = by_id.get(&id) {
                pending.extend(
                    package
                        .dependencies
                        .values()
                        .filter(|target| by_id.contains_key(target))
                        .copied(),
                );
            }
        }
        let mut unreachable = by_id
            .keys()
            .filter(|id| !reachable.contains(id))
            .copied()
            .collect::<Vec<_>>();
        unreachable.sort();
        for id in unreachable {
            diagnostics.push(format!(
                "<package #{}>: error: package is not reachable from primary package #{}",
                id.0, primary.id.0
            ));
        }
    }

    diagnostics
}

fn collect_symbols(
    parsed: &[ParsedUnit<'_>],
    dependencies: &DependencyTable,
) -> (SymbolTable, HashSet<Vec<String>>, Vec<String>) {
    let mut symbols: SymbolTable = HashMap::new();
    let mut symbol_namespaces = HashMap::<Vec<String>, HashSet<DeclarationNamespace>>::new();
    let mut module_paths = HashSet::new();
    let mut diagnostics = Vec::new();
    let mut function_overloads = HashMap::<Vec<String>, HashSet<Vec<Vec<String>>>>::new();
    let mut module_children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();

    for unit in parsed {
        for length in 0..=unit.module_path.len() {
            module_paths.insert(unit.module_path[..length].to_vec());
        }
        for index in 0..unit.module_path.len() {
            module_children
                .entry(unit.module_path[..index].to_vec())
                .or_default()
                .insert(unit.module_path[index].clone());
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
            if unit.module_path == unit.package_root
                && dependencies
                    .get(&unit.package_root)
                    .is_some_and(|aliases| aliases.contains_key(name))
            {
                diagnostics.push(format!(
                    "{}: error: root declaration `{name}` conflicts with dependency alias `{name}`",
                    unit.source.path
                ));
            }
            let mut logical_path = unit.module_path.clone();
            logical_path.push(name.to_owned());
            let symbol = Symbol {
                canonical: canonical_name(&unit.module_path, name),
                module_path: unit.module_path.clone(),
                package_root: unit.package_root.clone(),
                visibility: *visibility,
                source_path: unit.source.path.clone(),
            };

            let namespace = declaration_namespace(item);
            if let Some(previous) = symbols.get(&logical_path) {
                let occupied = symbol_namespaces
                    .get(&logical_path)
                    .cloned()
                    .unwrap_or_default();
                let type_function_pair = matches!(
                    (namespace, occupied.contains(&DeclarationNamespace::Type)),
                    (DeclarationNamespace::Function, true)
                ) || matches!(
                    (
                        namespace,
                        occupied.contains(&DeclarationNamespace::Function)
                    ),
                    (DeclarationNamespace::Type, true)
                );
                if type_function_pair && *visibility != previous.visibility {
                    diagnostics.push(format!(
                        "{}: error: declaration `{name}` must use the same visibility as the same-named declaration in {}",
                        unit.source.path, previous.source_path
                    ));
                    continue;
                }
                if let Item::Function(function) = item {
                    let shape = function
                        .groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    if type_function_pair
                        && !occupied.contains(&DeclarationNamespace::Other)
                        && !occupied.contains(&DeclarationNamespace::Function)
                    {
                        function_overloads
                            .entry(logical_path.clone())
                            .or_default()
                            .insert(shape);
                        symbol_namespaces
                            .entry(logical_path)
                            .or_default()
                            .insert(DeclarationNamespace::Function);
                        continue;
                    }
                    let Some(overloads) = function_overloads.get_mut(&logical_path) else {
                        diagnostics.push(format!(
                            "{}: error: duplicate declaration `{name}` in module `{}`; first declared in {}",
                            unit.source.path,
                            display_module(&unit.module_path),
                            previous.source_path
                        ));
                        continue;
                    };
                    if *visibility != previous.visibility {
                        diagnostics.push(format!(
                            "{}: error: overloads of `{name}` must use the same visibility as the declaration in {}",
                            unit.source.path, previous.source_path
                        ));
                    } else if !overloads.insert(shape) {
                        diagnostics.push(format!(
                            "{}: error: duplicate overload `{name}` has the same parameter labels as the declaration in {}",
                            unit.source.path, previous.source_path
                        ));
                    }
                } else if type_function_pair
                    && !occupied.contains(&DeclarationNamespace::Other)
                    && !occupied.contains(&DeclarationNamespace::Type)
                {
                    symbol_namespaces
                        .entry(logical_path)
                        .or_default()
                        .insert(namespace);
                    continue;
                } else {
                    diagnostics.push(format!(
                        "{}: error: duplicate declaration `{name}` in module `{}`; first declared in {}",
                        unit.source.path,
                        display_module(&unit.module_path),
                        previous.source_path
                    ));
                }
            } else {
                if let Item::Function(function) = item {
                    function_overloads.insert(
                        logical_path.clone(),
                        HashSet::from([function
                            .groups
                            .iter()
                            .map(|group| {
                                group
                                    .iter()
                                    .map(|parameter| parameter.name.clone())
                                    .collect::<Vec<_>>()
                            })
                            .collect::<Vec<_>>()]),
                    );
                }
                symbol_namespaces
                    .entry(logical_path.clone())
                    .or_default()
                    .insert(namespace);
                symbols.insert(logical_path, symbol);
            }

            if module_children
                .get(&unit.module_path)
                .is_some_and(|children| children.contains(name))
            {
                diagnostics.push(format!(
                    "{}: error: declaration `{name}` conflicts with child module `{}`",
                    unit.source.path,
                    canonical_name(&unit.module_path, name)
                ));
            }
        }
    }

    (symbols, module_paths, diagnostics)
}

fn install_standard_namespaces(
    parsed: &[ParsedUnit<'_>],
    symbols: &mut SymbolTable,
    module_paths: &mut HashSet<Vec<String>>,
    diagnostics: &mut Vec<String>,
) -> HashMap<String, String> {
    let package_roots = parsed
        .iter()
        .map(|unit| unit.package_root.clone())
        .collect::<BTreeSet<_>>();
    let mut required_imports = HashMap::new();

    for (module, name) in ALLOC_EXPORTS {
        required_imports.insert((*name).to_owned(), format!("alloc.{module}.{name}"));
    }
    for name in CORE_OPS_EXPORTS {
        required_imports.insert((*name).to_owned(), format!("core.ops.{name}"));
    }
    for name in CORE_EFFECTS_EXPORTS {
        required_imports.insert((*name).to_owned(), format!("core.effects.{name}"));
    }
    for name in CORE_ACCESS_EXPORTS {
        required_imports.insert((*name).to_owned(), format!("core.access.{name}"));
    }
    for name in CORE_ALGEBRA_EXPORTS {
        required_imports.insert((*name).to_owned(), format!("core.algebra.{name}"));
    }
    for name in CORE_FUNCTIONAL_EXPORTS {
        required_imports.insert((*name).to_owned(), format!("core.functional.{name}"));
    }
    for name in CORE_ITER_EXPORTS {
        required_imports.insert((*name).to_owned(), format!("core.iter.{name}"));
    }

    for package_root in package_roots {
        let mut core_root = package_root.clone();
        core_root.push("core".to_owned());
        if module_paths.contains(&core_root) {
            diagnostics.push(
                "<package>: error: top-level module `core` conflicts with the standard-library namespace"
                    .to_owned(),
            );
        } else if let Some(symbol) = symbols.get(&core_root) {
            diagnostics.push(format!(
                "{}: error: root declaration `core` conflicts with the standard-library namespace",
                symbol.source_path
            ));
        } else {
            module_paths.insert(core_root.clone());
            for module in [
                "prelude",
                "ops",
                "effects",
                "access",
                "control",
                "iter",
                "algebra",
                "functional",
            ] {
                let mut module_path = core_root.clone();
                module_path.push(module.to_owned());
                module_paths.insert(module_path);
            }
            for name in CORE_PRELUDE_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("prelude".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: (*name).to_owned(),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
            for name in CORE_OPS_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("ops".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: format!("core::ops::{name}"),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
            for name in CORE_EFFECTS_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("effects".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: format!("core::effects::{name}"),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
            for name in CORE_ACCESS_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("access".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: format!("core::access::{name}"),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
            for name in CORE_CONTROL_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("control".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: format!("core::control::{name}"),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
            for name in CORE_ALGEBRA_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("algebra".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: format!("core::algebra::{name}"),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
            for name in CORE_FUNCTIONAL_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("functional".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: format!("core::functional::{name}"),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
            for name in CORE_ITER_EXPORTS {
                let mut module_path = core_root.clone();
                module_path.push("iter".to_owned());
                let mut logical_path = module_path.clone();
                logical_path.push((*name).to_owned());
                symbols.insert(
                    logical_path,
                    Symbol {
                        canonical: format!("core::iter::{name}"),
                        module_path,
                        package_root: package_root.clone(),
                        visibility: Visibility::Public,
                        source_path: "<core>".to_owned(),
                    },
                );
            }
        }

        let mut alloc_root = package_root.clone();
        alloc_root.push("alloc".to_owned());
        if module_paths.contains(&alloc_root) {
            diagnostics.push(
                "<package>: error: top-level module `alloc` conflicts with the standard-library namespace"
                    .to_owned(),
            );
            continue;
        }
        if let Some(symbol) = symbols.get(&alloc_root) {
            diagnostics.push(format!(
                "{}: error: root declaration `alloc` conflicts with the standard-library namespace",
                symbol.source_path
            ));
            continue;
        }

        module_paths.insert(alloc_root.clone());
        for module in ["boxed", "vec"] {
            let mut module_path = alloc_root.clone();
            module_path.push(module.to_owned());
            module_paths.insert(module_path);
        }
        for (module, name) in ALLOC_EXPORTS {
            let mut module_path = alloc_root.clone();
            module_path.push((*module).to_owned());
            let mut logical_path = module_path.clone();
            logical_path.push((*name).to_owned());
            symbols.insert(
                logical_path,
                Symbol {
                    canonical: format!("alloc::{module}::{name}"),
                    module_path,
                    package_root: package_root.clone(),
                    visibility: Visibility::Public,
                    source_path: "<alloc>".to_owned(),
                },
            );
        }
    }

    required_imports
}

#[derive(Clone, Debug)]
struct ImportDef {
    path: Vec<String>,
    alias: String,
    visibility: Visibility,
    module_path: Vec<String>,
    package_root: Vec<String>,
    source_path: String,
}

fn collect_imports(
    parsed: &[ParsedUnit<'_>],
    symbols: &SymbolTable,
    module_paths: &HashSet<Vec<String>>,
    dependencies: &DependencyTable,
) -> (AliasTable, Vec<String>) {
    let mut definitions: HashMap<Vec<String>, ImportDef> = HashMap::new();
    let mut diagnostics = Vec::new();

    for unit in parsed {
        for declaration in &unit.program.uses {
            let Some(alias) = import_alias(declaration) else {
                diagnostics.push(format!(
                    "{}: error: import path must not be empty",
                    unit.source.path
                ));
                continue;
            };
            if !is_valid_import_alias(&alias) {
                diagnostics.push(format!(
                    "{}: error: `{alias}` cannot be used as an import alias",
                    unit.source.path
                ));
                continue;
            }
            if let Err(message) = validate_import_path(declaration) {
                diagnostics.push(format!("{}: error: {message}", unit.source.path));
                continue;
            }

            let mut key = unit.module_path.clone();
            key.push(alias.clone());
            let definition = ImportDef {
                path: declaration.path.clone(),
                alias,
                visibility: declaration.visibility,
                module_path: unit.module_path.clone(),
                package_root: unit.package_root.clone(),
                source_path: unit.source.path.clone(),
            };

            if let Some(symbol) = symbols.get(&key) {
                diagnostics.push(format!(
                    "{}: error: import alias `{}` conflicts with declaration in {}",
                    unit.source.path, definition.alias, symbol.source_path
                ));
            } else if module_paths.contains(&key) {
                diagnostics.push(format!(
                    "{}: error: import alias `{}` conflicts with child module `{}`",
                    unit.source.path,
                    definition.alias,
                    key.join(".")
                ));
            } else if unit.module_path == unit.package_root
                && dependencies
                    .get(&unit.package_root)
                    .is_some_and(|aliases| aliases.contains_key(&definition.alias))
            {
                diagnostics.push(format!(
                    "{}: error: import alias `{}` conflicts with dependency alias `{}`",
                    unit.source.path, definition.alias, definition.alias
                ));
            } else if let Some(previous) = definitions.get(&key) {
                diagnostics.push(format!(
                    "{}: error: duplicate import alias `{}` for `{}` and `{}`",
                    unit.source.path,
                    definition.alias,
                    display_path(&previous.path),
                    display_path(&definition.path)
                ));
            } else {
                definitions.insert(key, definition);
            }
        }
    }

    if !diagnostics.is_empty() {
        return (HashMap::new(), diagnostics);
    }

    let mut graph = ImportGraph {
        definitions,
        symbols,
        module_paths,
        dependencies,
        resolved: HashMap::new(),
        failed: HashSet::new(),
        stack: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut keys: Vec<Vec<String>> = graph.definitions.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let _ = graph.resolve_alias(&key);
    }
    (graph.resolved, graph.diagnostics)
}

fn import_alias(declaration: &UseDecl) -> Option<String> {
    declaration
        .alias
        .clone()
        .or_else(|| declaration.path.last().cloned())
}

fn is_valid_import_alias(alias: &str) -> bool {
    if alias == "_" || alias == "self" || lexer::is_keyword(alias) {
        return false;
    }
    let mut characters = alias.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn validate_import_path(declaration: &UseDecl) -> Result<(), String> {
    if declaration.path.is_empty() {
        return Err("import path must not be empty".into());
    }
    let leading_supers = declaration
        .path
        .iter()
        .take_while(|segment| segment.as_str() == "super")
        .count();
    for (index, segment) in declaration.path.iter().enumerate() {
        if matches!(segment.as_str(), "root" | "super")
            && index > 0
            && !(segment == "super" && index < leading_supers)
        {
            return Err(format!(
                "import anchor `{segment}` is only valid at the start of a path"
            ));
        }
    }
    let anchor_only = declaration
        .path
        .iter()
        .all(|segment| matches!(segment.as_str(), "root" | "self" | "super"));
    if anchor_only && declaration.alias.is_none() {
        return Err(format!(
            "anchor-only import `{}` requires an explicit alias",
            display_path(&declaration.path)
        ));
    }
    Ok(())
}

fn display_path(path: &[String]) -> String {
    path.join(".")
}

struct ImportGraph<'a> {
    definitions: HashMap<Vec<String>, ImportDef>,
    symbols: &'a SymbolTable,
    module_paths: &'a HashSet<Vec<String>>,
    dependencies: &'a DependencyTable,
    resolved: AliasTable,
    failed: HashSet<Vec<String>>,
    stack: Vec<Vec<String>>,
    diagnostics: Vec<String>,
}

#[derive(Clone)]
struct ImportReference {
    target: AliasTarget,
    accesses: Vec<ImportAccess>,
}

#[derive(Clone)]
struct ImportAccess {
    visibility: Visibility,
    module_path: Vec<String>,
    package_root: Vec<String>,
    display: String,
}

impl ImportGraph<'_> {
    fn resolve_alias(&mut self, key: &[String]) -> Result<ResolvedAlias, ()> {
        if let Some(alias) = self.resolved.get(key) {
            return Ok(alias.clone());
        }
        if self.failed.contains(key) {
            return Err(());
        }
        if let Some(start) = self.stack.iter().position(|entry| entry == key) {
            let mut cycle: Vec<String> = self.stack[start..]
                .iter()
                .map(|entry| entry.join("."))
                .collect();
            cycle.push(key.join("."));
            let source = self
                .definitions
                .get(key)
                .map(|definition| definition.source_path.as_str())
                .unwrap_or("<package>");
            self.diagnostics.push(format!(
                "{source}: error: cyclic import aliases: {}",
                cycle.join(" -> ")
            ));
            self.failed.extend(self.stack[start..].iter().cloned());
            return Err(());
        }

        let Some(definition) = self.definitions.get(key).cloned() else {
            return Err(());
        };
        self.stack.push(key.to_vec());
        let reference = self.resolve_import_path(&definition);
        self.stack.pop();
        let reference = match reference {
            Ok(reference) => reference,
            Err(()) => {
                self.failed.insert(key.to_vec());
                return Err(());
            }
        };

        for access in &reference.accesses {
            if !visibility_allows_access(
                access.visibility,
                &access.module_path,
                &access.package_root,
                &definition.module_path,
                &definition.package_root,
            ) {
                self.diagnostics.push(format!(
                    "{}: error: import `{}` cannot access {} target `{}`",
                    definition.source_path,
                    display_path(&definition.path),
                    visibility_description(access.visibility),
                    access.display
                ));
                self.failed.insert(key.to_vec());
                return Err(());
            }
        }

        let limiting_access = reference
            .accesses
            .iter()
            .min_by_key(|access| visibility_rank(access.visibility))
            .expect("every import target has an access boundary");
        if visibility_rank(definition.visibility) > visibility_rank(limiting_access.visibility) {
            self.diagnostics.push(format!(
                "{}: error: {} use `{}` cannot re-export {} target `{}`",
                definition.source_path,
                visibility_source(definition.visibility),
                display_path(&definition.path),
                visibility_description(limiting_access.visibility),
                limiting_access.display
            ));
            self.failed.insert(key.to_vec());
            return Err(());
        }

        let alias = ResolvedAlias {
            target: reference.target,
            visibility: definition.visibility,
            module_path: definition.module_path,
            package_root: definition.package_root,
        };
        self.resolved.insert(key.to_vec(), alias.clone());
        Ok(alias)
    }

    fn resolve_import_path(&mut self, definition: &ImportDef) -> Result<ImportReference, ()> {
        let candidates = match anchored_candidates(
            &definition.path,
            &definition.module_path,
            &definition.package_root,
        ) {
            Ok(candidates) => candidates,
            Err(message) => {
                self.diagnostics
                    .push(format!("{}: error: {message}", definition.source_path));
                return Err(());
            }
        };
        for candidate in candidates {
            if self
                .stack
                .last()
                .is_some_and(|current| current == &candidate)
            {
                continue;
            }
            if let Some(reference) = self.lookup_import_candidate(&candidate, 0)? {
                return Ok(reference);
            }
            if let Some(expanded) =
                expand_dependency_candidate(&candidate, &definition.package_root, self.dependencies)
            {
                if let Some(reference) = self.lookup_import_candidate(&expanded, 0)? {
                    return Ok(reference);
                }
            }
        }
        self.diagnostics.push(format!(
            "{}: error: unknown import `{}`",
            definition.source_path,
            display_path(&definition.path)
        ));
        Err(())
    }

    fn lookup_import_candidate(
        &mut self,
        candidate: &[String],
        depth: usize,
    ) -> Result<Option<ImportReference>, ()> {
        if depth > self.definitions.len() + 1 {
            return Ok(None);
        }
        if let Some(symbol) = self.symbols.get(candidate) {
            return Ok(Some(ImportReference {
                target: AliasTarget::Declaration(symbol.canonical.clone()),
                accesses: vec![ImportAccess {
                    visibility: symbol.visibility,
                    module_path: symbol.module_path.clone(),
                    package_root: symbol.package_root.clone(),
                    display: symbol.canonical.replace("::", "."),
                }],
            }));
        }
        if self.definitions.contains_key(candidate) {
            let alias = self.resolve_alias(candidate)?;
            return Ok(Some(ImportReference {
                target: alias.target,
                accesses: vec![ImportAccess {
                    visibility: alias.visibility,
                    module_path: alias.module_path,
                    package_root: alias.package_root,
                    display: candidate.join("."),
                }],
            }));
        }
        if self.module_paths.contains(candidate) {
            return Ok(Some(ImportReference {
                target: AliasTarget::Module(candidate.to_vec()),
                accesses: vec![ImportAccess {
                    visibility: Visibility::Public,
                    module_path: candidate[..candidate.len().saturating_sub(1)].to_vec(),
                    package_root: Vec::new(),
                    display: candidate.join("."),
                }],
            }));
        }

        for length in (1..candidate.len()).rev() {
            let prefix = &candidate[..length];
            if !self.definitions.contains_key(prefix) {
                continue;
            }
            let alias = self.resolve_alias(prefix)?;
            if let AliasTarget::Module(module) = alias.target {
                let mut expanded = module;
                expanded.extend_from_slice(&candidate[length..]);
                if let Some(mut reference) = self.lookup_import_candidate(&expanded, depth + 1)? {
                    reference.accesses.push(ImportAccess {
                        visibility: alias.visibility,
                        module_path: alias.module_path,
                        package_root: alias.package_root,
                        display: prefix.join("."),
                    });
                    return Ok(Some(reference));
                }
            }
        }
        Ok(None)
    }
}

fn anchored_candidates(
    path: &[String],
    module_path: &[String],
    package_root: &[String],
) -> Result<Vec<Vec<String>>, String> {
    let Some(first) = path.first().map(String::as_str) else {
        return Ok(Vec::new());
    };
    match first {
        "root" => {
            let mut candidate = package_root.to_vec();
            candidate.extend_from_slice(&path[1..]);
            Ok(vec![candidate])
        }
        "self" => {
            let mut candidate = module_path.to_vec();
            candidate.extend_from_slice(&path[1..]);
            Ok(vec![candidate])
        }
        "super" => {
            let count = path
                .iter()
                .take_while(|segment| *segment == "super")
                .count();
            if count > module_path.len().saturating_sub(package_root.len()) {
                return Err(format!(
                    "import path `{}` escapes above the package root",
                    display_path(path)
                ));
            }
            let mut candidate = module_path[..module_path.len() - count].to_vec();
            candidate.extend_from_slice(&path[count..]);
            Ok(vec![candidate])
        }
        _ => Ok((package_root.len()..=module_path.len())
            .rev()
            .map(|depth| {
                let mut candidate = module_path[..depth].to_vec();
                candidate.extend_from_slice(path);
                candidate
            })
            .collect()),
    }
}

fn expand_dependency_candidate(
    candidate: &[String],
    package_root: &[String],
    dependencies: &DependencyTable,
) -> Option<Vec<String>> {
    if !candidate.starts_with(package_root) || candidate.len() <= package_root.len() {
        return None;
    }
    let alias = &candidate[package_root.len()];
    let target_root = dependencies.get(package_root)?.get(alias)?;
    let mut expanded = target_root.clone();
    expanded.extend_from_slice(&candidate[package_root.len() + 1..]);
    Some(expanded)
}

fn visibility_rank(visibility: Visibility) -> u8 {
    match visibility {
        Visibility::Private => 0,
        Visibility::Package => 1,
        Visibility::Public => 2,
    }
}

fn visibility_allows_access(
    visibility: Visibility,
    declaring_module: &[String],
    declaring_package: &[String],
    accessing_module: &[String],
    accessing_package: &[String],
) -> bool {
    match visibility {
        Visibility::Public => true,
        Visibility::Package => declaring_package == accessing_package,
        Visibility::Private => {
            declaring_package == accessing_package && accessing_module.starts_with(declaring_module)
        }
    }
}

fn visibility_source(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "private",
        Visibility::Package => "pub(package)",
        Visibility::Public => "pub",
    }
}

fn visibility_description(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "private",
        Visibility::Package => "pub(package)",
        Visibility::Public => "public",
    }
}

#[derive(Clone, Debug)]
struct ApiBoundary {
    visibility: Visibility,
    origin: ItemOrigin,
}

fn validate_api_visibility(program: &Program, item_source_paths: &[String]) -> Vec<String> {
    debug_assert_eq!(program.items.len(), program.item_visibilities.len());
    debug_assert_eq!(program.items.len(), program.item_origins.len());
    debug_assert_eq!(program.items.len(), item_source_paths.len());

    let nominal_boundaries = program
        .items
        .iter()
        .zip(&program.item_visibilities)
        .zip(&program.item_origins)
        .filter_map(|((item, visibility), origin)| {
            let name = match item {
                Item::Struct(definition) => &definition.name,
                Item::Enum(definition) => &definition.name,
                Item::Trait(definition) => &definition.name,
                Item::Effect(definition) => &definition.name,
                Item::Access(definition) => &definition.name,
                Item::Function(_) | Item::Global(_) | Item::TypeAlias(_) | Item::Extend(_) => {
                    return None
                }
            };
            Some((
                name.as_str(),
                ApiBoundary {
                    visibility: *visibility,
                    origin: origin.clone(),
                },
            ))
        })
        .collect::<HashMap<_, _>>();

    let mut diagnostics = Vec::new();
    for (((item, visibility), origin), source_path) in program
        .items
        .iter()
        .zip(&program.item_visibilities)
        .zip(&program.item_origins)
        .zip(item_source_paths)
    {
        let boundary = ApiBoundary {
            visibility: *visibility,
            origin: origin.clone(),
        };
        validate_item_api(
            item,
            &boundary,
            source_path,
            &nominal_boundaries,
            &mut diagnostics,
        );
    }

    diagnostics.sort();
    diagnostics.dedup();
    diagnostics
}

fn validate_item_api(
    item: &Item,
    boundary: &ApiBoundary,
    source_path: &str,
    nominal_boundaries: &HashMap<&str, ApiBoundary>,
    diagnostics: &mut Vec<String>,
) {
    let no_bound_types = HashSet::new();
    match item {
        Item::Function(function) => validate_function_api(
            function,
            boundary,
            source_path,
            &no_bound_types,
            &format!("function `{}`", function.name),
            nominal_boundaries,
            diagnostics,
        ),
        Item::Global(binding) => {
            if let Some(annotation) = &binding.annotation {
                validate_exposed_type(
                    annotation,
                    boundary,
                    source_path,
                    &no_bound_types,
                    &format!("global `{}` type", binding.name),
                    nominal_boundaries,
                    diagnostics,
                );
            }
        }
        Item::TypeAlias(definition) => {
            let bound_types = compile_parameter_names(&definition.compile_groups, &no_bound_types);
            validate_exposed_type(
                &definition.target,
                boundary,
                source_path,
                &bound_types,
                &format!("type alias `{}` target", definition.name),
                nominal_boundaries,
                diagnostics,
            );
        }
        Item::Effect(definition) => {
            let bound_types = compile_parameter_names(&definition.compile_groups, &no_bound_types);
            for operation in &definition.operations {
                validate_function_api(
                    operation,
                    boundary,
                    source_path,
                    &bound_types,
                    &format!("effect operation `{}.{}`", definition.name, operation.name),
                    nominal_boundaries,
                    diagnostics,
                );
            }
        }
        Item::Access(_) => {}
        Item::Struct(definition) => {
            let bound_types = compile_parameter_names(&definition.compile_groups, &no_bound_types);
            for field in &definition.fields {
                let field_boundary = effective_api_boundary(boundary, field.visibility);
                validate_exposed_type(
                    &field.ty,
                    &field_boundary,
                    source_path,
                    &bound_types,
                    &format!("field `{}.{}`", definition.name, field.name),
                    nominal_boundaries,
                    diagnostics,
                );
            }
        }
        Item::Enum(definition) => {
            let bound_types = compile_parameter_names(&definition.compile_groups, &no_bound_types);
            for variant in &definition.variants {
                match &variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Positional(types) => {
                        for (field_index, ty) in types.iter().enumerate() {
                            validate_exposed_type(
                                ty,
                                boundary,
                                source_path,
                                &bound_types,
                                &format!(
                                    "enum variant `{}.{}` payload {}",
                                    definition.name, variant.name, field_index
                                ),
                                nominal_boundaries,
                                diagnostics,
                            );
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            let field_boundary = effective_api_boundary(boundary, field.visibility);
                            validate_exposed_type(
                                &field.ty,
                                &field_boundary,
                                source_path,
                                &bound_types,
                                &format!(
                                    "enum variant field `{}.{}.{}`",
                                    definition.name, variant.name, field.name
                                ),
                                nominal_boundaries,
                                diagnostics,
                            );
                        }
                    }
                }
            }
        }
        Item::Trait(definition) => {
            let mut trait_bound_types =
                compile_parameter_names(&definition.compile_groups, &no_bound_types);
            trait_bound_types.insert("Self".to_owned());
            trait_bound_types.extend(definition.members.iter().filter_map(|member| match member {
                TraitMember::AssociatedType { name, .. } => Some(name.clone()),
                TraitMember::Function(_) => None,
            }));

            for member in &definition.members {
                match member {
                    TraitMember::Function(function) => validate_function_api(
                        function,
                        boundary,
                        source_path,
                        &trait_bound_types,
                        &format!("trait method `{}.{}`", definition.name, function.name),
                        nominal_boundaries,
                        diagnostics,
                    ),
                    TraitMember::AssociatedType {
                        name,
                        compile_groups,
                        default: Some(default),
                    } => {
                        let bound_types =
                            compile_parameter_names(compile_groups, &trait_bound_types);
                        validate_exposed_type(
                            default,
                            boundary,
                            source_path,
                            &bound_types,
                            &format!("associated type `{}.{name}` default", definition.name),
                            nominal_boundaries,
                            diagnostics,
                        );
                    }
                    TraitMember::AssociatedType { default: None, .. } => {}
                }
            }
        }
        Item::Extend(extension) => {
            let extension_boundary = match &extension.target {
                Type::Named(name, _) => nominal_boundaries.get(name.as_str()).unwrap_or(boundary),
                _ => boundary,
            };
            let mut bound_types =
                compile_parameter_names(&extension.compile_groups, &no_bound_types);
            bound_types.insert("Self".to_owned());
            for (index, predicate) in extension.where_predicates.iter().enumerate() {
                validate_exposed_type(
                    &predicate.subject,
                    extension_boundary,
                    source_path,
                    &bound_types,
                    &format!("extension where predicate {} subject", index + 1),
                    nominal_boundaries,
                    diagnostics,
                );
                validate_exposed_type(
                    &predicate.trait_ref,
                    extension_boundary,
                    source_path,
                    &bound_types,
                    &format!("extension where predicate {} trait", index + 1),
                    nominal_boundaries,
                    diagnostics,
                );
                for binding in &predicate.associated_types {
                    validate_exposed_type(
                        &binding.ty,
                        extension_boundary,
                        source_path,
                        &bound_types,
                        &format!(
                            "extension where predicate {} associated type `{}`",
                            index + 1,
                            binding.name
                        ),
                        nominal_boundaries,
                        diagnostics,
                    );
                }
            }
            for member in &extension.members {
                match member {
                    ExtendMember::Function(function) => validate_function_api(
                        function,
                        extension_boundary,
                        source_path,
                        &bound_types,
                        &format!("extension method `{}`", function.name),
                        nominal_boundaries,
                        diagnostics,
                    ),
                    ExtendMember::Const(binding) => {
                        if let Some(annotation) = &binding.annotation {
                            validate_exposed_type(
                                annotation,
                                extension_boundary,
                                source_path,
                                &bound_types,
                                &format!("extension constant `{}`", binding.name),
                                nominal_boundaries,
                                diagnostics,
                            );
                        }
                    }
                }
            }
        }
    }
}

fn validate_function_api(
    function: &Function,
    boundary: &ApiBoundary,
    source_path: &str,
    outer_bound_types: &HashSet<String>,
    description: &str,
    nominal_boundaries: &HashMap<&str, ApiBoundary>,
    diagnostics: &mut Vec<String>,
) {
    let bound_types = compile_parameter_names(&function.compile_groups, outer_bound_types);
    for parameter in function.groups.iter().flatten() {
        validate_exposed_type(
            &parameter.ty,
            boundary,
            source_path,
            &bound_types,
            &format!("{description} parameter `{}`", parameter.name),
            nominal_boundaries,
            diagnostics,
        );
    }
    if let Some(return_type) = &function.return_type {
        validate_exposed_type(
            return_type,
            boundary,
            source_path,
            &bound_types,
            &format!("{description} return type"),
            nominal_boundaries,
            diagnostics,
        );
    }
    for effect in &function.effects.custom {
        validate_exposed_effect(
            effect,
            boundary,
            source_path,
            &bound_types,
            description,
            nominal_boundaries,
            diagnostics,
        );
    }
    for (index, predicate) in function.where_predicates.iter().enumerate() {
        validate_exposed_type(
            &predicate.subject,
            boundary,
            source_path,
            &bound_types,
            &format!("{description} where predicate {} subject", index + 1),
            nominal_boundaries,
            diagnostics,
        );
        validate_exposed_type(
            &predicate.trait_ref,
            boundary,
            source_path,
            &bound_types,
            &format!("{description} where predicate {} trait", index + 1),
            nominal_boundaries,
            diagnostics,
        );
        for binding in &predicate.associated_types {
            validate_exposed_type(
                &binding.ty,
                boundary,
                source_path,
                &bound_types,
                &format!(
                    "{description} where predicate {} associated type `{}`",
                    index + 1,
                    binding.name
                ),
                nominal_boundaries,
                diagnostics,
            );
        }
    }
}

fn validate_exposed_type(
    ty: &Type,
    exposed: &ApiBoundary,
    source_path: &str,
    bound_types: &HashSet<String>,
    description: &str,
    nominal_boundaries: &HashMap<&str, ApiBoundary>,
    diagnostics: &mut Vec<String>,
) {
    match ty {
        Type::Borrow { pointee, .. } => validate_exposed_type(
            pointee,
            exposed,
            source_path,
            bound_types,
            description,
            nominal_boundaries,
            diagnostics,
        ),
        Type::Array(element, _) => validate_exposed_type(
            element,
            exposed,
            source_path,
            bound_types,
            description,
            nominal_boundaries,
            diagnostics,
        ),
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter().flatten() {
                validate_exposed_type(
                    ty,
                    exposed,
                    source_path,
                    bound_types,
                    description,
                    nominal_boundaries,
                    diagnostics,
                );
            }
            validate_exposed_type(
                result,
                exposed,
                source_path,
                bound_types,
                description,
                nominal_boundaries,
                diagnostics,
            );
            for effect in &effects.custom {
                validate_exposed_effect(
                    effect,
                    exposed,
                    source_path,
                    bound_types,
                    description,
                    nominal_boundaries,
                    diagnostics,
                );
            }
        }
        Type::Named(name, arguments) => {
            if !is_bound_api_type(name, bound_types) {
                if let Some(referenced) = nominal_boundaries.get(name.as_str()) {
                    if !api_audience_is_contained(exposed, referenced) {
                        diagnostics.push(format!(
                            "{source_path}: error: {description} with {} visibility exposes {} type `{name}` beyond its access boundary{}",
                            visibility_description(exposed.visibility),
                            visibility_description(referenced.visibility),
                            boundary_location(referenced),
                        ));
                    }
                }
            }
            for argument in arguments {
                validate_exposed_type(
                    argument,
                    exposed,
                    source_path,
                    bound_types,
                    description,
                    nominal_boundaries,
                    diagnostics,
                );
            }
        }
        Type::NamedArgs(name, arguments) => {
            if !is_bound_api_type(name, bound_types) {
                if let Some(referenced) = nominal_boundaries.get(name.as_str()) {
                    if !api_audience_is_contained(exposed, referenced) {
                        diagnostics.push(format!(
                            "{source_path}: error: {description} with {} visibility exposes {} type `{name}` beyond its access boundary{}",
                            visibility_description(exposed.visibility),
                            visibility_description(referenced.visibility),
                            boundary_location(referenced),
                        ));
                    }
                }
            }
            for argument in arguments {
                validate_exposed_type(
                    &argument.ty,
                    exposed,
                    source_path,
                    bound_types,
                    description,
                    nominal_boundaries,
                    diagnostics,
                );
            }
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => {}
    }
}

fn validate_exposed_effect(
    effect: &Type,
    exposed: &ApiBoundary,
    source_path: &str,
    bound_types: &HashSet<String>,
    description: &str,
    nominal_boundaries: &HashMap<&str, ApiBoundary>,
    diagnostics: &mut Vec<String>,
) {
    let (name, positional, labeled) = match effect {
        Type::Named(name, arguments) => (name, Some(arguments.as_slice()), None),
        Type::NamedArgs(name, arguments) => (name, None, Some(arguments.as_slice())),
        _ => return,
    };
    if let Some(referenced) = nominal_boundaries.get(name.as_str()) {
        if !api_audience_is_contained(exposed, referenced) {
            diagnostics.push(format!(
                "{source_path}: error: {description} with {} visibility exposes {} effect `{name}` beyond its access boundary{}",
                visibility_description(exposed.visibility),
                visibility_description(referenced.visibility),
                boundary_location(referenced),
            ));
        }
    }
    for argument in positional.into_iter().flatten() {
        validate_exposed_type(
            argument,
            exposed,
            source_path,
            bound_types,
            description,
            nominal_boundaries,
            diagnostics,
        );
    }
    for argument in labeled.into_iter().flatten() {
        validate_exposed_type(
            &argument.ty,
            exposed,
            source_path,
            bound_types,
            description,
            nominal_boundaries,
            diagnostics,
        );
    }
}

fn effective_api_boundary(owner: &ApiBoundary, member_visibility: Visibility) -> ApiBoundary {
    let visibility = if visibility_rank(owner.visibility) <= visibility_rank(member_visibility) {
        owner.visibility
    } else {
        member_visibility
    };
    ApiBoundary {
        visibility,
        origin: owner.origin.clone(),
    }
}

fn api_audience_is_contained(exposed: &ApiBoundary, referenced: &ApiBoundary) -> bool {
    match referenced.visibility {
        Visibility::Public => true,
        Visibility::Package => {
            exposed.visibility != Visibility::Public
                && exposed.origin.package == referenced.origin.package
        }
        Visibility::Private => {
            (exposed.visibility == Visibility::Private
                && exposed.origin.package == referenced.origin.package
                && exposed
                    .origin
                    .module_path
                    .starts_with(&referenced.origin.module_path))
                || (exposed.visibility == Visibility::Package
                    && exposed.origin.package == referenced.origin.package
                    && referenced.origin.module_path.is_empty())
        }
    }
}

fn is_bound_api_type(name: &str, bound_types: &HashSet<String>) -> bool {
    bound_types.iter().any(|bound| {
        name == bound
            || name
                .strip_prefix(bound)
                .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

fn boundary_location(boundary: &ApiBoundary) -> String {
    match boundary.visibility {
        Visibility::Public => String::new(),
        Visibility::Package => format!(" in package #{}", boundary.origin.package),
        Visibility::Private => format!(
            " in module `{}` of package #{}",
            display_module(&boundary.origin.module_path),
            boundary.origin.package
        ),
    }
}

fn declaration_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(function) => Some(&function.name),
        Item::Global(binding) => Some(&binding.name),
        Item::Struct(definition) => Some(&definition.name),
        Item::Enum(definition) => Some(&definition.name),
        Item::Trait(definition) => Some(&definition.name),
        Item::Effect(definition) => Some(&definition.name),
        Item::Access(definition) => Some(&definition.name),
        Item::TypeAlias(definition) => Some(&definition.name),
        Item::Extend(_) => None,
    }
}

fn declaration_namespace(item: &Item) -> DeclarationNamespace {
    match item {
        Item::Function(_) => DeclarationNamespace::Function,
        Item::Struct(_) | Item::Enum(_) | Item::TypeAlias(_) => DeclarationNamespace::Type,
        Item::Global(_) | Item::Trait(_) | Item::Effect(_) | Item::Access(_) | Item::Extend(_) => {
            DeclarationNamespace::Other
        }
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
    package_root: &'a [String],
}

struct Resolver {
    symbols: SymbolTable,
    module_paths: HashSet<Vec<String>>,
    aliases: AliasTable,
    dependencies: DependencyTable,
    required_imports: HashMap<String, String>,
    diagnostics: Vec<String>,
}

#[derive(Clone)]
struct NameReference {
    canonical: String,
    accesses: Vec<(Visibility, Vec<String>, Vec<String>)>,
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
            Item::TypeAlias(definition) => {
                definition.name = canonical_name(context.module_path, &definition.name);
                let type_scope =
                    compile_parameter_names(&definition.compile_groups, &HashSet::new());
                self.rewrite_type(&mut definition.target, context, &type_scope);
            }
            Item::Struct(definition) => self.rewrite_struct(definition, context),
            Item::Enum(definition) => self.rewrite_enum(definition, context),
            Item::Trait(definition) => self.rewrite_trait(definition, context),
            Item::Effect(definition) => {
                definition.name = canonical_name(context.module_path, &definition.name);
                let type_scope =
                    compile_parameter_names(&definition.compile_groups, &HashSet::new());
                for operation in &mut definition.operations {
                    self.rewrite_function(operation, context, &type_scope);
                }
            }
            Item::Access(definition) => {
                definition.name = canonical_name(context.module_path, &definition.name);
            }
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
        for predicate in &mut definition.where_predicates {
            self.rewrite_type(&mut predicate.subject, context, &trait_types);
            self.rewrite_type(&mut predicate.trait_ref, context, &trait_types);
            for binding in &mut predicate.associated_types {
                self.rewrite_type(&mut binding.ty, context, &trait_types);
            }
        }
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
        let header_type_scope = compile_parameter_names(&extension.compile_groups, &HashSet::new());
        self.rewrite_type(&mut extension.target, context, &header_type_scope);
        if let Some(trait_ref) = &mut extension.trait_ref {
            self.rewrite_type(trait_ref, context, &header_type_scope);
        }
        for predicate in &mut extension.where_predicates {
            self.rewrite_type(&mut predicate.subject, context, &header_type_scope);
            self.rewrite_type(&mut predicate.trait_ref, context, &header_type_scope);
            for binding in &mut predicate.associated_types {
                self.rewrite_type(&mut binding.ty, context, &header_type_scope);
            }
        }

        let mut member_type_scope = header_type_scope;
        member_type_scope.insert("Self".to_owned());
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
        value_scope.extend(
            function
                .compile_groups
                .iter()
                .flatten()
                .map(|parameter| parameter.name.clone()),
        );
        for group in &mut function.groups {
            for parameter in group {
                self.rewrite_parameter(parameter, context, &type_scope);
                value_scope.insert(parameter.name.clone());
            }
        }
        if let Some(return_type) = &mut function.return_type {
            self.rewrite_type(return_type, context, &type_scope);
        }
        if let Some(error) = &mut function.effects.throws {
            self.rewrite_type(error, context, &type_scope);
        }
        for effect in &mut function.effects.custom {
            self.rewrite_type(effect, context, &type_scope);
        }
        for predicate in &mut function.where_predicates {
            self.rewrite_type(&mut predicate.subject, context, &type_scope);
            self.rewrite_type(&mut predicate.trait_ref, context, &type_scope);
            for binding in &mut predicate.associated_types {
                self.rewrite_type(&mut binding.ty, context, &type_scope);
            }
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
            Type::Borrow { pointee, .. } => self.rewrite_type(pointee, context, type_scope),
            Type::Array(element, _) => self.rewrite_type(element, context, type_scope),
            Type::Function {
                groups,
                effects,
                result,
            } => {
                for ty in groups.iter_mut().flatten() {
                    self.rewrite_type(ty, context, type_scope);
                }
                if let Some(error) = &mut effects.throws {
                    self.rewrite_type(error, context, type_scope);
                }
                for effect in &mut effects.custom {
                    self.rewrite_type(effect, context, type_scope);
                }
                self.rewrite_type(result, context, type_scope);
            }
            Type::Named(name, arguments) => {
                for argument in arguments {
                    self.rewrite_type(argument, context, type_scope);
                }
                let segments: Vec<String> = name.split('.').map(str::to_owned).collect();
                if segments
                    .first()
                    .is_some_and(|first| type_scope.contains(first))
                {
                    return;
                }
                if let Some(canonical) = self.resolve_logical_path(&segments, context) {
                    *name = canonical;
                } else {
                    if !self.reject_unimported_standard(&segments, context) {
                        self.reject_bare_module(&segments, context, "a type");
                    }
                }
            }
            Type::NamedArgs(name, arguments) => {
                for argument in arguments {
                    self.rewrite_type(&mut argument.ty, context, type_scope);
                }
                let segments: Vec<String> = name.split('.').map(str::to_owned).collect();
                if segments
                    .first()
                    .is_some_and(|first| type_scope.contains(first))
                {
                    return;
                }
                if let Some(canonical) = self.resolve_logical_path(&segments, context) {
                    *name = canonical;
                } else {
                    if !self.reject_unimported_standard(&segments, context) {
                        self.reject_bare_module(&segments, context, "a type");
                    }
                }
            }
            Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => {}
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
                    } else {
                        if !self.reject_unimported_standard(&logical, context) {
                            self.reject_bare_module(&logical, context, "a value or callable");
                        }
                    }
                }
            }
            Expr::Unary(_, operand)
            | Expr::Try(operand)
            | Expr::Throw(operand)
            | Expr::Unsafe(operand)
            | Expr::Borrow { value: operand, .. } => {
                self.rewrite_expr(operand, context, type_scope, value_scope);
            }
            Expr::DoBlock { body } => {
                self.rewrite_expr(body, context, type_scope, value_scope);
            }
            Expr::Binary(left, _, right)
            | Expr::Coalesce(left, right)
            | Expr::Assign(left, right)
            | Expr::CompoundAssign(left, _, right) => {
                self.rewrite_expr(left, context, type_scope, value_scope);
                self.rewrite_expr(right, context, type_scope, value_scope);
            }
            Expr::HandlerCoalesce {
                scrutinee,
                payload,
                success,
                fallback,
            } => {
                self.rewrite_expr(scrutinee, context, type_scope, value_scope);
                let mut success_scope = value_scope.clone();
                success_scope.insert(payload.clone());
                self.rewrite_expr(success, context, type_scope, &success_scope);
                self.rewrite_expr(fallback, context, type_scope, value_scope);
            }
            Expr::HandlerChainCall(chain) => {
                self.rewrite_expr(&mut chain.scrutinee, context, type_scope, value_scope);
                for argument in chain.groups.iter_mut().flatten() {
                    self.rewrite_expr(&mut argument.value, context, type_scope, value_scope);
                }
                let mut success_scope = value_scope.clone();
                success_scope.insert(chain.payload.clone());
                self.rewrite_expr(&mut chain.success, context, type_scope, &success_scope);
                let mut residual_scope = value_scope.clone();
                residual_scope.insert(chain.error.clone());
                self.rewrite_expr(&mut chain.residual, context, type_scope, &residual_scope);
            }
            Expr::Call(callee, arguments) => {
                self.rewrite_expr(callee, context, type_scope, value_scope);
                for argument in arguments {
                    self.rewrite_expr(&mut argument.value, context, type_scope, value_scope);
                }
            }
            Expr::StructLiteral {
                constructor,
                fields,
            } => {
                self.rewrite_compile_argument_expr(constructor, context, type_scope);
                for field in fields {
                    self.rewrite_expr(&mut field.value, context, type_scope, value_scope);
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
            Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Continue => {}
        }
    }

    fn rewrite_compile_argument_expr(
        &mut self,
        expression: &mut Expr,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
    ) {
        match expression {
            Expr::Name(name) => {
                if type_scope.contains(name) || compile_argument_name_is_builtin(name) {
                    return;
                }
                let logical = vec![name.clone()];
                if let Some(canonical) = self.resolve_logical_path(&logical, context) {
                    *name = canonical;
                } else if !self.reject_unimported_standard(&logical, context) {
                    self.reject_bare_module(&logical, context, "a type or compile-time argument");
                }
            }
            Expr::Call(callee, arguments) => {
                self.rewrite_compile_argument_expr(callee, context, type_scope);
                for argument in arguments {
                    self.rewrite_compile_argument_expr(&mut argument.value, context, type_scope);
                }
            }
            Expr::Member(_, _) => {
                self.rewrite_compile_argument_member_chain(expression, context, type_scope);
            }
            Expr::Unit | Expr::Integer(_) | Expr::Bool(_) => {}
            other => self.rewrite_expr(other, context, type_scope, &HashSet::new()),
        }
    }

    fn rewrite_compile_argument_member_chain(
        &mut self,
        expression: &mut Expr,
        context: ResolveContext<'_>,
        type_scope: &HashSet<String>,
    ) {
        let mut segments = Vec::new();
        if collect_member_segments(expression, &mut segments)
            && !segments
                .first()
                .is_some_and(|first| type_scope.contains(first))
        {
            if let Some((canonical, consumed)) = self.resolve_longest_prefix(&segments, context) {
                let mut resolved = Expr::Name(canonical);
                for member in &segments[consumed..] {
                    resolved = Expr::Member(Box::new(resolved), member.clone());
                }
                *expression = resolved;
                return;
            }
            if self.reject_unimported_standard(&segments, context) {
                return;
            }
            self.reject_bare_module(&segments, context, "a type or compile-time argument");
            return;
        }
        if let Expr::Member(base, _) = expression {
            self.rewrite_compile_argument_expr(base, context, type_scope);
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
                    } else {
                        if !self.reject_unimported_standard(path, context) {
                            self.reject_bare_module(path, context, "a constructor");
                        }
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

            if self.reject_unimported_standard(&segments, context) {
                return;
            }

            // Preserve the complete qualified spelling of an unknown name
            // under a known module. A plain Member tree would otherwise be
            // interpreted as a method receiver by the current analyzer and
            // lose its module prefix in the eventual diagnostic.
            let module_prefix = self.longest_module_prefix(&segments, context);
            if module_prefix > 0 {
                if module_prefix == segments.len() {
                    self.reject_bare_module(&segments, context, "a value or callable");
                }
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
        let reference = self.find_name(logical_path, context)?;
        for (visibility, module_path, package_root) in &reference.accesses {
            if !visibility_allows_access(
                *visibility,
                module_path,
                package_root,
                context.module_path,
                context.package_root,
            ) {
                let message = match visibility {
                    Visibility::Private => format!(
                        "`{}` is private to module `{}`",
                        logical_path.join("."),
                        display_module(module_path)
                    ),
                    Visibility::Package => format!(
                        "`{}` is pub(package) and cannot be used from another package",
                        logical_path.join(".")
                    ),
                    Visibility::Public => unreachable!("public names are always accessible"),
                };
                self.diagnostics
                    .push(format!("{}: error: {message}", context.source_path));
            }
        }
        Some(reference.canonical)
    }

    fn find_name(
        &self,
        logical_path: &[String],
        context: ResolveContext<'_>,
    ) -> Option<NameReference> {
        let candidates =
            anchored_candidates(logical_path, context.module_path, context.package_root).ok()?;
        for candidate in candidates {
            if let Some(reference) = self.find_absolute_name(&candidate, 0) {
                return Some(reference);
            }
            if let Some(expanded) =
                expand_dependency_candidate(&candidate, context.package_root, &self.dependencies)
            {
                if let Some(reference) = self.find_absolute_name(&expanded, 0) {
                    return Some(reference);
                }
            }
        }
        None
    }

    fn find_absolute_name(&self, candidate: &[String], depth: usize) -> Option<NameReference> {
        if depth > self.aliases.len() + 1 {
            return None;
        }
        if let Some(symbol) = self.symbols.get(candidate) {
            return Some(NameReference {
                canonical: symbol.canonical.clone(),
                accesses: vec![(
                    symbol.visibility,
                    symbol.module_path.clone(),
                    symbol.package_root.clone(),
                )],
            });
        }
        if let Some(alias) = self.aliases.get(candidate) {
            if let AliasTarget::Declaration(canonical) = &alias.target {
                return Some(NameReference {
                    canonical: canonical.clone(),
                    accesses: vec![(
                        alias.visibility,
                        alias.module_path.clone(),
                        alias.package_root.clone(),
                    )],
                });
            }
        }
        for length in (1..candidate.len()).rev() {
            let prefix = &candidate[..length];
            let Some(alias) = self.aliases.get(prefix) else {
                continue;
            };
            let AliasTarget::Module(module) = &alias.target else {
                continue;
            };
            let mut expanded = module.clone();
            expanded.extend_from_slice(&candidate[length..]);
            if let Some(mut reference) = self.find_absolute_name(&expanded, depth + 1) {
                reference.accesses.push((
                    alias.visibility,
                    alias.module_path.clone(),
                    alias.package_root.clone(),
                ));
                return Some(reference);
            }
        }
        None
    }

    fn longest_module_prefix(&self, segments: &[String], context: ResolveContext<'_>) -> usize {
        for length in (1..=segments.len()).rev() {
            let Ok(candidates) = anchored_candidates(
                &segments[..length],
                context.module_path,
                context.package_root,
            ) else {
                continue;
            };
            for candidate in candidates {
                if self.module_paths.contains(&candidate) {
                    return length;
                }
                if self
                    .aliases
                    .get(&candidate)
                    .is_some_and(|alias| matches!(alias.target, AliasTarget::Module(_)))
                {
                    return length;
                }
                if let Some(expanded) = expand_dependency_candidate(
                    &candidate,
                    context.package_root,
                    &self.dependencies,
                ) {
                    if self.module_paths.contains(&expanded)
                        || self
                            .aliases
                            .get(&expanded)
                            .is_some_and(|alias| matches!(alias.target, AliasTarget::Module(_)))
                    {
                        return length;
                    }
                }
            }
        }
        0
    }

    fn reject_bare_module(
        &mut self,
        logical_path: &[String],
        context: ResolveContext<'_>,
        usage: &str,
    ) {
        if !logical_path.is_empty()
            && self.longest_module_prefix(logical_path, context) == logical_path.len()
        {
            self.diagnostics.push(format!(
                "{}: error: module `{}` cannot be used as {usage}",
                context.source_path,
                logical_path.join(".")
            ));
        }
    }

    fn reject_unimported_standard(
        &mut self,
        logical_path: &[String],
        context: ResolveContext<'_>,
    ) -> bool {
        if self.longest_module_prefix(logical_path, context) > 0 {
            return false;
        }
        let Some(name) = logical_path.first() else {
            return false;
        };
        let Some(import_path) = self.required_imports.get(name) else {
            return false;
        };
        self.diagnostics.push(format!(
            "{}: error: standard-library item `{name}` is not in the prelude; import it with `use {import_path}`",
            context.source_path
        ));
        true
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
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    CompileParamKind::Type
                        | CompileParamKind::TypeConstructor { .. }
                        | CompileParamKind::EffectConstructor { .. }
                )
            })
            .map(|parameter| parameter.name.clone()),
    );
    names
}

fn compile_argument_name_is_builtin(name: &str) -> bool {
    matches!(
        name,
        "i32"
            | "i64"
            | "u32"
            | "u64"
            | "bool"
            | "shared"
            | "mut"
            | "auto"
            | "copy"
            | "move"
            | "pure"
    )
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

    fn package(
        id: usize,
        is_primary: bool,
        dependencies: &[(&str, usize)],
        sources: Vec<SourceUnit>,
    ) -> SourcePackage {
        SourcePackage {
            id: PackageId(id),
            is_primary,
            dependencies: dependencies
                .iter()
                .map(|(alias, target)| ((*alias).to_owned(), PackageId(*target)))
                .collect(),
            sources,
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

    fn function_tail(function: &Function) -> &Expr {
        let Some(Expr::Block(_, Some(tail))) = &function.body else {
            panic!("expected function body block with a tail value");
        };
        tail
    }

    #[test]
    fn flattens_modules_and_rewrites_calls_and_dotted_types() {
        let program = resolve_sources(&[
            unit(
                "src/main.sc",
                &[],
                "let main(): geometry.Point = { geometry.make() }\n",
                true,
            ),
            unit(
                "src/geometry.sc",
                &["geometry"],
                "pub(package) let Point = struct { x: i32, y: i32 }\n\
                 pub(package) let make(): Point = { Point { x: 1, y: 2 } }\n",
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
            function_tail(main),
            Expr::Call(callee, arguments)
                if arguments.is_empty()
                    && callee.as_ref() == &Expr::Name("geometry::make".into())
        ));

        let make = function(&program, "geometry::make");
        assert_eq!(
            make.return_type,
            Some(Type::Named("geometry::Point".into(), Vec::new()))
        );
        assert!(matches!(
            function_tail(make),
            Expr::StructLiteral { constructor, .. }
                if constructor.as_ref() == &Expr::Name("geometry::Point".into())
        ));
    }

    #[test]
    fn resolves_longest_declaration_prefix_and_preserves_fields() {
        let program = resolve_sources(&[
            unit(
                "src/main.sc",
                &[],
                "let main(): i32 = { data.origin.x }\n",
                true,
            ),
            unit(
                "src/data.sc",
                &["data"],
                "pub(package) let Point = struct { pub(package) x: i32 }\n\
                 pub(package) let origin = Point { x: 1 }\n",
                false,
            ),
        ])
        .unwrap();

        assert!(matches!(
            function_tail(function(&program, "main")),
            Expr::Member(base, field)
                if field == "x" && base.as_ref() == &Expr::Name("data::origin".into())
        ));
    }

    #[test]
    fn local_parameters_blocks_closures_and_match_bindings_shadow_modules() {
        let program = resolve_sources(&[
            unit(
                "src/main.sc",
                &[],
                "let keep(math: i32): i32 = {\n\
                   let local = { (math: i32) -> math }\n\
                   Option.Some(math) match { Option.Some(math) => local(math), _ => math }\n\
                 }\n",
                true,
            ),
            unit(
                "src/math.sc",
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
            unit("src/main.sc", &[], "let main(): i32 = { b.read() }\n", true),
            unit("src/a.sc", &["a"], "let secret(): i32 = { 1 }\n", false),
            unit(
                "src/a/child.sc",
                &["a", "child"],
                "pub(package) let read(): i32 = { secret() }\n",
                false,
            ),
            unit(
                "src/b.sc",
                &["b"],
                "pub(package) let read(): i32 = { a.secret() }\n",
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
            unit("src/main.sc", &[], "let main(): i32 = { 0 }\n", true),
            unit(
                "src/api.sc",
                &["api"],
                "pub(package) let Self = struct { value: i32 }\n\
                 pub(package) let Output = struct { value: i32 }\n\
                 pub(package) let A = struct { value: i32 }\n\
                 pub(package) let Convert = trait {\n\
                   let Output: type\n\
                   let A: type\n\
                   let B: type\n\
                   let convert(borrow self)(value: Self): Output\n\
                 }\n\
                 pub(package) let Number = struct { value: i32 }\n\
                 extend Number: Convert {\n\
                   let Output = i32\n\
                   let A = Self\n\
                   let B = A\n\
                   let convert(borrow self)(value: Self): Output = { value.value }\n}\n",
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
    fn preserves_generic_extend_parameters_while_qualifying_the_target() {
        let program = resolve_sources(&[
            unit(
                "src/main.sc",
                &[],
                "let main(): i32 = { api.Cell.new(42).take() }\n",
                true,
            ),
            unit(
                "src/api.sc",
                &["api"],
                "pub(package) let Cell (T: type) = struct { value: T }\n\
                 extend(T: type) Cell(T) {\n\
                   let new(move value: T): Cell(T) = { Cell { value: value } }\n\
                   let take(move self)(): T = { self.value }\n\
                 }\n",
                false,
            ),
        ])
        .unwrap();

        let extension = program
            .items
            .iter()
            .find_map(|item| match item {
                Item::Extend(extension) if !extension.compile_groups.is_empty() => Some(extension),
                _ => None,
            })
            .expect("missing generic extension");
        assert_eq!(extension.compile_groups[0][0].name, "T");
        assert_eq!(
            extension.target,
            Type::Named(
                "api::Cell".into(),
                vec![Type::Named("T".into(), Vec::new())]
            )
        );
        let ExtendMember::Function(new) = &extension.members[0] else {
            panic!("missing associated constructor");
        };
        assert_eq!(
            new.return_type,
            Some(Type::Named(
                "api::Cell".into(),
                vec![Type::Named("T".into(), Vec::new())]
            ))
        );
    }

    #[test]
    fn leaves_unknown_names_for_semantic_analysis() {
        let program = resolve_sources(&[unit(
            "src/main.sc",
            &[],
            "let main(): i32 = { missing.value() }\n",
            true,
        )])
        .unwrap();

        assert!(matches!(
            function_tail(function(&program, "main")),
            Expr::Call(callee, _)
                if matches!(callee.as_ref(), Expr::Member(base, field)
                    if base.as_ref() == &Expr::Name("missing".into()) && field == "value")
        ));
    }

    #[test]
    fn rejects_duplicate_modules_declarations_and_module_name_conflicts() {
        let duplicate_module = resolve_sources(&[
            unit("root.sc", &[], "let main() = {}\n", true),
            unit("one.sc", &["net"], "let one = 1\n", false),
            unit("two.sc", &["net"], "let two = 2\n", false),
        ])
        .unwrap_err();
        assert!(duplicate_module
            .iter()
            .any(|diagnostic| diagnostic.contains("duplicate module `net`")));

        let duplicate_declaration =
            resolve_sources(&[unit("root.sc", &[], "let value = 1\nlet value = 2\n", true)])
                .unwrap_err();
        assert!(duplicate_declaration
            .iter()
            .any(|diagnostic| diagnostic.contains("duplicate declaration `value`")));

        let conflict = resolve_sources(&[
            unit("root.sc", &[], "let net = 1\n", true),
            unit("net.sc", &["net"], "let value = 2\n", false),
        ])
        .unwrap_err();
        assert!(conflict
            .iter()
            .any(|diagnostic| diagnostic.contains("conflicts with child module")));
    }

    #[test]
    fn requires_exactly_one_root_source() {
        let no_root =
            resolve_sources(&[unit("a.sc", &["a"], "let value = 1\n", false)]).unwrap_err();
        assert!(no_root[0].contains("exactly one root source"));

        let two_roots = resolve_sources(&[
            unit("a.sc", &[], "let a = 1\n", true),
            unit("b.sc", &["b"], "let b = 2\n", true),
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
                unit("root.sc", &[], "let main(): i32 = { 0 }\n", true),
                unit("bad.sc", &[segment], "let value = 1\n", false),
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

    #[test]
    fn resolves_forward_aliases_module_aliases_and_reexport_chains() {
        let program = resolve_sources(&[
            unit(
                "root.sc",
                &[],
                "use root.facade.answer as selected\nlet main(): i32 = { selected() }\n",
                true,
            ),
            unit(
                "facade.sc",
                &["facade"],
                "pub use root.implementation.answer\n",
                false,
            ),
            unit(
                "implementation.sc",
                &["implementation"],
                "pub let answer(): i32 = { 42 }\n",
                false,
            ),
        ])
        .unwrap();

        assert!(program.uses.is_empty());
        assert!(matches!(
            function_tail(function(&program, "main")),
            Expr::Call(callee, _)
                if callee.as_ref() == &Expr::Name("implementation::answer".into())
        ));
    }

    #[test]
    fn rejects_bare_modules_before_they_can_fall_back_to_prelude_names() {
        let errors = resolve_sources(&[
            unit(
                "root.sc",
                &[],
                r#"use root.fake as Option
use root.fake as Add
use root.fake as Never

let Number = struct { value: i32 }
extend Number: Add(Number) {
  let Output = i32
  let add(move self)(move rhs: Number): i32 = { self.value + rhs.value }
}

let stop(): Never = { loop {} }
let main(): i32 = { Option {} }
"#,
                true,
            ),
            unit("fake.sc", &["fake"], "let marker = 0\n", false),
        ])
        .unwrap_err();

        for expected in [
            "module `Option` cannot be used as a type or compile-time argument",
            "module `Add` cannot be used as a type",
            "module `Never` cannot be used as a type",
        ] {
            assert!(
                errors
                    .iter()
                    .any(|diagnostic| diagnostic.contains(expected)),
                "missing `{expected}` in {errors:?}"
            );
        }
    }

    #[test]
    fn resolves_root_self_super_and_anchor_only_module_aliases() {
        let program = resolve_sources(&[
            unit(
                "root.sc",
                &[],
                "let root_value(): i32 = { 10 }\nlet main(): i32 = { nested.deep.answer() }\n",
                true,
            ),
            unit(
                "nested.sc",
                &["nested"],
                "let parent_value(): i32 = { 20 }\n",
                false,
            ),
            unit(
                "nested/deep.sc",
                &["nested", "deep"],
                "use root as pkg\n\
                 use self.local_value as local\n\
                 use super.parent_value as parent\n\
                 let local_value(): i32 = { 12 }\n\
                 pub(package) let answer(): i32 = { pkg.root_value() + parent() + local() }\n",
                false,
            ),
        ])
        .unwrap();

        let names = expression_names(function(&program, "nested::deep::answer").body.as_ref());
        assert!(names.contains("root_value"));
        assert!(names.contains("nested::parent_value"));
        assert!(names.contains("nested::deep::local_value"));
    }

    #[test]
    fn rejects_import_cycles_privacy_and_visibility_escalation() {
        let cycle = resolve_sources(&[unit(
            "root.sc",
            &[],
            "use root.second as first\nuse root.first as second\nlet main(): i32 = { 0 }\n",
            true,
        )])
        .unwrap_err();
        assert!(cycle.iter().any(|diagnostic| {
            diagnostic.contains("cyclic import aliases")
                && diagnostic.contains("first")
                && diagnostic.contains("second")
        }));

        let private = resolve_sources(&[
            unit(
                "root.sc",
                &[],
                "use root.sibling.secret\nlet main(): i32 = { secret() }\n",
                true,
            ),
            unit(
                "sibling.sc",
                &["sibling"],
                "let secret(): i32 = { 1 }\n",
                false,
            ),
        ])
        .unwrap_err();
        assert!(private.iter().any(|diagnostic| {
            diagnostic.contains("private") && diagnostic.contains("sibling.secret")
        }));

        let promotion = resolve_sources(&[
            unit("root.sc", &[], "let main(): i32 = { 0 }\n", true),
            unit(
                "facade.sc",
                &["facade"],
                "pub(package) let internal(): i32 = { 1 }\npub use self.internal as exposed\n",
                false,
            ),
        ])
        .unwrap_err();
        assert!(promotion.iter().any(|diagnostic| {
            diagnostic.contains("pub use")
                && diagnostic.contains("pub(package)")
                && diagnostic.contains("facade.internal")
        }));
    }

    #[test]
    fn preserves_private_module_alias_boundaries_and_skips_relative_self_aliases() {
        let program = resolve_sources(&[
            unit(
                "root.sc",
                &[],
                "pub(package) let answer(): i32 = { 42 }\nlet main(): i32 = { child.read() }\n",
                true,
            ),
            unit(
                "child.sc",
                &["child"],
                "use answer\npub(package) let read(): i32 = { answer() }\n",
                false,
            ),
        ])
        .unwrap();
        assert!(matches!(
            function_tail(function(&program, "child::read")),
            Expr::Call(callee, _)
                if callee.as_ref() == &Expr::Name("answer".into())
        ));

        let bypass = resolve_sources(&[
            unit("root.sc", &[], "let main(): i32 = { 0 }\n", true),
            unit(
                "net.sc",
                &["net"],
                "pub(package) let value(): i32 = { 1 }\n",
                false,
            ),
            unit(
                "owner.sc",
                &["owner"],
                "use root.net as hidden\nlet local(): i32 = { hidden.value() }\n",
                false,
            ),
            unit(
                "sibling.sc",
                &["sibling"],
                "use root.owner.hidden.value as stolen\nlet read(): i32 = { stolen() }\n",
                false,
            ),
        ])
        .unwrap_err();
        assert!(bypass.iter().any(|diagnostic| {
            diagnostic.contains("private") && diagnostic.contains("owner.hidden")
        }));

        let unknown = resolve_sources(&[unit(
            "root.sc",
            &[],
            "use missing as value\nlet main(): i32 = { 0 }\n",
            true,
        )])
        .unwrap_err();
        assert_eq!(
            unknown
                .iter()
                .filter(|diagnostic| diagnostic.contains("unknown import"))
                .count(),
            1,
            "{unknown:?}"
        );
    }

    #[test]
    fn resolves_dependency_packages_with_independent_roots() {
        let program = resolve_packages(&[
            package(
                0,
                true,
                &[("math", 1)],
                vec![unit(
                    "app/src/main.sc",
                    &[],
                    "use math.answer as selected\nlet main(): i32 = { selected() }\n",
                    true,
                )],
            ),
            package(
                1,
                false,
                &[],
                vec![
                    unit("math/src/lib.sc", &[], "pub use root.inner.answer\n", true),
                    unit(
                        "math/src/inner.sc",
                        &["inner"],
                        "pub let answer(): i32 = { 42 }\n",
                        false,
                    ),
                ],
            ),
        ])
        .unwrap();

        assert!(function(&program, "@1::inner::answer").body.is_some());
        assert!(matches!(
            function_tail(function(&program, "main")),
            Expr::Call(callee, _)
                if callee.as_ref() == &Expr::Name("@1::inner::answer".into())
        ));
    }

    #[test]
    fn enforces_package_visibility_across_dependency_boundaries() {
        for (visibility, expected) in [("pub(package)", "pub(package)"), ("", "private")] {
            let error = resolve_packages(&[
                package(
                    0,
                    true,
                    &[("dep", 1)],
                    vec![unit(
                        "app/src/main.sc",
                        &[],
                        "let main(): i32 = { dep.hidden() }\n",
                        true,
                    )],
                ),
                package(
                    1,
                    false,
                    &[],
                    vec![unit(
                        "dep/src/lib.sc",
                        &[],
                        &format!("{visibility} let hidden(): i32 = {{ 1 }}\n"),
                        true,
                    )],
                ),
            ])
            .unwrap_err();
            assert!(
                error.iter().any(|diagnostic| diagnostic.contains(expected)),
                "{error:?}"
            );
        }
    }

    #[test]
    fn prevents_dependency_anchors_from_escaping_their_package() {
        let root_lookup = resolve_packages(&[
            package(
                0,
                true,
                &[("dep", 1)],
                vec![unit(
                    "app/src/main.sc",
                    &[],
                    "pub let root_value(): i32 = { 1 }\nlet main(): i32 = { 0 }\n",
                    true,
                )],
            ),
            package(
                1,
                false,
                &[],
                vec![unit(
                    "dep/src/lib.sc",
                    &[],
                    "use root.root_value\npub let answer(): i32 = { root_value() }\n",
                    true,
                )],
            ),
        ])
        .unwrap_err();
        assert!(
            root_lookup.iter().any(|diagnostic| {
                diagnostic.contains("unknown import") && diagnostic.contains("root.root_value")
            }),
            "{root_lookup:?}"
        );

        let super_escape = resolve_packages(&[
            package(
                0,
                true,
                &[("dep", 1)],
                vec![unit(
                    "app/src/main.sc",
                    &[],
                    "let main(): i32 = { 0 }\n",
                    true,
                )],
            ),
            package(
                1,
                false,
                &[],
                vec![unit(
                    "dep/src/lib.sc",
                    &[],
                    "use super.outside as value\npub let answer(): i32 = { 0 }\n",
                    true,
                )],
            ),
        ])
        .unwrap_err();
        assert!(super_escape
            .iter()
            .any(|diagnostic| diagnostic.contains("escapes above the package root")));
    }

    #[test]
    fn rejects_dependency_aliases_that_conflict_with_file_modules() {
        let error = resolve_packages(&[
            package(
                0,
                true,
                &[("dep", 1)],
                vec![
                    unit("app/src/main.sc", &[], "let main(): i32 = { 0 }\n", true),
                    unit(
                        "app/src/dep/internal.sc",
                        &["dep", "internal"],
                        "pub(package) let local = 1\n",
                        false,
                    ),
                ],
            ),
            package(
                1,
                false,
                &[],
                vec![unit(
                    "dep/src/lib.sc",
                    &[],
                    "pub let answer(): i32 = { 1 }\n",
                    true,
                )],
            ),
        ])
        .unwrap_err();
        assert!(error
            .iter()
            .any(|diagnostic| diagnostic.contains("module `dep`")
                && diagnostic.contains("dependency alias")));
    }

    #[test]
    fn hides_transitive_dependencies_until_the_owner_reexports_them() {
        let hidden = resolve_packages(&[
            package(
                0,
                true,
                &[("middle", 1)],
                vec![unit(
                    "app/src/main.sc",
                    &[],
                    "use middle.leaf.answer\nlet main(): i32 = { answer() }\n",
                    true,
                )],
            ),
            package(
                1,
                false,
                &[("leaf", 2)],
                vec![unit(
                    "middle/src/lib.sc",
                    &[],
                    "pub let middle_value(): i32 = { leaf.answer() }\n",
                    true,
                )],
            ),
            package(
                2,
                false,
                &[],
                vec![unit(
                    "leaf/src/lib.sc",
                    &[],
                    "pub let answer(): i32 = { 42 }\n",
                    true,
                )],
            ),
        ])
        .unwrap_err();
        assert!(hidden.iter().any(|diagnostic| {
            diagnostic.contains("unknown import") && diagnostic.contains("middle.leaf.answer")
        }));

        let exposed = resolve_packages(&[
            package(
                0,
                true,
                &[("middle", 1)],
                vec![unit(
                    "app/src/main.sc",
                    &[],
                    "use middle.answer as selected\nlet main(): i32 = { selected() }\n",
                    true,
                )],
            ),
            package(
                1,
                false,
                &[("leaf", 2)],
                vec![unit(
                    "middle/src/lib.sc",
                    &[],
                    "pub use leaf.answer\n",
                    true,
                )],
            ),
            package(
                2,
                false,
                &[],
                vec![unit(
                    "leaf/src/lib.sc",
                    &[],
                    "pub let answer(): i32 = { 42 }\n",
                    true,
                )],
            ),
        ])
        .unwrap();
        assert!(matches!(
            function_tail(function(&exposed, "main")),
            Expr::Call(callee, _)
                if callee.as_ref() == &Expr::Name("@2::answer".into())
        ));
    }

    #[test]
    fn shared_dependencies_keep_one_definition_identity() {
        let program = resolve_packages(&[
            package(
                0,
                true,
                &[("left", 1), ("right", 2)],
                vec![unit(
                    "app/src/main.sc",
                    &[],
                    "let same(value: left.Token): right.Token = { value }\nlet main(): i32 = { 0 }\n",
                    true,
                )],
            ),
            package(
                1,
                false,
                &[("shared", 3)],
                vec![unit("left/src/lib.sc", &[], "pub use shared.Token\n", true)],
            ),
            package(
                2,
                false,
                &[("shared", 3)],
                vec![unit(
                    "right/src/lib.sc",
                    &[],
                    "pub use shared.Token\n",
                    true,
                )],
            ),
            package(
                3,
                false,
                &[],
                vec![unit(
                    "shared/src/lib.sc",
                    &[],
                    "pub let Token = struct { value: i32 }\n",
                    true,
                )],
            ),
        ])
        .unwrap();

        let same = function(&program, "same");
        assert_eq!(
            same.groups[0][0].ty,
            Type::Named("@3::Token".into(), vec![])
        );
        assert_eq!(
            same.return_type,
            Some(Type::Named("@3::Token".into(), vec![]))
        );
        assert_eq!(
            program
                .items
                .iter()
                .filter(|item| matches!(item, Item::Struct(definition) if definition.name == "@3::Token"))
                .count(),
            1
        );
    }

    #[test]
    fn dependency_aliases_conflict_with_root_declarations_and_imports() {
        for source in [
            "let dep = 1\nlet main(): i32 = { 0 }\n",
            "use root.answer as dep\nlet answer(): i32 = { 1 }\nlet main(): i32 = { 0 }\n",
        ] {
            let error = resolve_packages(&[
                package(
                    0,
                    true,
                    &[("dep", 1)],
                    vec![unit("app/src/main.sc", &[], source, true)],
                ),
                package(
                    1,
                    false,
                    &[],
                    vec![unit(
                        "dep/src/lib.sc",
                        &[],
                        "pub let answer(): i32 = { 1 }\n",
                        true,
                    )],
                ),
            ])
            .unwrap_err();
            assert!(error
                .iter()
                .any(|diagnostic| diagnostic.contains("dependency alias `dep`")));
        }
    }

    #[test]
    fn validates_public_package_graph_inputs() {
        let cycle = resolve_packages(&[
            package(
                0,
                true,
                &[("next", 1)],
                vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
            ),
            package(
                1,
                false,
                &[("back", 0)],
                vec![unit("next.sc", &[], "pub let value = 1\n", true)],
            ),
        ])
        .unwrap_err();
        assert!(cycle
            .iter()
            .any(|diagnostic| diagnostic.contains("cyclic package dependencies")));

        let invalid = resolve_packages(&[
            package(
                0,
                true,
                &[("missing", 99)],
                vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
            ),
            package(
                1,
                false,
                &[],
                vec![unit("orphan.sc", &[], "pub let value = 1\n", true)],
            ),
        ])
        .unwrap_err();
        assert!(invalid
            .iter()
            .any(|diagnostic| diagnostic.contains("unknown package ID 99")));
        assert!(invalid
            .iter()
            .any(|diagnostic| diagnostic.contains("not reachable")));

        let reserved = resolve_packages(&[package(
            PackageId::CORE.0,
            true,
            &[],
            vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
        )])
        .unwrap_err();
        assert!(reserved
            .iter()
            .any(|diagnostic| diagnostic.contains(&format!(
                "package ID {} is reserved for compiler core",
                PackageId::CORE.0
            ))));

        let reserved_alloc = resolve_packages(&[package(
            PackageId::ALLOC.0,
            true,
            &[],
            vec![unit("root.sc", &[], "let main(): i32 = { 0 }\n", true)],
        )])
        .unwrap_err();
        assert!(reserved_alloc
            .iter()
            .any(|diagnostic| diagnostic.contains(&format!(
                "package ID {} is reserved for compiler alloc",
                PackageId::ALLOC.0
            ))));
    }

    #[test]
    fn rejects_nominal_types_that_are_narrower_than_function_and_global_apis() {
        let errors = resolve_sources(&[unit(
            "src/lib.sc",
            &[],
            "let Hidden = struct {}\n\
             pub let Wrapper (T: type) = struct {}\n\
             pub let expose(value: Wrapper(Hidden)): Hidden = { value }\n\
             pub let shared: Hidden = Hidden {}\n",
            true,
        )])
        .unwrap_err();

        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("function `expose` parameter `value`")
                && diagnostic.contains("exposes private type `Hidden`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("function `expose` return type")
                && diagnostic.contains("exposes private type `Hidden`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("global `shared` type")
                && diagnostic.contains("exposes private type `Hidden`")
        }));
    }

    #[test]
    fn canonicalizes_qualified_custom_effects_across_modules() {
        let program = resolve_sources(&[
            unit(
                "src/main.sc",
                &[],
                "pub let screen(): i32 with(ui.UI) = { 0 }\n",
                true,
            ),
            unit("src/ui.sc", &["ui"], "pub let UI = effect\n", false),
        ])
        .unwrap();

        assert_eq!(
            function(&program, "screen").effects.custom,
            [Type::Named("ui::UI".into(), Vec::new())]
        );
    }

    #[test]
    fn rejects_private_effects_exposed_by_public_callable_apis() {
        let errors = resolve_sources(&[unit(
            "src/lib.sc",
            &[],
            "let UI = effect\n\
             pub let expose(action: (): i32 with(UI)): i32 with(UI) = { 0 }\n",
            true,
        )])
        .unwrap_err();

        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("function `expose` parameter `action`")
                && diagnostic.contains("exposes private effect `UI`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("function `expose` with public visibility")
                && diagnostic.contains("exposes private effect `UI`")
        }));
    }

    #[test]
    fn rejects_traits_that_are_narrower_than_public_where_predicates() {
        let errors = resolve_sources(&[unit(
            "src/lib.sc",
            &[],
            "let Hidden = trait {}\n\
             pub let expose(T: type)(value: T): T where T: Hidden = { value }\n",
            true,
        )])
        .unwrap_err();

        assert!(
            errors.iter().any(|diagnostic| {
                diagnostic.contains("where predicate 1 trait")
                    && diagnostic.contains("exposes private type `Hidden`")
            }),
            "{errors:?}"
        );
    }

    #[test]
    fn rejects_traits_that_are_narrower_than_constrained_extension_members() {
        let errors = resolve_sources(&[unit(
            "src/lib.sc",
            &[],
            "let Hidden = trait {}\n\
             pub let Cell (T: type) = struct { pub value: T }\n\
             extend(T: type) Cell(T) where T: Hidden {\n\
               let take(move self)(): T = { self.value }\n\
             }\n",
            true,
        )])
        .unwrap_err();

        assert!(
            errors.iter().any(|diagnostic| {
                diagnostic.contains("extension where predicate 1 trait")
                    && diagnostic.contains("exposes private type `Hidden`")
            }),
            "{errors:?}"
        );
    }

    #[test]
    fn compares_package_and_private_api_audiences_by_module_ancestry() {
        let errors = resolve_sources(&[
            unit(
                "src/main.sc",
                &[],
                "let RootSecret = struct {}\n\
                 pub(package) let PackageSecret = struct {}\n\
                 let main(): i32 = { 0 }\n",
                true,
            ),
            unit(
                "src/child.sc",
                &["child"],
                "let ChildSecret = struct {}\n\
                 pub(package) let package_ok(value: RootSecret): RootSecret = { value }\n\
                 let private_ok(value: RootSecret): RootSecret = { value }\n\
                 pub(package) let package_bad(value: ChildSecret): i32 = { 0 }\n\
                 pub let public_bad(value: PackageSecret): i32 = { 0 }\n",
                false,
            ),
        ])
        .unwrap_err();

        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("function `child::package_bad` parameter `value`")
                && diagnostic.contains("private type `child::ChildSecret`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("function `child::public_bad` parameter `value`")
                && diagnostic.contains("pub(package) type `PackageSecret`")
        }));
    }

    #[test]
    fn validates_effective_struct_and_enum_field_audiences() {
        let errors = resolve_sources(&[unit(
            "src/lib.sc",
            &[],
            "let Hidden = struct {}\n\
             pub let Record = struct { pub visible: Hidden, private: Hidden }\n\
             pub let Choice = enum {\n\
               Positional(Hidden),\n\
               Named(pub visible: Hidden, private: Hidden),\n\
             }\n",
            true,
        )])
        .unwrap_err();

        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("field `Record.visible`")
                && diagnostic.contains("private type `Hidden`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("enum variant `Choice.Positional` payload 0")
                && diagnostic.contains("private type `Hidden`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("enum variant field `Choice.Named.visible`")
                && diagnostic.contains("private type `Hidden`")
        }));
        assert!(errors
            .iter()
            .all(|diagnostic| !diagnostic.contains("Record.private")
                && !diagnostic.contains("Choice.Named.private")));
    }

    #[test]
    fn validates_trait_signatures_without_treating_bound_types_as_nominals() {
        let valid = resolve_sources(&[unit(
            "src/valid.sc",
            &[],
            "pub let Convert(T: type) = trait {\n\
               let Output: type = T\n\
               let convert(U: type)(borrow self)(value: T): Output\n\
             }\n",
            true,
        )]);
        assert!(valid.is_ok(), "{valid:?}");

        let errors = resolve_sources(&[unit(
            "src/invalid.sc",
            &[],
            "let Hidden = struct {}\n\
             pub let Expose = trait {\n\
               let Output: type = Hidden\n\
               let convert(borrow self)(value: Hidden): Hidden\n\
             }\n",
            true,
        )])
        .unwrap_err();

        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("associated type `Expose.Output` default")
                && diagnostic.contains("private type `Hidden`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("trait method `Expose.convert` parameter `value`")
                && diagnostic.contains("private type `Hidden`")
        }));
        assert!(errors.iter().any(|diagnostic| {
            diagnostic.contains("trait method `Expose.convert` return type")
                && diagnostic.contains("private type `Hidden`")
        }));
    }

    #[test]
    fn standard_library_modules_are_explicit_reserved_namespaces() {
        let program = resolve_sources(&[unit(
            "main.sc",
            &[],
            "use alloc.boxed.Box as HeapBox\nuse alloc.vec.Vec\n\
             let keep(move boxed: HeapBox(i32)): HeapBox(i32) = { boxed }\n\
             let empty(): Vec(i32) = { Vec(i32).new() }\n",
            true,
        )])
        .unwrap();
        assert_eq!(
            function(&program, "keep").groups[0][0].ty,
            Type::Named("alloc::boxed::Box".into(), vec![Type::I32])
        );
        assert_eq!(
            function(&program, "empty").return_type,
            Some(Type::Named("alloc::vec::Vec".into(), vec![Type::I32]))
        );

        let operator = resolve_sources(&[unit(
            "operator.sc",
            &[],
            "use core.ops.Add as Plus\n\
             let Number = struct { value: i32 }\n\
             extend Number: Plus(Number) {\n\
               let Output = Number\n\
               let add(move self)(move rhs: Number): Number = { Number { value: self.value + rhs.value } }\n\
             }\n",
            true,
        )])
        .unwrap();
        assert!(operator.items.iter().any(|item| {
            matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "core::ops::Add"))
        }));

        let standard_modules = resolve_sources(&[unit(
            "standard.sc",
            &[],
            "use core.effects.Async\n\
             use core.algebra.{Semigroup, Monoid}\n\
             let Number = struct { value: i32 }\n\
             let suspended(): i32 with(Async) = { 0 }\n\
             let invoke(move action: (): i32 with(Async)): i32 with(Async) = { action() }\n\
             extend Number: Semigroup {\n\
               let combine(move left: Number, move right: Number): Number = { Number { value: left.value + right.value } }\n}\n\
             extend Number: Monoid {\n\
               let empty(): Number = { Number { value: 0 } }\n}\n",
            true,
        )])
        .unwrap();
        assert_eq!(
            function(&standard_modules, "suspended").effects.custom,
            vec![Type::Named("core::effects::Async".into(), Vec::new())]
        );
        let invoke = function(&standard_modules, "invoke");
        let Type::Function { effects, .. } = &invoke.groups[0][0].ty else {
            panic!("expected function-typed action parameter");
        };
        assert_eq!(
            effects.custom,
            vec![Type::Named("core::effects::Async".into(), Vec::new())]
        );
        assert!(standard_modules.items.iter().any(|item| {
            matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "core::algebra::Semigroup"))
        }));
        assert!(standard_modules.items.iter().any(|item| {
            matches!(item, Item::Extend(extension)
                if matches!(&extension.trait_ref,
                    Some(Type::Named(name, _)) if name == "core::algebra::Monoid"))
        }));

        let bare = resolve_sources(&[unit(
            "main.sc",
            &[],
            "let make(): Box(i32) = { Box.new(1) }\n",
            true,
        )])
        .unwrap_err();
        assert!(bare.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `Box` is not in the prelude")
                && diagnostic.contains("use alloc.boxed.Box")
        }));

        let bare_operator = resolve_sources(&[unit(
            "operator.sc",
            &[],
            "let Number = struct { value: i32 }\n\
             extend Number: Add(Number) {\n\
               let Output = Number\n\
               let add(move self)(move rhs: Number): Number = { self }\n\
             }\n",
            true,
        )])
        .unwrap_err();
        assert!(bare_operator.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `Add` is not in the prelude")
                && diagnostic.contains("use core.ops.Add")
        }));

        let bare_effect = resolve_sources(&[unit(
            "effect.sc",
            &[],
            "let suspended(): i32 with(Async) = { 0 }\n",
            true,
        )])
        .unwrap_err();
        assert!(bare_effect.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `Async` is not in the prelude")
                && diagnostic.contains("use core.effects.Async")
        }));

        let bare_algebra = resolve_sources(&[unit(
            "algebra.sc",
            &[],
            "let Number = struct { value: i32 }\n\
             extend Number: Semigroup {\n\
               let combine(move left: Number, move right: Number): Number = { left }\n}\n",
            true,
        )])
        .unwrap_err();
        assert!(bare_algebra.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `Semigroup` is not in the prelude")
                && diagnostic.contains("use core.algebra.Semigroup")
        }));

        let bare_functional = resolve_sources(&[unit(
            "functional.sc",
            &[],
            "let Number = struct { value: i32 }\n\
             extend Number: Functor {}\n",
            true,
        )])
        .unwrap_err();
        assert!(bare_functional.iter().any(|diagnostic| {
            diagnostic.contains("standard-library item `Functor` is not in the prelude")
                && diagnostic.contains("use core.functional.Functor")
        }));

        for namespace in ["core", "alloc"] {
            let module = resolve_sources(&[
                unit("main.sc", &[], "let main(): i32 = { 0 }\n", true),
                unit(
                    &format!("{namespace}.sc"),
                    &[namespace],
                    "let value = 1\n",
                    false,
                ),
            ])
            .unwrap_err();
            assert!(module
                .iter()
                .any(|diagnostic| diagnostic.contains("standard-library namespace")));
        }

        for namespace in ["core", "alloc"] {
            let dependency = resolve_packages(&[
                package(
                    0,
                    true,
                    &[(namespace, 1)],
                    vec![unit("main.sc", &[], "let main(): i32 = { 0 }\n", true)],
                ),
                package(
                    1,
                    false,
                    &[],
                    vec![unit("dep.sc", &[], "pub let value = 1\n", true)],
                ),
            ])
            .unwrap_err();
            assert!(dependency.iter().any(|diagnostic| {
                diagnostic.contains(&format!("dependency alias `{namespace}`"))
            }));
        }
    }

    #[test]
    fn mounted_standard_exports_match_the_validated_bundles() {
        let core =
            crate::core::CoreBundle::for_edition(crate::manifest::Edition::Edition2026).unwrap();
        let core_exports = core
            .program()
            .items
            .iter()
            .zip(&core.program().item_origins)
            .filter_map(|(item, origin)| {
                Some((
                    origin.module_path.last()?.as_str(),
                    declaration_name(item)?.rsplit("::").next()?,
                ))
            })
            .collect::<BTreeSet<_>>();
        let expected_core = CORE_PRELUDE_EXPORTS
            .iter()
            .map(|name| ("prelude", *name))
            .chain(CORE_OPS_EXPORTS.iter().map(|name| ("ops", *name)))
            .chain(CORE_EFFECTS_EXPORTS.iter().map(|name| ("effects", *name)))
            .chain(CORE_ACCESS_EXPORTS.iter().map(|name| ("access", *name)))
            .chain(CORE_CONTROL_EXPORTS.iter().map(|name| ("control", *name)))
            .chain(CORE_ITER_EXPORTS.iter().map(|name| ("iter", *name)))
            .chain(CORE_ALGEBRA_EXPORTS.iter().map(|name| ("algebra", *name)))
            .chain(
                CORE_FUNCTIONAL_EXPORTS
                    .iter()
                    .map(|name| ("functional", *name)),
            )
            .collect::<BTreeSet<_>>();
        assert_eq!(core_exports, expected_core);

        let alloc =
            crate::alloc::AllocBundle::for_edition(crate::manifest::Edition::Edition2026).unwrap();
        let alloc_exports = alloc
            .program()
            .items
            .iter()
            .zip(&alloc.program().item_visibilities)
            .zip(&alloc.program().item_origins)
            .filter(|((_, visibility), _)| **visibility == Visibility::Public)
            .map(|((item, _), origin)| {
                (
                    origin
                        .module_path
                        .last()
                        .expect("alloc declaration has module provenance")
                        .as_str(),
                    declaration_name(item)
                        .expect("public alloc item is named")
                        .rsplit("::")
                        .next()
                        .expect("declaration name is nonempty"),
                )
            })
            .collect::<BTreeSet<_>>();
        let expected_alloc = ALLOC_EXPORTS.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(alloc_exports, expected_alloc);
    }

    fn expression_names(expression: Option<&Expr>) -> HashSet<String> {
        fn visit(expression: &Expr, names: &mut HashSet<String>) {
            match expression {
                Expr::Name(name) => {
                    names.insert(name.clone());
                }
                Expr::Unary(_, value)
                | Expr::Try(value)
                | Expr::Throw(value)
                | Expr::Unsafe(value)
                | Expr::Borrow { value, .. }
                | Expr::ChainMember(value, _)
                | Expr::Loop { body: value } => visit(value, names),
                Expr::DoBlock { body } => visit(body, names),
                Expr::Binary(left, _, right)
                | Expr::Coalesce(left, right)
                | Expr::Assign(left, right)
                | Expr::CompoundAssign(left, _, right) => {
                    visit(left, names);
                    visit(right, names);
                }
                Expr::HandlerCoalesce {
                    scrutinee,
                    success,
                    fallback,
                    ..
                } => {
                    visit(scrutinee, names);
                    visit(success, names);
                    visit(fallback, names);
                }
                Expr::HandlerChainCall(chain) => {
                    visit(&chain.scrutinee, names);
                    for argument in chain.groups.iter().flatten() {
                        visit(&argument.value, names);
                    }
                    visit(&chain.success, names);
                    visit(&chain.residual, names);
                }
                Expr::Call(callee, arguments) => {
                    visit(callee, names);
                    for argument in arguments {
                        visit(&argument.value, names);
                    }
                }
                Expr::StructLiteral {
                    constructor,
                    fields,
                } => {
                    visit(constructor, names);
                    for field in fields {
                        visit(&field.value, names);
                    }
                }
                Expr::Member(base, _) => visit(base, names),
                Expr::Array(elements) => {
                    for element in elements {
                        visit(element, names);
                    }
                }
                Expr::Index { base, index } => {
                    visit(base, names);
                    visit(index, names);
                }
                Expr::Block(statements, tail) => {
                    for statement in statements {
                        match statement {
                            Stmt::Let(binding) => visit(&binding.value, names),
                            Stmt::Expr(expression) => visit(expression, names),
                        }
                    }
                    if let Some(tail) = tail {
                        visit(tail, names);
                    }
                }
                Expr::Closure(_, body) => visit(body, names),
                Expr::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    visit(condition, names);
                    visit(then_branch, names);
                    if let Some(else_branch) = else_branch {
                        visit(else_branch, names);
                    }
                }
                Expr::Return(value) | Expr::Break(value) => {
                    if let Some(value) = value {
                        visit(value, names);
                    }
                }
                Expr::While { condition, body } => {
                    visit(condition, names);
                    visit(body, names);
                }
                Expr::Match { scrutinee, arms } => {
                    visit(scrutinee, names);
                    for arm in arms {
                        if let Some(guard) = &arm.guard {
                            visit(guard, names);
                        }
                        visit(&arm.body, names);
                    }
                }
                Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Continue => {}
            }
        }

        let mut names = HashSet::new();
        if let Some(expression) = expression {
            visit(expression, &mut names);
        }
        names
    }
}
