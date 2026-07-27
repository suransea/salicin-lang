//! Multi-file package module resolution.
//!
//! Source files provide their module path out of band; declarations and
//! module-level imports are collected package-wide and then flattened to
//! canonical names understood by the existing codegen.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ast::{
    Binding, EnumDef, Expr, ExtendDef, ExtendMember, Field, Function, Item, ItemOrigin, MatchArm,
    Param, Pattern, PatternField, PatternFields, Program, Sort, StaticExpr, Stmt, StructDef,
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
    /// Compiler-owned package identity reserved for the public `std` facade.
    /// Source package graphs must never claim this identity.
    pub const STD: Self = Self(usize::MAX - 2);
}

/// All source files and direct dependency aliases belonging to one package.
///
/// Package IDs provide definition identity; aliases are local to the package
/// declaring them. This keeps shared dependencies nominally identical and
/// prevents transitive dependencies from becoming accidentally spellable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePackage {
    pub id: PackageId,
    pub name: String,
    pub version: String,
    /// Resolved provider identity, including source, package name, and version.
    pub identity: String,
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
        name: "source".to_owned(),
        version: "0.0.0".to_owned(),
        identity: "source@0.0.0".to_owned(),
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
    resolve_packages_impl(packages, StandardLibraryExposure::user())
}

/// Resolve compiler-owned source modules without exposing the standard
/// library namespace to the bundle itself.
pub(crate) fn resolve_embedded_sources(sources: &[SourceUnit]) -> Result<Program, Vec<String>> {
    resolve_packages_impl(
        &[SourcePackage {
            id: PackageId(0),
            name: "core".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            identity: format!("core@{}", env!("CARGO_PKG_VERSION")),
            is_primary: true,
            dependencies: BTreeMap::new(),
            sources: sources.to_vec(),
        }],
        StandardLibraryExposure::none_for_embedded(),
    )
}

/// Resolve compiler-owned `alloc` modules while exposing the already-validated
/// `core` namespace. This keeps `alloc` as ordinary library source without
/// making it depend on its own public `alloc.*` facade.
pub(crate) fn resolve_embedded_alloc_sources(
    sources: &[SourceUnit],
) -> Result<Program, Vec<String>> {
    resolve_packages_impl(
        &[SourcePackage {
            id: PackageId(0),
            name: "alloc".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            identity: format!("alloc@{}", env!("CARGO_PKG_VERSION")),
            is_primary: true,
            dependencies: BTreeMap::new(),
            sources: sources.to_vec(),
        }],
        StandardLibraryExposure::core_for_embedded(),
    )
}

/// Resolve compiler-owned `std` modules while exposing the already-validated
/// `core` and `alloc` namespaces. The bundle cannot see or recursively import
/// its own public `std.*` facade.
pub(crate) fn resolve_embedded_std_sources(sources: &[SourceUnit]) -> Result<Program, Vec<String>> {
    resolve_packages_impl(
        &[SourcePackage {
            id: PackageId(0),
            name: "std".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            identity: format!("std@{}", env!("CARGO_PKG_VERSION")),
            is_primary: true,
            dependencies: BTreeMap::new(),
            sources: sources.to_vec(),
        }],
        StandardLibraryExposure::core_alloc_for_embedded(),
    )
}

