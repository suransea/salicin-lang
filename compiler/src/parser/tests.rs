use super::*;

#[test]
fn parses_prefix_effect_callable_declarations_and_types() {
    let program = parse(
        "let apply(comptime e: effects): with(e)\n\
           (action: with(e)((i32): i32))\n\
           (value: i32): i32 = { action(value) }\n\
         let pure(value: i32): i32 = { value }\n",
    )
    .expect("prefix effect callable syntax must parse");
    let Item::Function(apply) = &program.items[0] else {
        panic!("expected apply function");
    };
    assert_eq!(apply.effects.parameters, ["e"]);
    let Type::Function { effects, .. } = &apply.groups[0][0].ty else {
        panic!("expected callable action parameter");
    };
    assert_eq!(effects.parameters, ["e"]);
    let Item::Function(pure) = &program.items[1] else {
        panic!("expected pure function");
    };
    assert_eq!(pure.effects, FunctionEffects::default());
}

#[test]
fn prefix_with_requires_a_callable_operand_and_accepts_an_empty_row() {
    let program = parse("let use(action: with()((i32): i32)): i32 = { action(1) }\n")
        .expect("an empty effect row is a pure callable");
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Type::Function { effects, .. } = &function.groups[0][0].ty else {
        panic!("expected callable parameter");
    };
    assert_eq!(effects, &FunctionEffects::default());

    let error = parse("let value: with(io)(i32) = 1\n")
        .expect_err("with must reject a non-callable operand");
    assert!(
        error.message.contains("accepts only a callable type")
            || error.message.contains("parameter name"),
        "{}",
        error.message
    );
}

#[test]
fn parses_contextual_test_declarations_as_private_unit_throwing_functions() {
    let program =
        parse("test(\"arithmetic works\") {\n  core.error.throw(\"broken\")\n}\n").unwrap();
    let [Item::Function(test)] = program.items.as_slice() else {
        panic!("expected one test function");
    };
    assert_eq!(test.name, "$test$61726974686d6574696320776f726b73");
    assert_eq!(test.groups, vec![Vec::new()]);
    assert_eq!(test.return_type, Some(Type::Unit));
    assert_eq!(
        test.effects,
        FunctionEffects {
            custom: vec![Type::Named(
                "core.error.throwing".to_owned(),
                vec![Type::Named("core.string.string".to_owned(), Vec::new())],
            )],
            ..FunctionEffects::default()
        }
    );
    assert_eq!(program.item_visibilities, [Visibility::Private]);

    let visible = parse("pub test(\"arithmetic\") { () }\n")
        .expect_err("test declarations must remain runner-private");
    assert!(visible.message.contains("cannot have visibility"));
    let identifier = parse("test arithmetic { true }\n")
        .expect_err("test registration names must be string literals");
    assert!(identifier.message.contains("`(` after `test`"));
    let empty = parse("test(\"\") { () }\n").expect_err("test names must be useful");
    assert!(empty.message.contains("cannot be empty"));
}

fn function_tail(function: &Function) -> &Expr {
    let Some(Expr::Block(_, Some(tail))) = &function.body else {
        panic!("expected function body block with a tail value");
    };
    tail.unlocated()
}

fn flatten_test_call<'a>(expression: &'a Expr, groups: &mut Vec<&'a [CallArg]>) -> &'a Expr {
    let expression = expression.unlocated();
    if let Expr::Call(callee, arguments) = expression {
        let root = flatten_test_call(callee, groups);
        groups.push(arguments);
        root
    } else {
        expression
    }
}

fn match_call_parts(expression: &Expr) -> (&Expr, Vec<&Expr>) {
    let mut groups = Vec::new();
    let root = flatten_test_call(expression, &mut groups);
    assert_eq!(root, &Parser::core_match_function());
    assert!(groups.len() >= 2, "expected an input and at least one case");
    let values = groups
        .into_iter()
        .map(|group| {
            let [CallArg { label: None, value }] = group else {
                panic!("expected a single unlabeled match argument");
            };
            value
        })
        .collect::<Vec<_>>();
    (values[0], values[1..].to_vec())
}

fn if_call_parts(expression: &Expr) -> (&Expr, &Expr, &Expr) {
    fn branch(group: &[CallArg]) -> &Expr {
        let [CallArg {
            label: None,
            value: Expr::Closure(parameters, body),
        }] = group
        else {
            panic!("expected one unlabeled if branch closure");
        };
        assert!(parameters.is_empty());
        body.as_ref()
    }

    let mut groups = Vec::new();
    assert_eq!(
        flatten_test_call(expression, &mut groups),
        &Parser::core_if_function()
    );
    let [condition_group, then_group, else_group] = groups.as_slice() else {
        panic!("expected condition, then, and else groups");
    };
    let [CallArg {
        label: None,
        value: condition,
    }] = *condition_group
    else {
        panic!("expected one unlabeled if condition");
    };
    (condition, branch(then_group), branch(else_group))
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
        parse("let read(pointer: ptr(i32)): i32 with(unsafety) = { *pointer }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(!function.effects.unsafety);
    assert_eq!(
        function.effects.custom,
        vec![Type::Named("unsafety".to_owned(), Vec::new())]
    );

    let program = parse(
        "let answer(unsafe: i32): i32 = { unsafe }\n\
             let recover(try: i32): i32 = { try }\n",
    )
    .unwrap();
    assert_eq!(program.items.len(), 2);

    let error = parse("let f(): i32 ! unsafe = { 42 }\n").unwrap_err();
    assert!(error.message.contains("expected a newline or `;`"));

    let program =
        parse("let fallible(): i32 with(throwing(bool), unsafety) = { throw(true) }\n").unwrap();
    let Item::Function(fallible) = &program.items[0] else {
        panic!("expected fallible function");
    };
    assert_eq!(fallible.return_type, Some(Type::I32));
    assert!(!fallible.effects.unsafety);
    assert_eq!(fallible.effects.failure, None);
    assert_eq!(
        fallible.effects.custom,
        vec![
            Type::Named("throwing".to_owned(), vec![Type::Bool]),
            Type::Named("unsafety".to_owned(), Vec::new())
        ]
    );

    for source in [
        "let f(): i32 with(unsafety, unsafety) = { 0 }\n",
        "let f(): i32 with(throwing(bool), throwing(bool)) = { 0 }\n",
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains("duplicate"));
    }

    let contextual =
        parse("let f(): i32 with(unsafe, try(bool)) = { 0 }\n").expect("custom effect names");
    let Item::Function(contextual) = &contextual.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        contextual.effects.custom,
        [
            Type::Named("unsafe".to_owned(), Vec::new()),
            Type::Named("try".to_owned(), vec![Type::Bool]),
        ]
    );
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
    let mut program = parse(
        "use net.http.client\n\
             use net.http.client as other_client\n\
             pub use net.http.{get, post as send}\n\
             pub(package) use root.core.value\n\
             let answer = 42\n",
    )
    .unwrap();
    assert!(program
        .uses
        .iter()
        .all(|declaration| declaration.source.is_some()));
    for declaration in &mut program.uses {
        declaration.source = None;
    }

    assert_eq!(
        program.uses,
        vec![
            UseDecl {
                visibility: Visibility::Private,
                path: vec!["net".into(), "http".into(), "client".into()],
                alias: None,
                source: None,
            },
            UseDecl {
                visibility: Visibility::Private,
                path: vec!["net".into(), "http".into(), "client".into()],
                alias: Some("other_client".into()),
                source: None,
            },
            UseDecl {
                visibility: Visibility::Public,
                path: vec!["net".into(), "http".into(), "get".into()],
                alias: None,
                source: None,
            },
            UseDecl {
                visibility: Visibility::Public,
                path: vec!["net".into(), "http".into(), "post".into()],
                alias: Some("send".into()),
                source: None,
            },
            UseDecl {
                visibility: Visibility::Package,
                path: vec!["root".into(), "core".into(), "value".into()],
                alias: None,
                source: None,
            },
        ]
    );
    assert_eq!(program.items.len(), 1);
    assert_eq!(program.item_visibilities, vec![Visibility::Private]);
}

#[test]
fn parses_qualified_let_bindings_as_transparent_entity_aliases() {
    let mut program = parse(
        "let option = core.option\n\
             let http_client = net.http.client\n\
             pub let status = net.http.status\n\
             pub(package) let value = root.core.value\n\
             let snapshot = (object.member)\n\
             let answer = 42\n",
    )
    .unwrap();
    assert!(program
        .uses
        .iter()
        .all(|declaration| declaration.source.is_some()));
    for declaration in &mut program.uses {
        declaration.source = None;
    }

    assert_eq!(
        program.uses,
        vec![
            UseDecl {
                visibility: Visibility::Private,
                path: vec!["core".into(), "option".into()],
                alias: Some("option".into()),
                source: None,
            },
            UseDecl {
                visibility: Visibility::Private,
                path: vec!["net".into(), "http".into(), "client".into()],
                alias: Some("http_client".into()),
                source: None,
            },
            UseDecl {
                visibility: Visibility::Public,
                path: vec!["net".into(), "http".into(), "status".into()],
                alias: Some("status".into()),
                source: None,
            },
            UseDecl {
                visibility: Visibility::Package,
                path: vec!["root".into(), "core".into(), "value".into()],
                alias: Some("value".into()),
                source: None,
            },
        ]
    );
    assert_eq!(program.items.len(), 2);
    assert_eq!(
        program.item_visibilities,
        vec![Visibility::Private, Visibility::Private]
    );
    assert!(matches!(
        &program.items[0],
        Item::Global(Binding {
            name,
            value: Expr::Member(_, member),
            ..
        }) if name == "snapshot" && member == "member"
    ));
}