fn resolve_packages_impl(
    packages: &[SourcePackage],
    standard_library: StandardLibraryExposure,
) -> Result<Program, Vec<String>> {
    let (prepared, dependencies, mut diagnostics) =
        validate_package_layout(packages, standard_library.allow_source_standard_modules);
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
    let required_imports = if standard_library.expose_core
        || standard_library.expose_alloc
        || standard_library.expose_std
    {
        install_standard_namespaces(
            &parsed,
            &mut symbols,
            &mut module_paths,
            &mut collection_diagnostics,
            standard_library,
        )
    } else {
        HashMap::new()
    };
    if !collection_diagnostics.is_empty() {
        return Err(collection_diagnostics);
    }

    let (mut aliases, import_diagnostics) =
        collect_imports(&parsed, &symbols, &module_paths, &dependencies);
    if !import_diagnostics.is_empty() {
        return Err(import_diagnostics);
    }
    if standard_library.expose_core {
        install_standard_prelude_aliases(&parsed, &symbols, &module_paths, &mut aliases);
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
            primary_package_identity: _,
            primary_package: _,
            package_identities: _,
            item_visibilities: unit_visibilities,
            item_origins: unit_origins,
            uses: _,
        } = program;
        debug_assert_eq!(unit_items.len(), unit_visibilities.len());
        debug_assert_eq!(unit_items.len(), unit_origins.len());

        let context = ResolveContext {
            source_path: &source.path,
            module_path: &module_path,
            package_root: &package_root,
        };
        for ((mut item, visibility), mut origin) in unit_items
            .into_iter()
            .zip(unit_visibilities)
            .zip(unit_origins)
        {
            resolver.rewrite_item(&mut item, context);
            items.push(item);
            item_visibilities.push(visibility);
            origin.package = package_id.0;
            origin.module_path = source.module_path.clone();
            if let Some(location) = &mut origin.source {
                location.path = Some(source.path.clone());
            }
            item_origins.push(origin);
            item_source_paths.push(source.path.clone());
        }
    }

    if let Err(message) = parser::infer_extend_parameters(&mut items) {
        resolver
            .diagnostics
            .push(format!("<packages>: error: {message}"));
    }

    if resolver.diagnostics.is_empty() {
        let primary = packages
            .iter()
            .find(|package| package.is_primary)
            .expect("validated package graph has one primary package");
        let mut program =
            Program::with_metadata(items, item_visibilities, item_origins, Vec::new());
        program.primary_package = primary.id.0;
        program.primary_package_identity = primary.identity.clone();
        program.package_identities = packages
            .iter()
            .map(|package| (package.id.0, package.identity.clone()))
            .chain([
                (
                    PackageId::CORE.0,
                    format!("core@{}", env!("CARGO_PKG_VERSION")),
                ),
                (
                    PackageId::ALLOC.0,
                    format!("alloc@{}", env!("CARGO_PKG_VERSION")),
                ),
                (
                    PackageId::STD.0,
                    format!("std@{}", env!("CARGO_PKG_VERSION")),
                ),
            ])
            .collect();
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StandardLibraryExposure {
    expose_core: bool,
    expose_alloc: bool,
    expose_std: bool,
    allow_source_standard_modules: bool,
}

impl StandardLibraryExposure {
    const fn user() -> Self {
        Self {
            expose_core: true,
            expose_alloc: true,
            expose_std: true,
            allow_source_standard_modules: false,
        }
    }

    const fn none_for_embedded() -> Self {
        Self {
            expose_core: false,
            expose_alloc: false,
            expose_std: false,
            allow_source_standard_modules: true,
        }
    }

    const fn core_for_embedded() -> Self {
        Self {
            expose_core: true,
            expose_alloc: false,
            expose_std: false,
            allow_source_standard_modules: true,
        }
    }

    const fn core_alloc_for_embedded() -> Self {
        Self {
            expose_core: true,
            expose_alloc: true,
            expose_std: false,
            allow_source_standard_modules: true,
        }
    }
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
const ALLOC_EXPORTS: &[(&str, &str)] =
    &[("boxed", "box"), ("vec", "vec"), ("vec", "vec_into_iter")];

const CORE_PRELUDE_EXPORTS: &[(&str, &str)] = &[
    ("never", "core::never::never"),
    ("movable", "core::marker::movable"),
    ("copyable", "core::marker::copyable"),
    ("droppable", "core::marker::droppable"),
    ("array", "core::memory::array"),
    ("ptr", "core::memory::ptr"),
    ("size_of", "core::memory::size_of"),
    ("align_of", "core::memory::align_of"),
    ("string", "core::string::string"),
    ("array_literal", "core::literal::array_literal"),
    ("string_literal", "core::literal::string_literal"),
    ("copy", "core::passing::copy"),
    ("move", "core::passing::move"),
    ("comptime", "core::passing::comptime"),
    ("mut", "$access$mut"),
    ("shared", "$access$shared"),
];
const CORE_ROOT_EXPORTS: &[(&str, &str)] = &[
    ("abi", "core::foreign::abi"),
    ("foreign", "core::foreign::foreign"),
    ("never", "core::never::never"),
    ("movable", "core::marker::movable"),
    ("copyable", "core::marker::copyable"),
    ("droppable", "core::marker::droppable"),
    ("option", "core::option::option"),
    ("result", "core::result::result"),
    ("slice", "core::memory::slice"),
    ("string", "core::string::string"),
    ("array_literal", "core::literal::array_literal"),
    ("string_literal", "core::literal::string_literal"),
];
const CORE_NEVER_EXPORTS: &[&str] = &["never"];
const CORE_MARKER_EXPORTS: &[&str] = &["movable", "copyable", "droppable"];
const CORE_ARITH_EXPORTS: &[&str] = &["add", "sub", "mul", "div", "rem", "neg"];
const CORE_BIT_EXPORTS: &[&str] = &["bit_and", "bit_or", "bit_xor", "shl", "shr", "not"];
const CORE_ASSIGN_EXPORTS: &[&str] = &[
    "add_assign",
    "sub_assign",
    "mul_assign",
    "div_assign",
    "rem_assign",
    "bit_and_assign",
    "bit_or_assign",
    "bit_xor_assign",
    "shl_assign",
    "shr_assign",
];
const CORE_INDEX_EXPORTS: &[&str] = &["index"];
const CORE_CMP_EXPORTS: &[&str] = &["eq", "partial_ordering", "partial_ord"];
const CORE_OPS_EXPORTS: &[(&str, &str)] = &[
    ("add", "core::ops::arith::add"),
    ("sub", "core::ops::arith::sub"),
    ("mul", "core::ops::arith::mul"),
    ("div", "core::ops::arith::div"),
    ("rem", "core::ops::arith::rem"),
    ("neg", "core::ops::arith::neg"),
    ("bit_and", "core::ops::bit::bit_and"),
    ("bit_or", "core::ops::bit::bit_or"),
    ("bit_xor", "core::ops::bit::bit_xor"),
    ("shl", "core::ops::bit::shl"),
    ("shr", "core::ops::bit::shr"),
    ("not", "core::ops::bit::not"),
    ("add_assign", "core::ops::assign::add_assign"),
    ("sub_assign", "core::ops::assign::sub_assign"),
    ("mul_assign", "core::ops::assign::mul_assign"),
    ("div_assign", "core::ops::assign::div_assign"),
    ("rem_assign", "core::ops::assign::rem_assign"),
    ("bit_and_assign", "core::ops::assign::bit_and_assign"),
    ("bit_or_assign", "core::ops::assign::bit_or_assign"),
    ("bit_xor_assign", "core::ops::assign::bit_xor_assign"),
    ("shl_assign", "core::ops::assign::shl_assign"),
    ("shr_assign", "core::ops::assign::shr_assign"),
    ("eq", "core::cmp::eq"),
    ("partial_ordering", "core::cmp::partial_ordering"),
    ("partial_ord", "core::cmp::partial_ord"),
    ("index", "core::ops::index::index"),
    ("chain", "core::flow::chain"),
    ("coalesce", "core::flow::coalesce"),
    ("unwrap", "core::flow::unwrap"),
    ("raise", "core::flow::raise"),
];
const CORE_FLOW_EXPORTS: &[&str] = &["chain", "coalesce", "unwrap", "raise"];
const CORE_EFFECT_EXPORTS: &[&str] = &["continuation", "effect_callable", "handle"];
const CORE_RESULT_EXPORTS: &[&str] = &["result"];
const CORE_ERROR_EXPORTS: &[&str] = &["throwing", "try", "throw"];
const CORE_UNSAFE_EXPORTS: &[&str] = &["unsafety", "unsafe"];
const CORE_ASYNC_EXPORTS: &[&str] = &["suspension", "poll", "future", "executor", "async", "await"];
const CORE_PRIMITIVE_EXPORTS: &[&str] = &[
    "bool", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];
const CORE_SORT_EXPORTS: &[&str] = &[
    "sort",
    "sort_of",
    "type_of",
    "type",
    "region",
    "effect",
    "effects",
    "parameters",
];
const CORE_STRING_EXPORTS: &[&str] = &["string"];
const CORE_LITERAL_EXPORTS: &[&str] = &["array_literal", "string_literal"];
const CORE_FOREIGN_EXPORTS: &[&str] = &["abi", "foreign"];
const CORE_PASSING_EXPORTS: &[&str] = &["copy", "move", "comptime"];
const CORE_BORROW_EXPORTS: &[&str] = &["access", "mut", "shared", "borrow"];
const CORE_MEMORY_EXPORTS: &[&str] = &["array", "slice", "ptr", "size_of", "align_of"];
const CORE_CONTROL_EXPORTS: &[&str] = &[
    "loop_exit",
    "iteration_skip",
    "function_exit",
    "attempt",
    "break",
    "continue",
    "return",
    "do",
    "loop",
    "while",
    "if",
    "match",
    "for",
    "defer",
];
const CORE_ITER_EXPORTS: &[&str] = &[
    "iterator",
    "into_iterator",
    "array_into_iter",
    "slice_iter",
    "owned_item",
    "borrowed_item",
];

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
    let mut package_identities = HashMap::new();
    for package in packages {
        let identity = package.identity.clone();
        if let Some(previous) = package_identities.insert(identity.clone(), package.id) {
            diagnostics.push(format!(
                "<packages>: error: packages #{} and #{} have duplicate identity `{identity}`",
                previous.0, package.id.0
            ));
        }
        if matches!(
            package.id,
            PackageId::CORE | PackageId::ALLOC | PackageId::STD
        ) {
            diagnostics.push(format!(
                "<packages>: error: package ID {} is reserved for compiler {}",
                package.id.0,
                match package.id {
                    PackageId::CORE => "core",
                    PackageId::ALLOC => "alloc",
                    PackageId::STD => "std",
                    _ => unreachable!("reserved package ID check is exhaustive"),
                }
            ));
        }
        let root = if package.is_primary {
            Vec::new()
        } else {
            vec![stable_package_root(&identity)]
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
            if is_reserved_standard_namespace(alias) && !allow_standard_namespaces {
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
            .unwrap_or_else(|| vec![stable_package_root(&package.identity)]);

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
                if is_reserved_standard_namespace(first) && !allow_standard_namespaces {
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

fn stable_package_root(identity: &str) -> String {
    let mut root = String::from("@");
    for byte in identity.as_bytes() {
        use std::fmt::Write as _;
        write!(root, "{byte:02x}").expect("writing to a string cannot fail");
    }
    root
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
                let type_value_pair = (namespace == DeclarationNamespace::Type
                    && (occupied.contains(&DeclarationNamespace::Function)
                        || occupied.contains(&DeclarationNamespace::Other)))
                    || (namespace != DeclarationNamespace::Type
                        && occupied.contains(&DeclarationNamespace::Type));
                if type_value_pair && *visibility != previous.visibility {
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
                    if type_value_pair
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
                } else if type_value_pair
                    && !occupied.contains(&namespace)
                    && !(namespace == DeclarationNamespace::Type
                        && occupied.contains(&DeclarationNamespace::Type))
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

            if let Item::Sort(crate::ast::SortDef {
                members: Some(members),
                ..
            }) = item
            {
                let owner = canonical_name(&unit.module_path, name);
                for member in members {
                    let mut member_path = unit.module_path.clone();
                    member_path.push(name.to_owned());
                    member_path.push(member.clone());
                    symbols.insert(
                        member_path,
                        Symbol {
                            canonical: format!("{owner}.{member}"),
                            module_path: unit.module_path.clone(),
                            package_root: unit.package_root.clone(),
                            visibility: *visibility,
                            source_path: unit.source.path.clone(),
                        },
                    );
                }
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
    exposure: StandardLibraryExposure,
) -> HashMap<String, String> {
    let package_roots = parsed
        .iter()
        .map(|unit| unit.package_root.clone())
        .collect::<BTreeSet<_>>();
    let mut required_imports = HashMap::new();

    if exposure.expose_alloc {
        for (module, name) in ALLOC_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("alloc.{module}.{name}"));
        }
    }
    if exposure.expose_core {
        for (name, _) in CORE_ROOT_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.{name}"));
        }
        for (name, _) in CORE_OPS_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.ops.{name}"));
        }
        for name in CORE_FLOW_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.flow.{name}"));
        }
        for name in CORE_EFFECT_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.effect.{name}"));
        }
        for name in CORE_RESULT_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.result.{name}"));
        }
        for name in CORE_ERROR_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.error.{name}"));
        }
        for name in CORE_UNSAFE_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.unsafe.{name}"));
        }
        for name in CORE_ASYNC_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.async.{name}"));
        }
        for name in CORE_SORT_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.sorts.{name}"));
        }
        for name in CORE_STRING_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.string.{name}"));
        }
        for name in CORE_FOREIGN_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.foreign.{name}"));
        }
        for name in CORE_PASSING_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.passing.{name}"));
        }
        for name in CORE_BORROW_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.borrow.{name}"));
        }
        for name in CORE_MEMORY_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.memory.{name}"));
        }
        for name in CORE_ITER_EXPORTS {
            required_imports.insert((*name).to_owned(), format!("core.iter.{name}"));
        }
    }
    let std_exports = if exposure.expose_std {
        match crate::standard::StdBundle::cached_for_edition(crate::manifest::Edition::Edition2026)
        {
            Ok(bundle) => Some(bundle.exports()),
            Err(error) => {
                diagnostics.push(format!("<std>: error: {error}"));
                None
            }
        }
    } else {
        None
    };
    if let Some(exports) = std_exports {
        let mut std_required_imports = HashMap::new();
        for export in exports {
            if export.module.is_empty() && is_core_prelude_export(&export.name) {
                continue;
            }
            if matches!(
                export.module.as_str(),
                "prelude" | "never" | "marker" | "option" | "result" | "cmp"
            ) {
                continue;
            }
            if export.module == "ops"
                && matches!(
                    export.name.as_str(),
                    "chain" | "coalesce" | "unwrap" | "raise"
                )
            {
                continue;
            }
            let import_path = if export.module.is_empty() {
                format!("std.{}", export.name)
            } else {
                format!("std.{}.{}", export.module, export.name)
            };
            std_required_imports
                .entry(export.name.clone())
                .or_insert(import_path);
        }
        required_imports.extend(std_required_imports);
    }

    for package_root in package_roots {
        if exposure.expose_core {
            install_core_namespace(symbols, module_paths, diagnostics, &package_root);
        }

        if exposure.expose_alloc {
            install_alloc_namespace(symbols, module_paths, diagnostics, &package_root);
        }

        if let Some(exports) = std_exports {
            install_std_namespace(symbols, module_paths, diagnostics, &package_root, exports);
        }
    }

    required_imports
}

fn is_reserved_standard_namespace(name: &str) -> bool {
    matches!(name, "core" | "alloc" | "std")
}

fn is_core_prelude_export(name: &str) -> bool {
    CORE_PRELUDE_EXPORTS
        .iter()
        .any(|(export, _)| *export == name)
}

fn standard_module_path(root: &[String], module: &str) -> Vec<String> {
    let mut module_path = root.to_vec();
    if !module.is_empty() {
        module_path.extend(module.split('.').map(str::to_owned));
    }
    module_path
}

fn insert_standard_module_path(
    module_paths: &mut HashSet<Vec<String>>,
    root: &[String],
    module: &str,
) {
    let mut module_path = root.to_vec();
    if module.is_empty() {
        module_paths.insert(module_path);
        return;
    }
    for segment in module.split('.') {
        module_path.push(segment.to_owned());
        module_paths.insert(module_path.clone());
    }
}

fn insert_standard_symbol(
    symbols: &mut SymbolTable,
    package_root: &[String],
    namespace_root: &[String],
    module: &str,
    name: &str,
    canonical: &str,
    source_path: &str,
) {
    let module_path = standard_module_path(namespace_root, module);
    let mut logical_path = module_path.clone();
    logical_path.push(name.to_owned());
    symbols.insert(
        logical_path,
        Symbol {
            canonical: canonical.to_owned(),
            module_path,
            package_root: package_root.to_vec(),
            visibility: Visibility::Public,
            source_path: source_path.to_owned(),
        },
    );
}