#[test]
fn rejects_empty_duplicate_and_invalid_imports() {
    let empty = parse("use net.http.{}\n").unwrap_err();
    assert!(empty.message.contains("cannot be empty"));

    let duplicate = parse("use net.http.{get, post as get}\n").unwrap_err();
    assert!(duplicate.message.contains("duplicate import binding `get`"));

    let duplicates_for_semantic_resolution = parse("use net.http.get\nuse other.get\n").unwrap();
    assert_eq!(duplicates_for_semantic_resolution.uses.len(), 2);

    for alias in ["self", "_"] {
        let error = parse(&format!("use net.http.client as {alias}\n")).unwrap_err();
        assert!(error.message.contains("cannot be used as an import alias"));
    }

    let missing_alias = parse("use net.http.client as\n").unwrap_err();
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
        "let bad(): foo.root.value = { 0 }\n",
        "let bad(): i32 = { root.super.value }\n",
        "let bad(value: root.option): i32 = { value match { root.super.option.none => 0 } }\n",
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains("first path segment"), "{error:?}");
    }

    parse(
        "let ok(value: super.super.model.value): i32 = { super.super.api.read(root.self.value) }\n",
    )
    .unwrap();
}

#[test]
fn accepts_root_super_and_contextual_self_in_ordinary_paths() {
    let program = parse(
            "let resolve(value: root.model.value): super.model.result = { root.api.call(super.value) }\n\
             let unwrap(value: root.option): i32 = { value match { root.option.some(self) => self } }\n",
        )
        .unwrap();

    let Item::Function(resolve) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        resolve.groups[0][0].ty,
        Type::Named("root.model.value".into(), Vec::new())
    );
    assert_eq!(
        resolve.return_type,
        Some(Type::Named("super.model.result".into(), Vec::new()))
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
    let (_, cases) = match_call_parts(function_tail(unwrap));
    let Expr::PatternClosure { pattern, .. } = cases[0] else {
        panic!("expected pattern closure");
    };
    assert!(matches!(
        pattern,
        Pattern::Constructor { path, fields: PatternFields::Positional(fields) }
            if path == &vec!["root".to_owned(), "option".to_owned(), "some".to_owned()]
                && fields == &vec![Pattern::Binding("self".into())]
    ));
}

#[test]
fn parses_dotted_type_paths() {
    let program =
        parse("let convert(value: net.http.point): net.http.result(core.status) = { value }\n")
            .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };

    assert_eq!(
        function.groups[0][0].ty,
        Type::Named("net.http.point".into(), Vec::new())
    );
    assert_eq!(
        function.return_type,
        Some(Type::Named(
            "net.http.result".into(),
            vec![Type::Named("core.status".into(), Vec::new())],
        ))
    );
}

#[test]
fn rejects_visibility_where_it_is_not_supported_yet() {
    let extension = parse("pub extend(thing) {}\n").unwrap_err();
    assert!(extension
        .message
        .contains("`extend` declarations cannot have visibility"));

    let trait_member = parse("let protocol = trait { pub let f(value: i32): i32 }\n").unwrap_err();
    assert!(trait_member.message.contains("trait members"));

    let extend_member = parse("extend(thing) { pub(package) let answer = 42 }\n").unwrap_err();
    assert!(extend_member.message.contains("extend members"));
}

#[test]
fn separates_compile_time_and_runtime_parameter_groups() {
    let program = parse(
        "let identity(comptime t: type)(value: t): t = { value }\n\
             let staged(comptime t: type)(comptime u: type)(value: t): u = { value }\n",
    )
    .unwrap();

    let Item::Function(identity) = &program.items[0] else {
        panic!("expected generic function");
    };
    assert_eq!(identity.compile_groups.len(), 1);
    assert_eq!(identity.compile_groups[0].len(), 1);
    assert_eq!(identity.compile_groups[0][0].name, "t");
    assert_eq!(identity.compile_groups[0][0].kind, Sort::Type);
    assert_eq!(identity.groups.len(), 1);
    assert_eq!(
        identity.groups[0][0].ty,
        Type::Named("t".into(), Vec::new())
    );
    assert_eq!(
        identity.return_type,
        Some(Type::Named("t".into(), Vec::new()))
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
    let program =
        parse("let choose(comptime t: type, comptime u: type)(value: t): u = { value }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected generic function");
    };
    assert_eq!(function.compile_groups.len(), 1);
    assert_eq!(
        function.compile_groups[0]
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>(),
        vec!["t", "u"]
    );
}

#[test]
fn parses_generic_structs_and_enums() {
    let program = parse(
        "let cell(comptime t: type) = struct { value: t }\n\
             let maybe(comptime t: type) = enum {\n\
               some(t),\n\
               named(value: t),\n\
               none,\n\
             }\n",
    )
    .unwrap();

    let Item::Struct(cell) = &program.items[0] else {
        panic!("expected generic struct");
    };
    assert_eq!(cell.compile_groups[0][0].name, "t");
    assert_eq!(cell.fields[0].ty, Type::Named("t".into(), Vec::new()));

    let Item::Enum(maybe) = &program.items[1] else {
        panic!("expected generic enum");
    };
    assert_eq!(maybe.compile_groups[0][0].kind, Sort::Type);
    assert!(matches!(
        &maybe.variants[0].fields,
        VariantFields::Positional(types)
            if types == &vec![Type::Named("t".into(), Vec::new())]
    ));
    assert!(matches!(
        &maybe.variants[1].fields,
        VariantFields::Named(fields)
            if fields[0].ty == Type::Named("t".into(), Vec::new())
    ));
}

#[test]
fn parses_c_struct_representation_independently_of_derives() {
    let program = parse(
        "let timespec = struct(c) { seconds: i64, nanoseconds: i64 }\n\
             let pair = struct(derive: copyable, c) { left: i32, right: i32 }\n",
    )
    .unwrap();

    let Item::Struct(timespec) = &program.items[0] else {
        panic!("expected c representation struct");
    };
    assert_eq!(timespec.representation, StructRepresentation::C);
    assert!(timespec.derives.is_empty());

    let Item::Struct(pair) = &program.items[1] else {
        panic!("expected derived c representation struct");
    };
    assert_eq!(pair.representation, StructRepresentation::C);
    assert_eq!(pair.derives, ["copyable"]);

    for source in [
        "let bad = struct(system) { value: i32 }\n",
        "let bad = struct(c, c) { value: i32 }\n",
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains("struct representation"));
    }
}

#[test]
fn parses_empty_structs_as_types_that_can_be_extended() {
    let program = parse(
        "let marker = struct {}\n\
             extend(marker) {\n\
               let answer(): i32 = { 42 }\n\
             }\n",
    )
    .unwrap();

    let Item::Struct(marker) = &program.items[0] else {
        panic!("expected empty struct");
    };
    assert_eq!(marker.name, "marker");
    assert!(marker.fields.is_empty());

    let Item::Extend(extension) = &program.items[1] else {
        panic!("expected extension");
    };
    assert_eq!(
        extension.target,
        Type::Named("marker".to_owned(), Vec::new())
    );
    assert_eq!(extension.members.len(), 1);

    let error = parse("let namespace = struct { let value = 42 }\n").unwrap_err();
    assert!(
        error.message.contains("expected a field name"),
        "{}",
        error.message
    );
}

#[test]
fn parses_trait_method_signatures_and_associated_types() {
    let program = parse(
        "let foo = trait {\n\
               let f(self: borrow(self))(x: i32): i32;\n\
               let item: type\n\
             }\n",
    )
    .unwrap();

    let Item::Trait(definition) = &program.items[0] else {
        panic!("expected trait definition");
    };
    assert_eq!(definition.name, "foo");
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
            pointee: Box::new(Type::Named("self".into(), Vec::new())),
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
        ..
    } = &definition.members[1]
    else {
        panic!("expected associated type");
    };
    assert_eq!(name, "item");
    assert!(compile_groups.is_empty());
    assert_eq!(default, &None);
}

#[test]
fn preserves_generic_traits_and_trait_member_defaults() {
    let program = parse(
        "let convert(comptime t: type) = trait {\n\
               let convert(comptime u: type)(self: borrow(self))(value: u): t = { value }\n\
               let output(comptime v: type): type = pair(t, v)\n\
             }\n",
    )
    .unwrap();

    let Item::Trait(definition) = &program.items[0] else {
        panic!("expected generic trait definition");
    };
    assert_eq!(definition.compile_groups.len(), 1);
    assert_eq!(definition.compile_groups[0][0].name, "t");

    let TraitMember::Function(function) = &definition.members[0] else {
        panic!("expected default method");
    };
    assert_eq!(function.compile_groups[0][0].name, "u");
    assert_eq!(
        function.return_type,
        Some(Type::Named("t".into(), Vec::new()))
    );
    assert_eq!(function_tail(function), &Expr::Name("value".into()));

    let TraitMember::AssociatedType {
        name,
        compile_groups,
        default,
        ..
    } = &definition.members[1]
    else {
        panic!("expected generic associated type");
    };
    assert_eq!(name, "output");
    assert_eq!(compile_groups[0][0].name, "v");
    assert_eq!(
        default,
        &Some(Type::Named(
            "pair".into(),
            vec![
                Type::Named("t".into(), Vec::new()),
                Type::Named("v".into(), Vec::new()),
            ],
        ))
    );
}

#[test]
fn preserves_region_and_access_generic_associated_type_groups() {
    let program = parse(
            "let lend = trait {\n\
               let item(comptime a: access)(comptime r: region): type\n\
               let view(comptime a: access, comptime r: region)(self: borrow(a)(r)(self))(): item(a)(r)\n\
             }\n",
        )
        .unwrap();
    let Item::Trait(definition) = &program.items[0] else {
        panic!("expected trait");
    };
    let TraitMember::AssociatedType { compile_groups, .. } = &definition.members[0] else {
        panic!("expected generic associated type");
    };
    assert_eq!(compile_groups.len(), 2);
    assert_eq!(compile_groups[0][0].kind, Sort::Named("access".into()));
    assert_eq!(compile_groups[1][0].kind, Sort::Region);
}

#[test]
fn rejects_runtime_parameter_groups_on_associated_types() {
    let error = parse("let broken = trait { let item(value: i32): type }\n").unwrap_err();
    assert!(error
        .message
        .contains("cannot have runtime parameter groups"));
}

#[test]
fn rejects_removed_underscore_inference_syntax() {
    for source in [
        "let value: cell(_) = cell(i32) { value: 20 }\n",
        "let value = cell(_) { value: 20 }\n",
        "let value = cell(cell(_)) { value: cell(i32) { value: 20 } }\n",
        "let value = _\n",
        "let value: array(i32)(_) = []\n",
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains("`_`"));
        assert!(error.message.contains("inference") || error.message.contains("inferred"));
    }
}