fn install_core_namespace(
    symbols: &mut SymbolTable,
    module_paths: &mut HashSet<Vec<String>>,
    diagnostics: &mut Vec<String>,
    package_root: &[String],
) {
    let mut core_root = package_root.to_vec();
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
            "never",
            "marker",
            "option",
            "result",
            "error",
            "cmp",
            "ops",
            "ops.arith",
            "ops.bit",
            "ops.assign",
            "flow",
            "effect",
            "async",
            "unsafe",
            "sorts",
            "literal",
            "string",
            "foreign",
            "control",
            "iter",
            "algebra",
            "functional",
        ] {
            insert_standard_module_path(module_paths, &core_root, module);
        }
        for (name, canonical) in CORE_PRELUDE_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "prelude",
                name,
                canonical,
                "<core>",
            );
        }
        for (name, canonical) in CORE_ROOT_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "",
                name,
                canonical,
                "<core>",
            );
        }
        for name in CORE_NEVER_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "never",
                name,
                &format!("core::never::{name}"),
                "<core>",
            );
        }
        for name in CORE_MARKER_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "marker",
                name,
                &format!("core::marker::{name}"),
                "<core>",
            );
        }
        insert_standard_symbol(
            symbols,
            package_root,
            &core_root,
            "option",
            "option",
            "core::option::option",
            "<core>",
        );
        for name in CORE_RESULT_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "result",
                name,
                &format!("core::result::{name}"),
                "<core>",
            );
        }
        for name in CORE_ERROR_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "error",
                name,
                &format!("core::error::{name}"),
                "<core>",
            );
        }
        for name in CORE_CMP_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "cmp",
                name,
                &format!("core::cmp::{name}"),
                "<core>",
            );
        }
        for (name, canonical) in CORE_OPS_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "ops",
                name,
                canonical,
                "<core>",
            );
        }
        for name in CORE_ARITH_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "ops.arith",
                name,
                &format!("core::ops::arith::{name}"),
                "<core>",
            );
        }
        for name in CORE_BIT_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "ops.bit",
                name,
                &format!("core::ops::bit::{name}"),
                "<core>",
            );
        }
        for name in CORE_ASSIGN_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "ops.assign",
                name,
                &format!("core::ops::assign::{name}"),
                "<core>",
            );
        }
        for name in CORE_INDEX_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "ops.index",
                name,
                &format!("core::ops::index::{name}"),
                "<core>",
            );
        }
        for name in CORE_FLOW_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "flow",
                name,
                &format!("core::flow::{name}"),
                "<core>",
            );
        }
        for name in CORE_EFFECT_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "effect",
                name,
                &format!("core::effect::{name}"),
                "<core>",
            );
        }
        for name in CORE_UNSAFE_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "unsafe",
                name,
                &format!("core::unsafe::{name}"),
                "<core>",
            );
        }
        for name in CORE_ASYNC_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "async",
                name,
                &format!("core::async::{name}"),
                "<core>",
            );
        }
        for name in CORE_PRIMITIVE_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "primitives",
                name,
                &format!("core::primitives::{name}"),
                "<core>",
            );
        }
        for name in CORE_SORT_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "sorts",
                name,
                &format!("core::sorts::{name}"),
                "<core>",
            );
        }
        for name in CORE_STRING_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "string",
                name,
                &format!("core::string::{name}"),
                "<core>",
            );
        }
        for name in CORE_LITERAL_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "literal",
                name,
                &format!("core::literal::{name}"),
                "<core>",
            );
        }
        for name in CORE_FOREIGN_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "foreign",
                name,
                &format!("core::foreign::{name}"),
                "<core>",
            );
        }
        for name in CORE_PASSING_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "passing",
                name,
                &format!("core::passing::{name}"),
                "<core>",
            );
        }
        for name in CORE_BORROW_EXPORTS {
            let canonical = match *name {
                "mut" => "$access$mut".to_owned(),
                "shared" => "$access$shared".to_owned(),
                _ => format!("core::borrow::{name}"),
            };
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "borrow",
                name,
                &canonical,
                "<core>",
            );
        }
        for name in CORE_MEMORY_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "memory",
                name,
                &format!("core::memory::{name}"),
                "<core>",
            );
        }
        for name in CORE_CONTROL_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "control",
                name,
                &format!("core::control::{name}"),
                "<core>",
            );
        }
        for name in CORE_ITER_EXPORTS {
            insert_standard_symbol(
                symbols,
                package_root,
                &core_root,
                "iter",
                name,
                &format!("core::iter::{name}"),
                "<core>",
            );
        }
    }
}