#[test]
fn parses_unsafe_raw_pointer_dereference_and_assignment() {
    let program = parse(
            "let main(): i32 = {\n  let mut value = 41\n  let pointer = ptr(mut)(borrow(mut)(value))\n  unsafe {\n    *pointer = *pointer + 1\n  }\n  value\n}\n",
        )
        .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Some(Expr::Block(statements, _)) = &function.body else {
        panic!("expected function block");
    };
    let Stmt::Expr(unsafe_expression) = &statements[2] else {
        panic!("expected unsafe expression");
    };
    assert!(matches!(
        unsafe_expression.unlocated(),
        Expr::Unsafe(body)
            if matches!(body.as_ref(), Expr::DoBlock { body } if matches!(
                body.as_ref(), Expr::Block(_, Some(tail)) if matches!(
                    tail.unlocated(),
                    Expr::Assign(left, _)
                        if matches!(left.as_ref(), Expr::Unary(UnaryOp::Deref, _))
                )
            ))
    ));

    let program = parse("let main(): () = { unsafe do {} }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(matches!(
        function_tail(function),
        Expr::Call(callee, arguments)
            if matches!(callee.as_ref(), Expr::Name(name) if name == "unsafe")
                && matches!(arguments.as_slice(), [CallArg {
                    value: Expr::DoBlock { .. },
                    ..
                }])
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
        "let cell = cell(i32) { value: 42 }\n\
             let nested = cell(cell(i32)) { value: 42 }\n\
             let some = maybe(i32).some(42)\n\
             let none = maybe(i32).none\n",
    )
    .unwrap();

    let Item::Global(cell) = &program.items[0] else {
        panic!("expected cell binding");
    };
    assert_eq!(
        cell.value,
        Expr::StructLiteral {
            constructor: Box::new(type_head("cell", Expr::Name("i32".into()))),
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
                "cell",
                type_head("cell", Expr::Name("i32".into())),
            )),
            fields: vec![argument(Some("value"), Expr::Integer(42))],
        }
    );

    let Item::Global(some) = &program.items[2] else {
        panic!("expected some binding");
    };
    assert_eq!(
        some.value,
        Expr::Call(
            Box::new(Expr::Member(
                Box::new(type_head("maybe", Expr::Name("i32".into()))),
                "some".into(),
            )),
            vec![argument(None, Expr::Integer(42))],
        )
    );

    let Item::Global(none) = &program.items[3] else {
        panic!("expected none binding");
    };
    assert_eq!(
        none.value,
        Expr::Member(
            Box::new(type_head("maybe", Expr::Name("i32".into()))),
            "none".into(),
        )
    );
}

#[test]
fn rejects_mixed_or_misordered_compile_parameter_groups() {
    let cases = [
        (
            "let bad(value: i32)(comptime t: type): i32 = { value }\n",
            "cannot be mixed",
        ),
        (
            "let bad(comptime t: type, value: t): t = { value }\n",
            "expected `comptime`",
        ),
        (
            "let bad(value: t, comptime u: type): t = { value }\n",
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
    for name in [
        "_", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
        "usize", "bool", "never",
    ] {
        let source = format!("let invalid(comptime {name}: type)(value: i32): i32 = {{ value }}\n");
        let error = parse(&source).unwrap_err();
        assert_eq!(
            error.message,
            format!("reserved type name `{name}` cannot be used as a compile-time parameter")
        );
        assert_eq!((error.line, error.column), (1, 22));
    }
}

#[test]
fn parses_bounded_c_foreign_declarations() {
    let program = parse(
        "pub let c_abs(value: i32): i32 = foreign(c, \"abs\")\n\
             let strlen(value: ptr(u8)): usize = foreign(c)\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected foreign function");
    };
    let foreign = function.foreign.as_ref().expect("foreign metadata");
    assert_eq!(foreign.abi, ForeignAbi::C);
    assert_eq!(foreign.link_name, "abs");
    assert!(function.effects.unsafety);
    assert!(function.body.is_none());
    assert_eq!(function.groups.len(), 1);
    assert_eq!(program.item_visibilities[0], Visibility::Public);

    let Item::Function(default_symbol) = &program.items[1] else {
        panic!("expected foreign function with default symbol");
    };
    assert_eq!(
        default_symbol
            .foreign
            .as_ref()
            .expect("foreign metadata")
            .link_name,
        "strlen"
    );

    let legacy = parse("extern \"c\" { let abs(value: i32): i32 }\n").unwrap_err();
    assert!(legacy.message.contains("grouped `extern`"));

    for (source, expected) in [
        (
            "let value = foreign(c)\n",
            "requires one runtime parameter group",
        ),
        (
            "let identity(comptime t: type)(value: t): t = foreign(c)\n",
            "cannot be generic",
        ),
        (
            "let abs(value: i32) = foreign(c)\n",
            "require an explicit result type",
        ),
        (
            "let abs(value: i32): i32 with(unsafety) = foreign(c)\n",
            "cannot declare effects",
        ),
        (
            "let abs(value: i32): i32 = foreign(c, \"\")\n",
            "non-empty ASCII linker symbol",
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert!(
            error.message.contains(expected),
            "`{source}` did not report `{expected}`: {}",
            error.message
        );
    }
}

#[test]
fn rejects_runtime_parameters_on_generic_data_and_legacy_extend_headers() {
    let data = parse("let bad(comptime t: type)(value: t) = struct { value: t }\n").unwrap_err();
    assert!(data.message.contains("runtime parameters"));

    let extension = parse("extend cell {}\n").unwrap_err();
    assert!(extension.message.contains("`(` after `extend`"));
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
    let [Stmt::Expr(expression)] = statements.as_slice() else {
        panic!("expected one expression statement");
    };
    assert!(matches!(
        expression.unlocated(),
        Expr::Call(_, arguments)
            if matches!(
                arguments.as_slice(),
                [CallArg {
                    label: None,
                    value: Expr::Integer(1)
                }]
            )
    ));
    assert_eq!(tail.unlocated(), &Expr::Integer(2));
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
    assert_eq!(tail.unlocated(), &Expr::Name("x".into()));
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
    assert_eq!(tail.unlocated(), &Expr::Unit);
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
        let Stmt::Expr(expression) = statement else {
            panic!("expected compound assignment");
        };
        assert!(matches!(
            expression.unlocated(),
            Expr::CompoundAssign(_, found, _) if *found == operator
        ));
    }
}

#[test]
fn parses_do_if_else_and_return() {
    let program = parse(
        "let choose(flag: bool): i32 = { do {\n\
               if flag { return(1) }\n\
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
fn rejects_removed_if_let_syntax() {
    let error = parse(
        "let choose(value: option(i32)): i32 = {\n\
               if let some(found) = value { found } else { 0 }\n\
             }\n",
    )
    .unwrap_err();
    assert!(!error.message.is_empty());
}

#[test]
fn parses_throw_as_a_core_error_function() {
    let program = parse("let fail(): result(bool)(i32) = { throw(false) }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        function_tail(function),
        &Expr::Call(
            Box::new(Expr::Member(
                Box::new(Expr::Member(
                    Box::new(Expr::Name("core".to_owned())),
                    "error".to_owned(),
                )),
                "throw".to_owned(),
            )),
            vec![CallArg {
                label: None,
                value: Expr::Bool(false),
            }],
        )
    );

    let program = parse("let fail(): result(bool)(i32) = { throw false }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        function_tail(function),
        &Expr::Call(
            Box::new(Expr::Member(
                Box::new(Expr::Member(
                    Box::new(Expr::Name("core".to_owned())),
                    "error".to_owned(),
                )),
                "throw".to_owned(),
            )),
            vec![CallArg {
                label: None,
                value: Expr::Bool(false),
            }],
        )
    );
}

#[test]
fn parses_do_and_try_as_distinct_immediate_handlers() {
    let program = parse(
        "let main(): result(bool)(i32) = { try { 42 } }\n\
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

    let member =
        parse("let unwrap(value: result(bool)(i32)): i32 with(throwing(bool)) = { value.try }\n")
            .unwrap();
    let Item::Function(member) = &member.items[0] else {
        panic!("expected function");
    };
    assert!(matches!(
        function_tail(member),
        Expr::Member(_, name) if name == "try"
    ));

    let program = parse("let value: result(bool)(i32) = try do { 42 }\n").unwrap();
    let Item::Global(binding) = &program.items[0] else {
        panic!("expected global");
    };
    assert!(matches!(
        &binding.value,
        Expr::Call(callee, arguments)
            if matches!(callee.as_ref(), Expr::Name(name) if name == "try")
                && matches!(arguments.as_slice(), [CallArg {
                    value: Expr::DoBlock { .. },
                    ..
                }])
    ));
}

#[test]
fn parses_parenthesis_free_unary_call_groups() {
    let program = parse(
        "let value = transform input\n\
             let curried = combine left right\n\
             let mapped = map values { (value: i32) -> value + 1 }\n\
             let method = receiver.shift amount\n",
    )
    .unwrap();
    let values = program
        .items
        .iter()
        .map(|item| match item {
            Item::Global(binding) => &binding.value,
            _ => panic!("expected global"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        values[0],
        Expr::Call(callee, arguments)
            if matches!(callee.as_ref(), Expr::Name(name) if name == "transform")
                && matches!(arguments.as_slice(), [CallArg {
                    value: Expr::Name(name),
                    ..
                }] if name == "input")
    ));
    assert!(matches!(
        values[1],
        Expr::Call(callee, right)
            if matches!(right.as_slice(), [CallArg {
                value: Expr::Name(name),
                ..
            }] if name == "right")
                && matches!(
                    callee.as_ref(),
                    Expr::Call(inner, left)
                        if matches!(inner.as_ref(), Expr::Name(name) if name == "combine")
                            && matches!(left.as_slice(), [CallArg {
                                value: Expr::Name(name),
                                ..
                            }] if name == "left")
                )
    ));
    assert!(matches!(
        values[2],
        Expr::Call(callee, closure)
            if matches!(closure.as_slice(), [CallArg {
                value: Expr::Closure(_, _),
                ..
            }])
                && matches!(
                    callee.as_ref(),
                    Expr::Call(_, values) if values.len() == 1
                )
    ));
    assert!(matches!(
        values[3],
        Expr::Call(callee, arguments)
            if matches!(callee.as_ref(), Expr::Member(_, member) if member == "shift")
                && arguments.len() == 1
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
fn trailing_closure_can_supply_the_first_call_group() {
    let program = parse("let invoke(): () = { run { cleanup() } }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Expr::Call(callee, arguments) = function_tail(function) else {
        panic!("expected trailing closure call");
    };
    assert!(matches!(callee.as_ref(), Expr::Name(name) if name == "run"));
    let [argument] = arguments.as_slice() else {
        panic!("expected one closure argument");
    };
    assert!(matches!(argument.value.unlocated(), Expr::Closure(_, _)));
}

#[test]
fn multiple_trailing_closures_create_successive_call_groups() {
    let program = parse(
        "let value = choose()\n\
               { true }\n\
               { 1 }\n",
    )
    .unwrap();
    let Item::Global(binding) = &program.items[0] else {
        panic!("expected global");
    };
    let Expr::Call(second_call, second_group) = &binding.value else {
        panic!("expected second trailing call group");
    };
    assert_eq!(second_group.len(), 1);
    let Expr::Call(first_call, first_group) = second_call.as_ref() else {
        panic!("expected first trailing call group");
    };
    assert_eq!(first_group.len(), 1);
    assert!(matches!(first_call.as_ref(), Expr::Call(_, arguments) if arguments.is_empty()));

    let program = parse("let value = choose true { 1 } { 0 }\n").unwrap();
    let Item::Global(binding) = &program.items[0] else {
        panic!("expected global");
    };
    let Expr::Call(second_call, _) = &binding.value else {
        panic!("expected second trailing group");
    };
    let Expr::Call(first_call, _) = second_call.as_ref() else {
        panic!("expected first trailing group");
    };
    assert!(matches!(
        first_call.as_ref(),
        Expr::Call(_, arguments)
            if matches!(arguments.as_slice(), [CallArg { value: Expr::Bool(true), .. }])
    ));
}

#[test]
fn named_trailing_closures_create_labeled_call_groups() {
    let program = parse("let value = choose() condition { true } body { 1 }\n").unwrap();
    let Item::Global(binding) = &program.items[0] else {
        panic!("expected global");
    };
    let Expr::Call(first_call, body_group) = &binding.value else {
        panic!("expected body group");
    };
    assert_eq!(body_group[0].label.as_deref(), Some("body"));
    let Expr::Call(_, condition_group) = first_call.as_ref() else {
        panic!("expected condition group");
    };
    assert_eq!(condition_group[0].label.as_deref(), Some("condition"));
}

#[test]
fn handler_member_accepts_implicit_named_trailing_groups() {
    let program = parse(
        "let run(): i32 = {\n\
               ask.handle\n\
                 value { (resume) -> resume(42) }\n\
                 done { (answer) -> answer }\n\
                 action { ask.value() }\n\
             }\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Some(Expr::Block(_, Some(value))) = &function.body else {
        panic!("expected function body");
    };
    let Expr::Call(done_call, action_group) = value.unlocated() else {
        panic!("expected action group");
    };
    assert_eq!(action_group[0].label.as_deref(), Some("action"));
    let Expr::Call(value_call, done_group) = done_call.as_ref() else {
        panic!("expected done group");
    };
    assert_eq!(done_group[0].label.as_deref(), Some("done"));
    let Expr::Call(handler, value_group) = value_call.as_ref() else {
        panic!("expected value group");
    };
    assert_eq!(value_group[0].label.as_deref(), Some("value"));
    assert!(matches!(handler.as_ref(), Expr::Member(_, member) if member == "handle"));
}

#[test]
fn handler_action_ends_the_trailing_group_sequence() {
    let program = parse(
        "let run(): i32 = {\n\
               let ignored = iteration_skip.handle\n\
                 next { () }\n\
                 action { () }\n\
               if true { 42 } else { 0 }\n\
             }\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Some(Expr::Block(statements, Some(tail))) = &function.body else {
        panic!("expected function body");
    };
    assert_eq!(statements.len(), 1);
    let (condition, then_branch, else_branch) = if_call_parts(tail);
    assert_eq!(condition, &Expr::Bool(true));
    assert!(matches!(then_branch, Expr::Block(_, _)));
    assert!(matches!(else_branch, Expr::Block(_, _)));
}

#[test]
fn rejects_legacy_handler_and_for_forms() {
    for (source, expected) in [
        (
            "let run(): i32 = { ask.handle(value: { (resume) -> resume(42) }) { ask.value() } }\n",
            "`handle` clauses use named trailing groups",
        ),
        (
            "let run(): i32 = { ask.handle value { (resume) -> resume(42) } { ask.value() } }\n",
            "handler action must use the named trailing group",
        ),
        (
            "let run(values: values): () = { for value in values { consume(value) } }\n",
            "trailing pattern closure",
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn named_nested_trailing_calls_are_implicitly_closed() {
    let program =
        parse("let value = choose(true) { 1 } otherwise choose(false) { 2 } { 3 }\n").unwrap();
    let Item::Global(binding) = &program.items[0] else {
        panic!("expected global");
    };
    let Expr::Call(_, group) = &binding.value else {
        panic!("expected named trailing group");
    };
    assert_eq!(group[0].label.as_deref(), Some("otherwise"));
    assert!(matches!(group[0].value, Expr::Closure(_, _)));
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
        "extend(cell) { let read(self: borrow(self))(): i32 = self.value }\n",
        "let read = trait { let read(self: borrow(self))(): i32 = 42 }\n",
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains("require a braced body"), "{error:?}");
    }

    parse("let answer = 42\nlet read(): i32 = { 42 }\n").unwrap();
}

#[test]
fn parses_structs_and_enum_field_shapes() {
    let program = parse(
        "let point = struct { x: i32, pub(package) y: i32, pub z: i32 }\n\
             let documented = struct {\n\
               /// a field with a documentation comment.\n\
               value: i32,\n\
             }\n\
             let shape = enum {\n\
               circle(pub radius: i32, pub(package) center: point, label: i32),\n\
               record(\n\
                 /// a named variant field with a documentation comment.\n\
                 item: i32,\n\
               ),\n\
               pair(i32, i32),\n\
               unit,\n\
             }\n",
    )
    .unwrap();

    let Item::Struct(point) = &program.items[0] else {
        panic!("expected struct");
    };
    assert_eq!(point.name, "point");
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
        "let point = struct { x: i32, y: i32 }\n\
             let main(): i32 = {\n\
               let mut point = point { x: 1, y: 2 }\n\
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
    let Stmt::Expr(assignment) = &statements[1] else {
        panic!("expected assignment");
    };
    assert!(matches!(
        assignment.unlocated(),
        Expr::Assign(left, right)
            if matches!(left.as_ref(), Expr::Member(_, field) if field == "x")
                && right.as_ref() == &Expr::Integer(3)
    ));
    assert!(matches!(tail.unlocated(), Expr::Member(_, field) if field == "x"));
}

#[test]
fn parses_postfix_match_patterns_and_guards() {
    let program = parse(
        "let classify(shape: shape): i32 = { shape match {\n\
               shape.circle(radius: value) if value > 0 => value,\n\
               shape.unit => 0,\n\
               _ => -1,\n\
             } }\n",
    )
    .unwrap();

    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let (input, cases) = match_call_parts(function_tail(function));
    assert_eq!(input, &Expr::Name("shape".into()));
    assert_eq!(cases.len(), 3);
    let Expr::PatternClosure { pattern, guard, .. } = cases[0] else {
        panic!("expected pattern closure");
    };
    assert!(guard.is_some());
    assert!(matches!(
        pattern,
        Pattern::Constructor { path, fields: PatternFields::Named(fields) }
            if path == &vec!["shape".to_owned(), "circle".to_owned()]
                && fields[0].name == "radius"
    ));
    assert!(matches!(
        cases[2],
        Expr::PatternClosure {
            pattern: Pattern::Wildcard,
            ..
        }
    ));
}

#[test]
fn parses_prefix_match_as_trailing_pattern_cases() {
    let program = parse(
        "let classify(shape: shape): i32 = {\n\
               match shape\n\
                 { shape.circle(radius: value) if value > 0 ->\n\
                   let adjusted = value + 1\n\
                   adjusted\n\
                 }\n\
                 { shape.unit -> 0 }\n\
                 { _ -> -1 }\n\
             }\n",
    )
    .unwrap();

    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let (input, cases) = match_call_parts(function_tail(function));
    assert_eq!(input, &Expr::Name("shape".into()));
    assert_eq!(cases.len(), 3);
    let Expr::PatternClosure {
        pattern,
        guard,
        body,
    } = cases[0]
    else {
        panic!("expected pattern closure");
    };
    assert!(guard.is_some());
    assert!(matches!(
        body.as_ref(),
        Expr::Block(statements, Some(_)) if statements.len() == 1
    ));
    assert!(matches!(
        pattern,
        Pattern::Constructor { path, fields: PatternFields::Named(fields) }
            if path == &vec!["shape".to_owned(), "circle".to_owned()]
                && fields[0].name == "radius"
    ));
    assert!(matches!(
        cases[2],
        Expr::PatternClosure {
            pattern: Pattern::Wildcard,
            ..
        }
    ));
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
    let (input, cases) = match_call_parts(&matched.value);
    assert!(matches!(input, Expr::Coalesce(_, _)));
    assert_eq!(cases.len(), 1);

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
        "let read(values: array(i32)(2)): i32 = {\n\
               let local: array(i32)(3) = [\n\
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
        Type::ArrayApplication {
            constructor: "array".to_owned(),
            element: Box::new(Type::I32),
            length: USizeConst::Literal(2),
        }
    );
    let Some(Expr::Block(statements, Some(tail))) = &function.body else {
        panic!("expected block");
    };
    let Stmt::Let(binding) = &statements[0] else {
        panic!("expected local array binding");
    };
    assert_eq!(
        binding.annotation,
        Some(Type::ArrayApplication {
            constructor: "array".to_owned(),
            element: Box::new(Type::I32),
            length: USizeConst::Literal(3),
        })
    );
    assert!(matches!(&binding.value, Expr::Array(elements) if elements.len() == 3));
    assert!(matches!(
        tail.unlocated(),
        Expr::Binary(left, BinaryOp::Add, right)
            if matches!(left.as_ref(), Expr::Index { .. })
                && matches!(right.as_ref(), Expr::Index { .. })
    ));
}

#[test]
fn parses_pure_static_expressions_in_dependent_array_lengths() {
    let program = parse(
        "let next(value: usize): usize = { value + 1 }\n\
             let consume(values: array(i32)(next(2) * 2)): i32 = { values[0] }\n",
    )
    .unwrap();
    let Item::Function(consume) = &program.items[1] else {
        panic!("expected consume function");
    };
    assert_eq!(
        consume.groups[0][0].ty,
        Type::ArrayApplication {
            constructor: "array".into(),
            element: Box::new(Type::I32),
            length: USizeConst::Expression(Box::new(StaticExpr::Binary(
                Box::new(StaticExpr::Call {
                    function: "next".into(),
                    groups: vec![vec![StaticCallArg {
                        label: None,
                        value: StaticExpr::USize(2),
                    }]],
                }),
                BinaryOp::Mul,
                Box::new(StaticExpr::USize(2)),
            ))),
        }
    );
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
    let Stmt::Expr(assignment) = &statements[1] else {
        panic!("expected assignment");
    };
    assert!(matches!(
        assignment.unlocated(),
        Expr::Assign(left, _) if matches!(left.as_ref(), Expr::Index { .. })
    ));
    assert!(matches!(
        &statements[2],
        Stmt::Let(Binding {
            value: Expr::Borrow { value, .. },
            ..
        }) if matches!(value.as_ref(), Expr::Index { .. })
    ));
    assert!(matches!(tail.unlocated(), Expr::Index { .. }));
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
            "let choose(comptime r: region)(value: borrow(r)(i32)): borrow(r)(i32) = { borrow(value) }\n",
        )
        .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(function.compile_groups[0][0].name, "r");
    assert_eq!(function.compile_groups[0][0].kind, Sort::Region);
    assert_eq!(
        function.groups[0][0].ty,
        Type::Borrow {
            mutable: false,
            access: None,
            region: Some("r".to_owned()),
            pointee: Box::new(Type::I32),
        }
    );
    assert_eq!(
        function.return_type,
        Some(Type::Borrow {
            mutable: false,
            access: None,
            region: Some("r".to_owned()),
            pointee: Box::new(Type::I32),
        })
    );
}

#[test]
fn parses_access_parameters_in_borrow_modes_types_and_expressions() {
    let program = parse(
        "let identity(comptime a: access, comptime r: region, comptime t: type)\n\
               (value: borrow(a)(r)(t)): borrow(a)(r)(t) = { borrow(a)(value) }\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(function.compile_groups[0][0].kind.is_access());
    assert_eq!(
        function.groups[0][0].ty,
        Type::Borrow {
            mutable: false,
            access: Some("a".to_owned()),
            region: Some("r".to_owned()),
            pointee: Box::new(Type::Named("t".into(), Vec::new())),
        }
    );
    assert!(matches!(
        function.return_type,
        Some(Type::Borrow {
            mutable: false,
            access: Some(ref access),
            region: Some(ref region),
            ..
        }) if access == "a" && region == "r"
    ));
    assert!(matches!(
        function_tail(function),
        Expr::Borrow {
            mutable: false,
            access: Some(ref access),
            ..
        } if access == "a"
    ));
}

#[test]
fn parses_closed_types_as_compile_parameter_types() {
    let program = parse(
        "let optimization = enum { size, speed }\n\
             let select(comptime b: bool, comptime o: optimization)(value: i32): i32 = { value }\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[1] else {
        panic!("expected a function");
    };
    assert!(matches!(
        &function.compile_groups[0][0].kind,
        Sort::Named(name) if name == "bool"
    ));
    assert!(matches!(
        &function.compile_groups[0][1].kind,
        Sort::Named(name) if name == "optimization"
    ));
}

#[test]
fn parses_string_as_an_ordinary_named_type() {
    let program = parse(
        "let register(comptime name: string)(move body: with(core.error.throwing(core.string.string))((): ())): () = builtin()\n",
    )
    .unwrap();
    let [Item::Function(function)] = program.items.as_slice() else {
        panic!("expected one metadata function");
    };
    assert_eq!(
        function.compile_groups,
        vec![vec![CompileParam {
            name: "name".to_owned(),
            kind: Sort::Named("string".to_owned()),
            default: None,
        }]]
    );
    assert_eq!(function.groups.len(), 1);

    let runtime = parse("let identity(value: string): string = { value }\n").unwrap();
    let [Item::Function(function)] = runtime.items.as_slice() else {
        panic!("expected one runtime string function");
    };
    assert_eq!(
        function.groups[0][0].ty,
        Type::Named("string".to_owned(), Vec::new())
    );
    assert_eq!(
        function.return_type,
        Some(Type::Named("string".to_owned(), Vec::new()))
    );
}

#[test]
fn parses_parameter_modifier_functions_in_prefix_position() {
    let program = parse(
            "let identity(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = { value }\n",
        )
        .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(function.compile_groups[0][0].kind.is_parameter_modifier());
    assert_eq!(function.groups[0][0].mode, PassMode::Inferred);
    assert_eq!(function.groups[0][0].modifiers, ["m"]);
}

#[test]
fn parses_parameter_prefixes_as_composable_modifiers() {
    let program = parse(
            "let decorate(comptime b: bool, comptime m: (comptime p: parameters): parameters)(b m value: i32): i32 = { value }\n",
        )
        .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected a function");
    };
    assert_eq!(function.groups[0][0].modifiers, ["b", "m"]);
    assert_eq!(function.groups[0][0].mode, PassMode::Inferred);
}

#[test]
fn parses_parameter_modifier_function_kind() {
    let program = parse(
            "let identity(comptime m: (comptime p: parameters): parameters, comptime t: type)(m value: t): t = { value }\n",
        )
        .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(function.compile_groups[0][0].kind, Sort::ParameterModifier);
    assert_eq!(function.groups[0][0].modifiers, ["m"]);
}

#[test]
fn parses_effect_parameters_in_with_clauses() {
    let program = parse(
        "let tagged(comptime e: effects)(value: i32): i32 with(e) = { value }\n\
             let combined(comptime e: effects)(value: i32): i32 with(unsafety, e) = { value }\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(function.compile_groups[0][0].kind, Sort::Effects);
    assert_eq!(function.effects.parameters, vec!["e"]);
    let Item::Function(combined) = &program.items[1] else {
        panic!("expected function");
    };
    assert_eq!(
        combined.effects.custom,
        vec![Type::Named("unsafety".to_owned(), Vec::new())]
    );
    assert_eq!(combined.effects.parameters, vec!["e"]);

    let error = parse("let box(comptime e: effects) = struct { value: i32 }\n").unwrap_err();
    assert!(error
        .message
        .contains("effect parameters belong to functions"));

    let error = parse("let bad(comptime e: effects)(value: e): i32 with(e) = { 0 }\n").unwrap_err();
    assert!(error.message.contains("cannot be used as a runtime type"));

    let error =
        parse("let old(comptime e: effects)(value: i32): i32(e) = { value }\n").unwrap_err();
    assert!(
        error
            .message
            .contains("effect parameter `e` cannot be used as a runtime type"),
        "{}",
        error.message
    );
    assert!(!error.message.contains("was removed"));
}

#[test]
fn parses_compiler_owned_constraint_fragments_and_rejects_defaults() {
    let program = parse("let inspect(comptime c: constraint)(): () = { () }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        function.compile_groups[0][0].kind,
        Sort::Fragment(crate::ast::StaticFragmentKind::constraint())
    );

    let constraint = parse("let bad(comptime c: constraint = value)(): () = { () }\n").unwrap_err();
    assert!(constraint
        .message
        .contains("defaults for constraint fragments are not supported"));
    let runtime = parse("let bad(comptime c: constraint)(value: c): () = { () }\n").unwrap_err();
    assert!(
        runtime
            .message
            .contains("constraint fragment parameter `c` cannot be used as a runtime type"),
        "{}",
        runtime.message
    );

    let ordinary = parse("let inspect(comptime d: declaration)(): () = { () }\n").unwrap();
    let Item::Function(function) = &ordinary.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        function.compile_groups[0][0].kind,
        Sort::Named("declaration".into())
    );
}

#[test]
fn parses_trait_self_effect_parameter_in_member_rows() {
    let program = parse(
            "let handle = trait(comptime self: effect) {\n\
               let clauses(comptime value: type, comptime answer: type): parameters\n\
               let handle(comptime value: type, comptime answer: type, comptime rest: effects): with(rest) ...clauses(value, answer) (move action: with(self, rest)((): value)): answer\n\
             }\n",
        )
        .unwrap();
    let Item::Trait(definition) = &program.items[0] else {
        panic!("expected trait");
    };
    let TraitMember::AssociatedType { kind, .. } = &definition.members[0] else {
        panic!("expected clauses schema");
    };
    assert_eq!(*kind, AssociatedKind::Parameters);
    let TraitMember::Function(function) = &definition.members[1] else {
        panic!("expected handle member");
    };
    assert_eq!(
        function.groups[0][0].ty,
        Type::Named(
            "$parameter$groups$expand".to_owned(),
            vec![Type::Named(
                "clauses".to_owned(),
                vec![
                    Type::Named("value".to_owned(), Vec::new()),
                    Type::Named("answer".to_owned(), Vec::new()),
                ],
            )],
        )
    );
    assert_eq!(function.effects.parameters, vec!["rest"]);
    let Type::Function { effects, .. } = &function.groups[1][0].ty else {
        panic!("expected action callable parameter");
    };
    assert_eq!(effects.parameters, vec!["rest", "self"]);
    assert!(effects.custom.is_empty());
}

#[test]
fn parses_compiler_provided_sort_and_control_contract_declarations() {
    let program = parse(
            "pub let unsafety = effect {}\n\
             pub let throwing(comptime error: type) = effect { let raise(move error: error): never }\n\
             pub let type: sort(2)\n\
             pub let effect: sort(2)\n\
             pub let effects: sort(2)\n\
             pub let empty = sort(1) {}\n\
             pub let access = sort(1) {\n\
               /// shared read-only access.\n\
               shared\n\
               /// exclusive mutable access.\n\
               mut\n\
             }\n\
             pub let do(comptime e: effects, comptime t: type)(move action: (): t with(e)): t with(e)\n",
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
        Item::Sort(sort) if sort.name == "type" && sort.level == 2 && sort.members.is_none()
    ));
    assert!(matches!(
        &program.items[3],
        Item::Sort(sort) if sort.name == "effect" && sort.level == 2 && sort.members.is_none()
    ));
    assert!(matches!(
        &program.items[4],
        Item::Sort(sort) if sort.name == "effects" && sort.level == 2 && sort.members.is_none()
    ));
    assert!(matches!(
        &program.items[5],
        Item::Sort(sort)
            if sort.name == "empty" && sort.level == 1 && sort.members == Some(Vec::new())
    ));
    assert!(matches!(
        &program.items[6],
        Item::Sort(sort) if sort.name == "access"
            && sort.members.as_ref().is_some_and(|members| members.iter().map(String::as_str)
                .eq(["shared", "mut"])
            )
    ));
    assert!(matches!(
        &program.items[7],
        Item::Function(function) if function.name == "do" && function.body.is_none()
    ));
}

#[test]
fn parses_complete_builtin_definition_markers() {
    let program = parse(
        "let builtin() = builtin()\n\
             pub let scalar: type = builtin()\n\
             pub let family(comptime t: type)(comptime l: usize): type = builtin()\n\
             pub let intrinsic(comptime t: type)(value: t): t = builtin()\n\
             extend(i32, add(i32)) {\n\
               let output = i32\n\
               let add(self)(rhs: i32): i32 = builtin()\n\
             }\n",
    )
    .expect("builtin markers should parse as complete declaration initializers");

    assert!(matches!(
        &program.items[0],
        Item::Function(function)
            if function.name == "builtin"
                && function.builtin
                && function.body.is_none()
                && function.return_type == Some(Type::Named("never".to_owned(), Vec::new()))
    ));
    assert!(matches!(
        &program.items[1],
        Item::TypeForm(definition)
            if definition.name == "scalar" && definition.builtin
    ));
    assert!(matches!(
        &program.items[2],
        Item::TypeForm(definition)
            if definition.name == "family"
                && definition.builtin
                && definition.compile_groups.len() == 2
    ));
    assert!(matches!(
        &program.items[3],
        Item::Function(function)
            if function.name == "intrinsic" && function.builtin && function.body.is_none()
    ));
    let Item::Extend(extension) = &program.items[4] else {
        panic!("expected extension");
    };
    assert!(matches!(
        &extension.members[1],
        ExtendMember::Function(function) if function.builtin && function.body.is_none()
    ));
}

#[test]
fn rejects_malformed_builtin_definition_markers() {
    for source in [
        "let builtin(): never = builtin()\n",
        "let value: i32 = builtin()\n",
        "let intrinsic(value: i32) = builtin()\n",
        "let scalar: type = builtin(1)\n",
        "extend(i32) { let constant = builtin() }\n",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}

#[test]
fn parses_variadic_match_control_contract() {
    let program = parse(
        "pub let match(\n\
               comptime input: type,\n\
               comptime output: type,\n\
               comptime e: effects,\n\
               comptime ...cases: parameters,\n\
             ): with(e)\n\
               (move input: input)\n\
               ...cases: output\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected match function");
    };
    assert_eq!(
        function.compile_groups,
        vec![vec![
            CompileParam {
                name: "input".to_owned(),
                kind: Sort::Type,
                default: None,
            },
            CompileParam {
                name: "output".to_owned(),
                kind: Sort::Type,
                default: None,
            },
            CompileParam {
                name: "e".to_owned(),
                kind: Sort::Effects,
                default: None,
            },
            CompileParam {
                name: "cases".to_owned(),
                kind: Sort::ParameterPack,
                default: None,
            },
        ]]
    );
    assert!(matches!(
        function.groups.as_slice(),
        [input, cases]
            if input[0].name == "input"
                && cases[0].name == "cases"
                && cases[0].ty
                    == Type::Named(
                        "$parameter$groups$expand".to_owned(),
                        vec![Type::Named("cases".to_owned(), Vec::new())],
                    )
    ));
    assert_eq!(
        function.return_type,
        Some(Type::Named("output".to_owned(), Vec::new()))
    );
    assert_eq!(function.effects.parameters, vec!["e"]);
    assert!(function.body.is_none());
}

#[test]
fn parses_type_forms_and_closed_enums_from_the_type_sort() {
    let program = parse("pub let i32: type\npub let bool = enum { false, true }\n").unwrap();
    assert!(matches!(
        &program.items[0],
        Item::TypeForm(definition)
            if definition.name == "i32"
                && definition.compile_groups.is_empty()
                && definition.values.is_empty()
    ));
    assert!(matches!(
        &program.items[1],
        Item::Enum(definition)
            if definition.name == "bool"
                && definition.compile_groups.is_empty()
                && definition.variants.iter().map(|variant| variant.name.as_str())
                    .eq(["false", "true"])
    ));
}

#[test]
fn rejects_removed_type_value_syntax_and_duplicate_enum_variants() {
    let error = parse("let i32 = type\n").unwrap_err();
    assert!(error.message.contains("abstract sort"));

    let error = parse("let bool = type { false, true }\n").unwrap_err();
    assert!(error.message.contains("abstract sort"));

    let error = parse("let kind = sort\n").unwrap_err();
    assert!(error.message.contains("`(` after `sort`"));

    let error = parse("let kind = sort(0) { value }\n").unwrap_err();
    assert!(error.message.contains("`sort(0)` is invalid"));

    let error = parse("let kind: sort\n").unwrap_err();
    assert!(error.message.contains("`(` after `sort`"));

    let error = parse("let bool = enum { false, false }\n").unwrap_err();
    assert!(error.message.contains("duplicate enum variant `false`"));
}

#[test]
fn parses_nominal_marker_effect_declarations_and_callable_rows() {
    let program = parse(
        "pub let ui = effect\n\
             let render(): i32 with(ui) = { 0 }\n\
             let invoke(action: (): i32 with(ui)): i32 with(ui) = { action() }\n",
    )
    .unwrap();

    assert!(matches!(&program.items[0], Item::Effect(effect) if effect.name == "ui"));
    let Item::Function(render) = &program.items[1] else {
        panic!("expected render function");
    };
    assert_eq!(
        render.effects.custom,
        [Type::Named("ui".into(), Vec::new())]
    );
    let Item::Function(invoke) = &program.items[2] else {
        panic!("expected invoke function");
    };
    assert!(matches!(
        &invoke.groups[0][0].ty,
        Type::Function { effects, .. }
            if effects.custom == [Type::Named("ui".into(), Vec::new())]
    ));

    let duplicate = parse("let f(): i32 with(ui, ui) = { 0 }\n").unwrap_err();
    assert!(duplicate.message.contains("duplicate custom effect `ui`"));

    parse("let local_effect = effect\n")
        .expect("snake_case effect declarations are valid nominal identities");
    parse("let f(): i32 with(core.effect.ui) = { 0 }\n")
        .expect("snake_case qualified effect names are valid");
}

#[test]
fn parses_parameterized_algebraic_effect_operations() {
    let program = parse(
        "let state(comptime s: type) = effect {\n\
               let get(): s\n\
               let put(move value: s): ()\n\
             }\n\
             let program(): i32 with(state(i32)) = { 0 }\n",
    )
    .unwrap();
    let Item::Effect(state) = &program.items[0] else {
        panic!("expected state effect");
    };
    assert_eq!(state.compile_groups[0][0].name, "s");
    assert_eq!(state.operations.len(), 2);
    assert_eq!(state.operations[0].name, "get");
    assert_eq!(
        state.operations[0].return_type,
        Some(Type::Named("s".into(), Vec::new()))
    );
    assert_eq!(state.operations[1].groups[0][0].mode, PassMode::Move);

    let Item::Function(program) = &program.items[1] else {
        panic!("expected program function");
    };
    assert_eq!(
        program.effects.custom,
        [Type::Named("state".into(), vec![Type::I32])]
    );
}

#[test]
fn permits_effect_operation_overloads_only_by_parameter_names() {
    let program = parse(
        "let ask = effect {\n\
               let value(left: i32): i32\n\
               let value(right: i32): i32\n\
             }\n",
    )
    .expect("distinct operation labels should form an overload set");
    let Item::Effect(ask) = &program.items[0] else {
        panic!("expected ask effect");
    };
    assert_eq!(ask.operations.len(), 2);

    let duplicate = parse(
        "let ask = effect {\n\
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
        "let state(comptime s: type) = effect { let get(): s }\n\
             let main(): i32 = {\n\
               state(i32).handle get { (resume) -> resume(42) } action {\n\
                 state(i32).get()\n\
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
            "let apply(comptime e: effects)(action: (i32): i32 with(e))(value: i32): i32 with(e) = { value }\n",
        )
        .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(matches!(
        &function.groups[0][0].ty,
        Type::Function { groups, effects, result }
            if groups == &vec![vec![Type::I32]]
                && effects.parameters == vec!["e"]
                && result.as_ref() == &Type::I32
    ));

    let old = parse(
        "let apply(comptime e: effects)(action: (i32): i32(e))(value: i32): i32 = { value }\n",
    )
    .unwrap_err();
    assert!(
        old.message
            .contains("effect parameter `e` cannot be used as a runtime type"),
        "{}",
        old.message
    );
    assert!(!old.message.contains("was removed"));
}

#[test]
fn rejects_parameter_modifier_parameters_on_data_declarations() {
    let error = parse(
        "let wrapper(comptime m: (comptime p: parameters): parameters) = struct { value: i32 }\n",
    )
    .unwrap_err();
    assert!(error
        .message
        .contains("modifier parameters belong to functions"));
}

#[test]
fn rejects_undeclared_access_parameters() {
    let error = parse("let invalid(value: borrow(a)(i32)): i32 = { value }\n").unwrap_err();
    assert!(error
        .message
        .contains("undeclared access or region parameter `a`"));
}

#[test]
fn rejects_the_legacy_while_condition_form() {
    let error = parse("let main(): () = { while ready() { work() } }\n").unwrap_err();
    assert!(error.message.contains(
        "`while` requires condition and `do` closures; write `while { condition } do { body }`"
    ));
}

#[test]
fn parses_while_with_positional_or_named_trailing_closures() {
    for source in [
        "let main(): () = { while { ready() } { work() } }\n",
        "let main(): () = {\n  while\n    condition { ready() }\n    do { work() }\n}\n",
    ] {
        let program = parse(source).unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        assert!(matches!(
            function_tail(function),
            Expr::While { condition, body, post_test: false }
                if matches!(condition.as_ref(), Expr::Block(_, _))
                    && matches!(body.as_ref(), Expr::Block(_, _))
        ));
    }
}

#[test]
fn parses_do_while_as_the_labeled_do_overload() {
    let program = parse("let main(): () = {\n  do { work() }\n  while { ready() }\n}\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert!(matches!(
        function_tail(function),
        Expr::While {
            condition,
            body,
            post_test: true,
        } if matches!(condition.as_ref(), Expr::Block(_, _))
            && matches!(body.as_ref(), Expr::Block(_, _))
    ));
}

#[test]
fn parses_if_with_positional_or_named_trailing_closures() {
    for source in [
        "let main(): i32 = { if true { 42 } { 0 } }\n",
        "let main(): i32 = { if true then { 42 } else { 0 } }\n",
        "let main(): i32 = { if false { 0 } else if true { 42 } else { 0 } }\n",
    ] {
        let program = parse(source).unwrap();
        let Item::Function(function) = &program.items[0] else {
            panic!("expected function");
        };
        let (_, then_branch, else_branch) = if_call_parts(function_tail(function));
        assert!(matches!(then_branch, Expr::Block(_, _)));
        let mut nested_groups = Vec::new();
        assert!(
            matches!(else_branch, Expr::Block(_, _))
                || flatten_test_call(else_branch, &mut nested_groups)
                    == &Parser::core_if_function()
        );
    }
}

#[test]
fn rejects_removed_while_let_syntax() {
    let error = parse("let main(): () = { while let some(value) = next() { consume(value) } }\n")
        .unwrap_err();
    assert!(error.message.contains("condition and `do` closures"));
}

#[test]
fn parses_loop_with_break_value() {
    let program = parse("let main(): i32 = { loop {\n  break(40 + 2)\n} }\n").unwrap();
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
                tail.unlocated(),
                Expr::Break(Some(value))
                    if matches!(value.as_ref(), Expr::Binary(_, BinaryOp::Add, _))
            )
    ));
}

#[test]
fn desugars_for_to_iteration_lang_item_calls() {
    let program = parse("let main(): () = { for values() { value -> consume(value) } }\n").unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Expr::Block(_, Some(for_loop)) = function.body.as_ref().unwrap() else {
        panic!("expected function block");
    };
    let Expr::Block(statements, None) = for_loop.unlocated() else {
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
fn rejects_bare_control_exits() {
    for (source, expected) in [
        (
            "let run(): () = { loop { break } }\n",
            "`break` requires one value",
        ),
        (
            "let run(): () = { loop { continue } }\n",
            "`(` after `continue`",
        ),
        (
            "let run(): () = { return }\n",
            "`return` requires one value",
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert!(error.message.contains(expected), "{}", error.message);
    }
}

#[test]
fn array_length_must_be_a_restricted_static_expression() {
    let error = parse("let main(values: array(i32)(borrow(value))): i32 = { 0 }\n").unwrap_err();
    assert!(
        error.message.contains("invalid compile-time array length"),
        "{}",
        error.message
    );
    assert!(
        error.message.contains("expected a pure expression"),
        "{}",
        error.message
    );
}

#[test]
fn array_type_preserves_curried_compile_parameter_groups() {
    let program = parse(
        "pub let array(comptime t: type)(comptime l: usize): type\n\
             let first(comptime l: usize)(values: array(i32)(l)): i32 = { values[0] }\n",
    )
    .unwrap();

    let Item::TypeForm(array) = &program.items[0] else {
        panic!("expected array type form");
    };
    assert_eq!(
        array.compile_groups,
        vec![
            vec![CompileParam {
                name: "t".to_owned(),
                kind: Sort::Type,
                default: None,
            }],
            vec![CompileParam {
                name: "l".to_owned(),
                kind: Sort::USize,
                default: None,
            }]
        ]
    );

    let error = parse("let values: array(i32, 2) = [1, 2]\n").unwrap_err();
    assert!(error.message.contains("second group"), "{}", error.message);
}

#[test]
fn parses_extend_methods_associated_functions_constants_and_trait_refs() {
    let program = parse(
        "let a = struct { value: i32 }\n\
             extend(a, foo) {\n\
               let reset(self: borrow(mut)(self))(): () = {}\n\
               let answer: i32 = 42\n\
               let make(value: i32): a = { a { value: value } }\n\
             }\n",
    )
    .unwrap();

    let Item::Extend(extension) = &program.items[1] else {
        panic!("expected extend declaration");
    };
    assert_eq!(extension.target, Type::Named("a".into(), Vec::new()));
    assert_eq!(
        extension.trait_ref,
        Some(Type::Named("foo".into(), Vec::new()))
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
            pointee: Box::new(Type::Named("self".into(), Vec::new())),
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
        "extend(a) {\n\
               let convert(comptime t: type)(self: borrow(self))(value: t): t = { value }\n\
               let make(comptime t: type)(value: t): t = { value }\n\
             }\n",
    )
    .unwrap();

    let Item::Extend(extension) = &program.items[0] else {
        panic!("expected extend declaration");
    };
    let ExtendMember::Function(convert) = &extension.members[0] else {
        panic!("expected generic method");
    };
    assert_eq!(convert.compile_groups[0][0].name, "t");
    assert_eq!(convert.groups.len(), 2);
    assert_eq!(convert.groups[0][0].name, "self");
    assert_eq!(convert.groups[1][0].ty, Type::Named("t".into(), Vec::new()));

    let ExtendMember::Function(make) = &extension.members[1] else {
        panic!("expected generic associated function");
    };
    assert_eq!(make.compile_groups.len(), 1);
    assert_eq!(make.groups.len(), 1);
}

#[test]
fn infers_extend_pattern_parameters_from_constructor_sorts() {
    let program = parse(
        "let cell(comptime t: type) = struct { value: t }\n\
             extend(cell(t))\n\
             (requires: t is copyable) {\n\
               let get(self: borrow(self))(): t = { self.value }\n}\n",
    )
    .unwrap();

    let Item::Extend(extension) = &program.items[1] else {
        panic!("expected extend declaration");
    };
    assert_eq!(extension.compile_groups.len(), 1);
    assert_eq!(extension.where_predicates.len(), 1);
    assert_eq!(extension.compile_groups[0][0].name, "t");
    assert_eq!(
        extension.target,
        Type::Named("cell".into(), vec![Type::Named("t".into(), Vec::new())])
    );

    let program = parse(
        "let result(comptime error: type)(comptime t: type) = enum { ok(t), err(error) }\n\
             let chain = trait {}\n\
             extend(result(error)(t), chain) {}\n",
    )
    .unwrap();
    let Item::Extend(extension) = &program.items[2] else {
        panic!("expected destructuring extend declaration");
    };
    assert_eq!(
        extension.compile_groups[0]
            .iter()
            .map(|parameter| (parameter.name.as_str(), parameter.kind.clone()))
            .collect::<Vec<_>>(),
        vec![("error", Sort::Type), ("t", Sort::Type)]
    );
    assert_eq!(
        extension.trait_ref,
        Some(Type::Named("chain".into(), Vec::new()))
    );
}

#[test]
fn qualified_extend_roots_are_not_inferred_as_parameters() {
    let program = parse(
        "let functor = trait(comptime self: (comptime value: type): type) {}\n\
             extend(core.option.option, functor) {}\n\
             extend(core.result.result(error), functor) {}\n",
    )
    .unwrap();
    let Item::Extend(option) = &program.items[1] else {
        panic!("expected qualified option extension");
    };
    assert!(option.compile_groups.is_empty());
    let Item::Extend(result) = &program.items[2] else {
        panic!("expected qualified result extension");
    };
    assert_eq!(result.compile_groups[0][0].name, "error");
    assert_eq!(result.compile_groups[0][0].kind, Sort::Type);
}

#[test]
fn parses_multiline_constraint_guards_without_inference_placeholders() {
    let program = parse(
        "let choose(comptime t: type)(copy value: t): t\n\
             = requires(t is copyable && t is marker(i32) && t.item == t) { value }\n",
    )
    .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected a generic function");
    };
    assert_eq!(function.where_predicates.len(), 2);
    assert_eq!(
        function.where_predicates[0].subject,
        Type::Named("t".into(), Vec::new())
    );
    assert_eq!(
        function.where_predicates[1].trait_ref,
        Type::Named("marker".into(), vec![Type::I32])
    );
    assert_eq!(function.where_predicates[1].associated_types.len(), 1);
    assert_eq!(
        function.where_predicates[1].associated_types[0].name,
        "item"
    );
    assert_eq!(
        function.where_predicates[1].associated_types[0].ty,
        Type::Named("t".into(), Vec::new())
    );
}

#[test]
fn lowers_compile_time_constraint_guards_to_trait_predicates() {
    let program = parse(
        "let copyable = trait {}\n\
         let cell(comptime t: type) = struct { value: t }\n\
         let duplicate(comptime t: type)(value: t): (t, t) = requires(t is copyable) {\n\
           (value, value)\n\
         }\n\
         extend(cell(t), copyable)\n\
         (requires: t is copyable) {}\n",
    )
    .unwrap();

    let Item::Function(function) = &program.items[2] else {
        panic!("expected guarded function");
    };
    assert_eq!(function.where_predicates.len(), 1);
    assert_eq!(
        function.where_predicates[0].subject,
        Type::Named("t".into(), Vec::new())
    );
    assert_eq!(
        function.where_predicates[0].trait_ref,
        Type::Named("copyable".into(), Vec::new())
    );

    let Item::Extend(extension) = &program.items[3] else {
        panic!("expected guarded extension");
    };
    assert_eq!(extension.where_predicates, function.where_predicates);
}

#[test]
fn constraint_guards_require_is_evidence_before_projection_equalities() {
    let error =
        parse("let read(comptime t: type)(value: t): t = requires(t.item == i32) { value }\n")
            .expect_err("a projection without trait evidence must fail");
    assert!(error
        .message
        .contains("must follow an `is` constraint for the same subject"));

    let error = parse("let read(comptime t: type)(value: t): t where t: copyable = { value }\n")
        .expect_err("colon-style predicates must fail");
    assert!(error
        .message
        .contains("colon-style `where` predicates were removed"));
}

#[test]
fn parses_generic_associated_type_equalities() {
    let program = parse(
            "let lend(comptime t: type)(value: t): t\n\
             = requires(t is lender && t.item(comptime a: access)(comptime r: region) == borrow(a)(r)(i32)) { value }\n",
        )
        .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected a generic function");
    };
    let binding = &function.where_predicates[0].associated_types[0];
    assert_eq!(binding.name, "item");
    assert_eq!(binding.compile_groups.len(), 2);
    assert!(binding.compile_groups[0][0].kind.is_access());
    assert_eq!(binding.compile_groups[1][0].kind, Sort::Region);
    assert_eq!(
        binding.ty,
        Type::Borrow {
            mutable: false,
            access: Some("a".into()),
            region: Some("r".into()),
            pointee: Box::new(Type::I32),
        }
    );
}

#[test]
fn rejects_invalid_extend_receivers() {
    let cases = [
        (
            "extend(a) { let invalid(self, value: i32)(): () = {} }\n",
            "only parameter",
        ),
        (
            "extend(a) { let invalid(self): () = {} }\n",
            "requires an explicit parameter group",
        ),
        (
            "extend(a) { let invalid(value: i32)(self)(): () = {} }\n",
            "first parameter group",
        ),
        (
            "extend(a) { let invalid(self)(self)(): () = {} }\n",
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
        "extend(a) {\n\
               let inspect(self: borrow(self))(): i32 = { self.value }\n\
               let replace(move self)(value: i32)(other: i32): a = { a { value: value + other } }\n\
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
            pointee: Box::new(Type::Named("self".into(), Vec::new())),
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
    let receiver = parse("let invalid(self: a)(): () = {}\n").unwrap_err();
    assert!(receiver.message.contains("only allowed in extend"));

    let mutable = parse("extend(a) { let mut answer = 42 }\n").unwrap_err();
    assert!(mutable.message.contains("let mut"));

    let data = parse("extend(a) { let nested = struct { value: i32 } }\n").unwrap_err();
    assert!(data.message.contains("data declarations"));

    let missing = parse("extend(a) { let answer: i32\n}\n").unwrap_err();
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
        "let family(comptime t: type): type = box(t)\n\
             let constructor: (comptime element: type): type = box\n\
             let scalar: type = i32\n",
    )
    .unwrap();

    let Item::TypeAlias(family) = &program.items[0] else {
        panic!("expected type-family alias");
    };
    assert_eq!(family.compile_groups[0][0].name, "t");
    assert_eq!(
        family.target,
        Type::Named("box".into(), vec![Type::Named("t".into(), Vec::new())])
    );

    let Item::TypeAlias(constructor) = &program.items[1] else {
        panic!("expected type-constructor alias");
    };
    assert_eq!(constructor.compile_groups[0][0].name, "element");
    assert_eq!(
        constructor.target,
        Type::Named(
            "box".into(),
            vec![Type::Named("element".into(), Vec::new())]
        )
    );

    assert!(matches!(
        &program.items[2],
        Item::TypeAlias(alias) if alias.compile_groups.is_empty() && alias.target == Type::I32
    ));
}

#[test]
fn parses_constructor_compile_parameter_sorts() {
    let program = parse(
            "let use(comptime f: (comptime element: type): type)(move value: f(i32)): f(i32) = { value }\n\
             let curried(comptime f: (comptime element: type)(comptime length: usize): type)(): i32 = { 0 }\n\
             let effects(comptime e: (comptime error: type): effect)(move action: (): i32 with(e(bool))): i32 with(e(bool)) = { action() }\n\
             let functor = trait(comptime self: (comptime value: type): type) {\n\
               let map(comptime e: effects, comptime a: type, comptime b: type)(move self: self(a))(move transform: (a): b with(e)): self(b) with(e)\n\
             }\n\
             let applicative = trait(comptime self: (comptime value: type): type)(requires: self is functor) {\n\
               let pure(comptime a: type)(move value: a): self(a)\n}\n",
        )
        .unwrap();

    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        function.compile_groups[0][0].kind,
        Sort::TypeConstructor {
            parameter_groups: vec![vec![Sort::Type]],
        }
    );
    assert_eq!(
        function.groups[0][0].ty,
        Type::Named("f".into(), vec![Type::I32])
    );

    let Item::Function(curried) = &program.items[1] else {
        panic!("expected curried constructor function");
    };
    assert_eq!(
        curried.compile_groups[0][0].kind,
        Sort::TypeConstructor {
            parameter_groups: vec![vec![Sort::Type], vec![Sort::USize]],
        }
    );

    let Item::Function(effects) = &program.items[2] else {
        panic!("expected effect-constructor function");
    };
    assert_eq!(
        effects.compile_groups[0][0].kind,
        Sort::EffectConstructor {
            parameter_groups: vec![vec![Sort::Type]],
        }
    );
    assert_eq!(
        effects.effects.custom,
        vec![Type::Named("e".into(), vec![Type::Bool])]
    );

    let Item::Trait(trait_def) = &program.items[3] else {
        panic!("expected trait");
    };
    assert_eq!(
        trait_def.self_parameter.kind,
        Sort::TypeConstructor {
            parameter_groups: vec![vec![Sort::Type]],
        }
    );
    assert!(trait_def.compile_groups.is_empty());

    let Item::Trait(applicative) = &program.items[4] else {
        panic!("expected inherited trait");
    };
    assert_eq!(applicative.where_predicates.len(), 1);
    assert_eq!(
        applicative.where_predicates[0].subject,
        Type::Named("self".into(), Vec::new())
    );
    assert_eq!(
        applicative.where_predicates[0].trait_ref,
        Type::Named("functor".into(), Vec::new())
    );
}

#[test]
fn parses_labeled_type_arguments_without_reordering() {
    let program =
        parse("let consume(value: pair(v: bool, k: i32)): result(e: bool)(t: i32) = { value }\n")
            .unwrap();
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    assert_eq!(
        function.groups[0][0].ty,
        Type::NamedArgs(
            "pair".into(),
            vec![
                TypeArg {
                    label: Some("v".into()),
                    ty: Type::Bool,
                },
                TypeArg {
                    label: Some("k".into()),
                    ty: Type::I32,
                },
            ],
        )
    );
    assert_eq!(
        function.return_type,
        Some(Type::NamedArgs(
            "result".into(),
            vec![
                TypeArg {
                    label: Some("e".into()),
                    ty: Type::Bool,
                },
                TypeArg {
                    label: Some("t".into()),
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

#[test]
fn records_local_initializer_and_statement_ranges() {
    let program = parse(
        "let main(): i32 = {\n  let value: i32 = true\n  value\n}\n\
             let choose(): i32 = { if true { 1 } else { 2 } }\n",
    )
    .expect("parse source ranges");
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Some(Expr::Block(statements, Some(tail))) = &function.body else {
        panic!("expected function block");
    };
    let Stmt::Let(binding) = &statements[0] else {
        panic!("expected local binding");
    };
    assert_eq!(
        binding.value_source.as_deref(),
        Some(&crate::ast::SourceSpan {
            line: 2,
            column: 20,
            end_line: 2,
            end_column: 24,
        })
    );
    assert!(matches!(
        tail.as_ref(),
        Expr::Located {
            line: 3,
            column: 3,
            end_line: 3,
            end_column: 8,
            ..
        }
    ));
    let Item::Function(function) = &program.items[1] else {
        panic!("expected trailing-closure function");
    };
    let Some(Expr::Block(_, Some(tail))) = &function.body else {
        panic!("expected trailing-closure block");
    };
    assert!(matches!(
        tail.as_ref(),
        Expr::Located { value, .. }
            if matches!(value.as_ref(), Expr::Call(_, arguments)
                if arguments.iter().any(|argument| {
                    matches!(argument.value, Expr::Closure(_, _))
                }))
    ));
}

#[test]
fn parses_contextual_async_and_await_as_language_expressions() {
    let program = parse("let make(): i32 = {\n  let future = async { await next() }\n  0\n}\n")
        .expect("async expressions must parse");
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Some(Expr::Block(statements, _)) = &function.body else {
        panic!("expected function block");
    };
    let Stmt::Let(binding) = &statements[0] else {
        panic!("expected future binding");
    };
    let Expr::Async { body } = binding.value.unlocated() else {
        panic!("expected async expression");
    };
    let Expr::Block(_, Some(tail)) = body.as_ref() else {
        panic!("expected async body");
    };
    assert!(matches!(tail.unlocated(), Expr::Await(_)));

    let program = parse(
            "let make(): i32 = {\n  let future = async {\n    while { true } {\n      let value = await next();\n      break()\n    }\n  }\n  0\n}\n",
        )
        .expect("control-flow blocks must preserve their enclosing async context");
    let Item::Function(function) = &program.items[0] else {
        panic!("expected function");
    };
    let Some(Expr::Block(statements, _)) = &function.body else {
        panic!("expected function block");
    };
    let Stmt::Let(binding) = &statements[0] else {
        panic!("expected future binding");
    };
    let Expr::Async { body } = binding.value.unlocated() else {
        panic!("expected async expression");
    };
    let Expr::Block(_, Some(tail)) = body.as_ref() else {
        panic!("expected async body");
    };
    let Expr::While { body, .. } = tail.unlocated() else {
        panic!("expected while expression");
    };
    let Expr::Block(statements, _) = body.as_ref() else {
        panic!("expected while body");
    };
    let Stmt::Let(binding) = &statements[0] else {
        panic!("expected awaited binding");
    };
    assert!(matches!(binding.value.unlocated(), Expr::Await(_)));
}

#[test]
fn async_and_await_remain_contextual_identifiers() {
    parse(
        "let async: i32 = 1\n\
             let await(value: i32): i32 = { value }\n\
             let main(): i32 = { await(async) }\n",
    )
    .expect("contextual async spellings must remain ordinary identifiers");

    parse("let main(): i32 = { async { { await value } } }\n")
        .expect("await accepts a parenthesis-free operand");
}