fn install_standard_prelude_aliases(
    parsed: &[ParsedUnit<'_>],
    symbols: &SymbolTable,
    module_paths: &HashSet<Vec<String>>,
    aliases: &mut AliasTable,
) {
    let package_roots = parsed
        .iter()
        .map(|unit| unit.package_root.clone())
        .collect::<BTreeSet<_>>();
    for package_root in package_roots {
        for (name, canonical) in CORE_PRELUDE_EXPORTS {
            let mut key = package_root.clone();
            key.push((*name).to_owned());
            if symbols.contains_key(&key)
                || module_paths.contains(&key)
                || aliases.contains_key(&key)
            {
                continue;
            }
            aliases.insert(
                key,
                ResolvedAlias {
                    target: AliasTarget::Declaration((*canonical).to_owned()),
                    visibility: Visibility::Public,
                    module_path: package_root.clone(),
                    package_root: package_root.clone(),
                },
            );
        }
    }
}

fn install_alloc_namespace(
    symbols: &mut SymbolTable,
    module_paths: &mut HashSet<Vec<String>>,
    diagnostics: &mut Vec<String>,
    package_root: &[String],
) {
    let mut alloc_root = package_root.to_vec();
    alloc_root.push("alloc".to_owned());
    if module_paths.contains(&alloc_root) {
        diagnostics.push(
            "<package>: error: top-level module `alloc` conflicts with the standard-library namespace"
                .to_owned(),
        );
        return;
    }
    if let Some(symbol) = symbols.get(&alloc_root) {
        diagnostics.push(format!(
            "{}: error: root declaration `alloc` conflicts with the standard-library namespace",
            symbol.source_path
        ));
        return;
    }

    module_paths.insert(alloc_root.clone());
    for module in ["boxed", "vec"] {
        let mut module_path = alloc_root.clone();
        module_path.push(module.to_owned());
        module_paths.insert(module_path);
    }
    for (module, name) in ALLOC_EXPORTS {
        insert_standard_symbol(
            symbols,
            package_root,
            &alloc_root,
            "",
            name,
            &format!("alloc::{module}::{name}"),
            "<alloc>",
        );
        let mut module_path = alloc_root.clone();
        module_path.push((*module).to_owned());
        let mut logical_path = module_path.clone();
        logical_path.push((*name).to_owned());
        symbols.insert(
            logical_path,
            Symbol {
                canonical: format!("alloc::{module}::{name}"),
                module_path,
                package_root: package_root.to_vec(),
                visibility: Visibility::Public,
                source_path: "<alloc>".to_owned(),
            },
        );
    }
}

fn install_std_namespace(
    symbols: &mut SymbolTable,
    module_paths: &mut HashSet<Vec<String>>,
    diagnostics: &mut Vec<String>,
    package_root: &[String],
    exports: &[crate::standard::StdExport],
) {
    let mut std_root = package_root.to_vec();
    std_root.push("std".to_owned());
    if module_paths.contains(&std_root) {
        diagnostics.push(
            "<package>: error: top-level module `std` conflicts with the standard-library namespace"
                .to_owned(),
        );
        return;
    }
    if let Some(symbol) = symbols.get(&std_root) {
        diagnostics.push(format!(
            "{}: error: root declaration `std` conflicts with the standard-library namespace",
            symbol.source_path
        ));
        return;
    }

    module_paths.insert(std_root.clone());
    for export in exports {
        insert_standard_module_path(module_paths, &std_root, &export.module);
        let canonical = if export.target.first().is_some_and(|root| root == "std") {
            export.target.join("::")
        } else {
            let mut target_path = package_root.to_vec();
            target_path.extend(export.target.iter().cloned());
            let Some(target) = symbols.get(&target_path) else {
                diagnostics.push(format!(
                    "<std>: error: std export `{}` targets unavailable declaration `{}`",
                    if export.module.is_empty() {
                        format!("std.{}", export.name)
                    } else {
                        format!("std.{}.{}", export.module, export.name)
                    },
                    export.target.join(".")
                ));
                continue;
            };
            target.canonical.clone()
        };
        let mut logical_path = std_root.clone();
        if !export.module.is_empty() {
            logical_path.extend(export.module.split('.').map(str::to_owned));
        }
        logical_path.push(export.name.clone());
        symbols.insert(
            logical_path,
            Symbol {
                canonical,
                module_path: standard_module_path(&std_root, &export.module),
                package_root: package_root.to_vec(),
                visibility: Visibility::Public,
                source_path: "<std>".to_owned(),
            },
        );
    }
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
            let finite_sort_keyword_alias = alias == "mut"
                && declaration
                    .path
                    .last()
                    .is_some_and(|target| target == "mut");
            if !is_valid_import_alias(&alias) && !finite_sort_keyword_alias {
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
                Item::Sort(definition) => &definition.name,
                Item::TypeForm(definition) => &definition.name,
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
        Item::Sort(_) | Item::TypeForm(_) => {}
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
            trait_bound_types.insert("self".to_owned());
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
                        ..
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
            bound_types.insert("self".to_owned());
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
                    let binding_types =
                        compile_parameter_names(&binding.compile_groups, &bound_types);
                    validate_exposed_type(
                        &binding.ty,
                        extension_boundary,
                        source_path,
                        &binding_types,
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
            let binding_types = compile_parameter_names(&binding.compile_groups, &bound_types);
            validate_exposed_type(
                &binding.ty,
                boundary,
                source_path,
                &binding_types,
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
        Type::Tuple(fields) => {
            for field in fields {
                validate_exposed_type(
                    field,
                    exposed,
                    source_path,
                    bound_types,
                    description,
                    nominal_boundaries,
                    diagnostics,
                );
            }
        }
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
        Type::ArrayApplication {
            constructor,
            element,
            ..
        } => {
            if !is_bound_api_type(constructor, bound_types) {
                if let Some(referenced) = nominal_boundaries.get(constructor.as_str()) {
                    if !api_audience_is_contained(exposed, referenced) {
                        diagnostics.push(format!(
                            "{source_path}: error: {description} with {} visibility exposes {} type `{constructor}` beyond its access boundary{}",
                            visibility_description(exposed.visibility),
                            visibility_description(referenced.visibility),
                            boundary_location(referenced),
                        ));
                    }
                }
            }
            validate_exposed_type(
                element,
                exposed,
                source_path,
                bound_types,
                description,
                nominal_boundaries,
                diagnostics,
            );
        }
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
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => {}
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
        Item::Sort(definition) => Some(&definition.name),
        Item::TypeForm(definition) => Some(&definition.name),
        Item::TypeAlias(definition) => Some(&definition.name),
        Item::Extend(_) => None,
    }
}

fn declaration_namespace(item: &Item) -> DeclarationNamespace {
    match item {
        Item::Function(_) => DeclarationNamespace::Function,
        Item::Struct(_)
        | Item::Enum(_)
        | Item::Trait(_)
        | Item::Effect(_)
        | Item::Sort(_)
        | Item::TypeAlias(_)
        | Item::TypeForm(_) => DeclarationNamespace::Type,
        Item::Global(_) | Item::Extend(_) => DeclarationNamespace::Other,
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
                self.rewrite_compile_parameter_types(
                    &mut definition.compile_groups,
                    context,
                    &HashSet::new(),
                );
                let type_scope =
                    compile_parameter_names(&definition.compile_groups, &HashSet::new());
                self.rewrite_type(&mut definition.target, context, &type_scope);
            }
            Item::Struct(definition) => self.rewrite_struct(definition, context),
            Item::Enum(definition) => self.rewrite_enum(definition, context),
            Item::Trait(definition) => self.rewrite_trait(definition, context),
            Item::Effect(definition) => {
                definition.name = canonical_name(context.module_path, &definition.name);
                self.rewrite_compile_parameter_types(
                    &mut definition.compile_groups,
                    context,
                    &HashSet::new(),
                );
                let type_scope =
                    compile_parameter_names(&definition.compile_groups, &HashSet::new());
                for operation in &mut definition.operations {
                    self.rewrite_function(operation, context, &type_scope);
                }
            }
            Item::Sort(definition) => {
                definition.name = canonical_name(context.module_path, &definition.name);
            }
            Item::TypeForm(definition) => {
                definition.name = canonical_name(context.module_path, &definition.name);
            }
            Item::Extend(extension) => self.rewrite_extend(extension, context),
        }
    }

    fn rewrite_struct(&mut self, definition: &mut StructDef, context: ResolveContext<'_>) {
        definition.name = canonical_name(context.module_path, &definition.name);
        self.rewrite_compile_parameter_types(
            &mut definition.compile_groups,
            context,
            &HashSet::new(),
        );
        let type_scope = compile_parameter_names(&definition.compile_groups, &HashSet::new());
        for field in &mut definition.fields {
            self.rewrite_field(field, context, &type_scope);
        }
    }

    fn rewrite_enum(&mut self, definition: &mut EnumDef, context: ResolveContext<'_>) {
        definition.name = canonical_name(context.module_path, &definition.name);
        self.rewrite_compile_parameter_types(
            &mut definition.compile_groups,
            context,
            &HashSet::new(),
        );
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
        self.rewrite_compile_parameter_types(
            &mut definition.compile_groups,
            context,
            &HashSet::new(),
        );
        let mut trait_types = compile_parameter_names(&definition.compile_groups, &HashSet::new());
        trait_types.insert("self".to_owned());
        trait_types.extend(definition.members.iter().filter_map(|member| match member {
            TraitMember::AssociatedType { name, .. } => Some(name.clone()),
            TraitMember::Function(_) => None,
        }));
        for predicate in &mut definition.where_predicates {
            self.rewrite_type(&mut predicate.subject, context, &trait_types);
            self.rewrite_type(&mut predicate.trait_ref, context, &trait_types);
            for binding in &mut predicate.associated_types {
                self.rewrite_associated_binding(binding, context, &trait_types);
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
        self.rewrite_compile_parameter_types(
            &mut extension.compile_groups,
            context,
            &HashSet::new(),
        );
        let header_type_scope = compile_parameter_names(&extension.compile_groups, &HashSet::new());
        self.rewrite_type(&mut extension.target, context, &header_type_scope);
        if let Some(trait_ref) = &mut extension.trait_ref {
            self.rewrite_type(trait_ref, context, &header_type_scope);
        }
        for predicate in &mut extension.where_predicates {
            self.rewrite_type(&mut predicate.subject, context, &header_type_scope);
            self.rewrite_type(&mut predicate.trait_ref, context, &header_type_scope);
            for binding in &mut predicate.associated_types {
                self.rewrite_associated_binding(binding, context, &header_type_scope);
            }
        }

        let mut member_type_scope = header_type_scope;
        member_type_scope.insert("self".to_owned());
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
        self.rewrite_compile_parameter_types(&mut function.compile_groups, context, outer_types);
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
        if let Some(error) = &mut function.effects.failure {
            self.rewrite_type(error, context, &type_scope);
        }
        for effect in &mut function.effects.custom {
            self.rewrite_type(effect, context, &type_scope);
        }
        for predicate in &mut function.where_predicates {
            self.rewrite_type(&mut predicate.subject, context, &type_scope);
            self.rewrite_type(&mut predicate.trait_ref, context, &type_scope);
            for binding in &mut predicate.associated_types {
                self.rewrite_associated_binding(binding, context, &type_scope);
            }
        }
        if let Some(body) = &mut function.body {
            self.rewrite_expr(body, context, &type_scope, &value_scope);
        }
    }

    fn rewrite_compile_parameter_types(
        &mut self,
        groups: &mut [Vec<crate::ast::CompileParam>],
        context: ResolveContext<'_>,
        outer_types: &HashSet<String>,
    ) {
        for parameter in groups.iter_mut().flatten() {
            let Sort::Named(name) = &mut parameter.kind else {
                continue;
            };
            if matches!(
                name.as_str(),
                "bool"
                    | "access"
                    | "abi"
                    | "string"
                    | "type"
                    | "region"
                    | "effect"
                    | "effects"
                    | "parameters"
            ) {
                continue;
            }
            let mut source = Type::Named(name.clone(), Vec::new());
            self.rewrite_type(&mut source, context, outer_types);
            if let Type::Named(resolved, arguments) = source {
                if arguments.is_empty() {
                    *name = resolved;
                }
            }
        }
    }

    fn rewrite_associated_binding(
        &mut self,
        binding: &mut crate::ast::AssociatedTypeBinding,
        context: ResolveContext<'_>,
        outer_types: &HashSet<String>,
    ) {
        self.rewrite_compile_parameter_types(&mut binding.compile_groups, context, outer_types);
        let type_scope = compile_parameter_names(&binding.compile_groups, outer_types);
        self.rewrite_type(&mut binding.ty, context, &type_scope);
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
            Type::Tuple(fields) => {
                for field in fields {
                    self.rewrite_type(field, context, type_scope);
                }
            }
            Type::Borrow { pointee, .. } => self.rewrite_type(pointee, context, type_scope),
            Type::Array(element, _) => self.rewrite_type(element, context, type_scope),
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } => {
                self.rewrite_type(element, context, type_scope);
                if let crate::ast::USizeConst::Expression(expression) = length {
                    self.rewrite_static_expression(expression, context, type_scope);
                }
                let segments: Vec<String> = constructor.split('.').map(str::to_owned).collect();
                if let Some(canonical) = self.resolve_logical_path(&segments, context) {
                    *constructor = canonical;
                } else if !self.reject_unimported_standard(&segments, context) {
                    self.reject_bare_module(&segments, context, "an array type constructor");
                }
            }
            Type::Function {
                groups,
                effects,
                result,
            } => {
                for ty in groups.iter_mut().flatten() {
                    self.rewrite_type(ty, context, type_scope);
                }
                if let Some(error) = &mut effects.failure {
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
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::I128
            | Type::ISize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::U128
            | Type::USize
            | Type::Bool
            | Type::Unit
            | Type::CompileUSize(_) => {}
        }
    }

    fn rewrite_static_expression(
        &mut self,
        expression: &mut StaticExpr,
        context: ResolveContext<'_>,
        static_scope: &HashSet<String>,
    ) {
        match expression {
            StaticExpr::Name(name) => {
                if static_scope.contains(name) {
                    return;
                }
                let logical = vec![name.clone()];
                if let Some(canonical) = self.resolve_logical_path(&logical, context) {
                    *name = canonical;
                } else if !self.reject_unimported_standard(&logical, context) {
                    self.reject_bare_module(&logical, context, "a static value");
                }
            }
            StaticExpr::Unary(_, operand) => {
                self.rewrite_static_expression(operand, context, static_scope)
            }
            StaticExpr::Binary(left, _, right) => {
                self.rewrite_static_expression(left, context, static_scope);
                self.rewrite_static_expression(right, context, static_scope);
            }
            StaticExpr::Call { function, groups } => {
                let logical: Vec<String> = function.split('.').map(str::to_owned).collect();
                if let Some(canonical) = self.resolve_logical_path(&logical, context) {
                    *function = canonical;
                } else if !self.reject_unimported_standard(&logical, context) {
                    self.reject_bare_module(&logical, context, "a ctfe function");
                }
                for argument in groups.iter_mut().flatten() {
                    self.rewrite_static_expression(&mut argument.value, context, static_scope);
                }
            }
            StaticExpr::USize(_) | StaticExpr::Bool(_) => {}
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
            Expr::Located { value, .. } => {
                self.rewrite_expr(value, context, type_scope, value_scope)
            }
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
            Expr::DoBlock { body } | Expr::Async { body } | Expr::Await(body) => {
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
            Expr::Array(elements) | Expr::Tuple(elements) => {
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
            Expr::PatternClosure {
                pattern,
                guard,
                body,
            } => {
                let mut bindings = HashSet::new();
                self.rewrite_pattern(pattern, context, value_scope, &mut bindings);
                let mut closure_scope = value_scope.clone();
                closure_scope.extend(bindings);
                if let Some(guard) = guard {
                    self.rewrite_expr(guard, context, type_scope, &closure_scope);
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
            Expr::While {
                condition, body, ..
            } => {
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
            Expr::Type(_)
            | Expr::Unit
            | Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::Continue => {}
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
            Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) => {}
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
            Pattern::Tuple(patterns) => {
                for pattern in patterns {
                    self.rewrite_pattern(pattern, context, value_scope, bindings);
                }
            }
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
            "{}: error: standard-library item `{name}` is not in the prelude; bind it with `let {name} = {import_path}`",
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
                    Sort::Type
                        | Sort::TypeConstructor { .. }
                        | Sort::EffectConstructor { .. }
                        | Sort::ParameterModifier
                )
            })
            .map(|parameter| parameter.name.clone()),
    );
    names
}

fn compile_argument_name_is_builtin(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16"
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
            | "shared"
            | "mut"
            | "copy"
            | "move"
            | "pure"
            | "self"
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
mod tests;
