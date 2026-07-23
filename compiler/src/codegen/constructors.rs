use super::*;

impl Analyzer {
    pub(super) fn lower_struct_literal(
        &mut self,
        constructor: &Expr,
        fields: &[CallArg],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if fields.iter().any(|field| field.label.is_none()) {
            self.error("struct literal fields must be named; use `field: value` inside `{ ... }`");
            return error_expr();
        }
        let mut groups = Vec::new();
        let root = flatten_call(constructor, &mut groups);
        let Expr::Name(name) = root else {
            self.error("struct literal requires a struct type name");
            return error_expr();
        };
        if context.lookup(name).is_some() {
            self.error(format!(
                "local value `{name}` cannot be used as a struct literal constructor"
            ));
            return error_expr();
        }
        if context.has_type_parameter(name) {
            self.error(format!(
                "type parameter `{name}` cannot be used as a struct literal constructor"
            ));
            return error_expr();
        }
        if name == "Self" && !context.type_substitutions.contains_key("Self") {
            self.error("expression `Self` is only available inside an extend member");
            return error_expr();
        }
        if groups.is_empty() && self.struct_layouts.contains_key(name) {
            return self.lower_struct_constructor(name, &[fields], context);
        }
        if self.struct_templates.contains_key(name) {
            let mut construction_groups = groups;
            construction_groups.push(fields);
            let Some((canonical, runtime_start)) = self.resolve_inferred_generic_struct_instance(
                name,
                &construction_groups,
                expected,
                context,
            ) else {
                return error_expr();
            };
            return self.lower_struct_constructor(
                &canonical,
                &construction_groups[runtime_start..],
                context,
            );
        }
        if self.enum_layouts.contains_key(name) || self.enum_templates.contains_key(name) {
            self.error(format!(
                "struct literal `{name} {{ ... }}` requires a struct type, found enum `{name}`"
            ));
            return error_expr();
        }
        if self.struct_layouts.contains_key(name) {
            self.error(format!(
                "struct `{name}` does not accept type argument groups in a struct literal"
            ));
            return error_expr();
        }
        self.error(format!("unknown struct `{name}`"));
        error_expr()
    }

    pub(super) fn lower_struct_constructor(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 {
            self.error(format!(
                "struct constructor `{name}` expects exactly one argument group"
            ));
            return error_expr();
        }
        let Some(layout) = self.struct_layout_or_diagnostic(name) else {
            return error_expr();
        };
        let mut accessible = true;
        for field in &layout.fields {
            accessible &= self.require_field_access(name, field, &context.origin);
        }
        if !accessible {
            return error_expr();
        }
        let fields = self.lower_constructor_fields(
            groups[0],
            &layout.fields,
            true,
            &format!("struct `{name}`"),
            context,
        );
        HirExpr {
            ty: Ty::Struct(name.to_owned()),
            kind: HirExprKind::ConstructStruct {
                name: name.to_owned(),
                fields,
            },
        }
    }

    pub(super) fn lower_enum_constructor(
        &mut self,
        enum_name: &str,
        variant: usize,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 {
            self.error(format!(
                "enum variant constructor `{enum_name}` expects exactly one argument group"
            ));
            return error_expr();
        }
        let Some(layout) = self.enum_layout_or_diagnostic(enum_name) else {
            return error_expr();
        };
        let variant_layout = &layout.variants[variant];
        if variant_layout.fields.is_empty() {
            self.error(format!(
                "unit variant `{enum_name}.{}` is a value and must not be called",
                variant_layout.name
            ));
            return error_expr();
        }
        let owner = format!("{enum_name}.{}", variant_layout.name);
        let mut accessible = true;
        for field in &variant_layout.fields {
            accessible &= self.require_field_access(&owner, field, &context.origin);
        }
        if !accessible {
            return error_expr();
        }
        let fields = self.lower_constructor_fields(
            groups[0],
            &variant_layout.fields,
            variant_layout.named,
            &format!("variant `{enum_name}.{}`", variant_layout.name),
            context,
        );
        HirExpr {
            ty: Ty::Enum(enum_name.to_owned()),
            kind: HirExprKind::ConstructEnum {
                name: enum_name.to_owned(),
                variant,
                fields,
            },
        }
    }

    pub(super) fn lower_constructor_fields(
        &mut self,
        arguments: &[CallArg],
        fields: &[FieldLayout],
        labels_allowed: bool,
        constructor: &str,
        context: &mut LowerCtx,
    ) -> Vec<(usize, HirExpr)> {
        let labeled = arguments
            .iter()
            .filter(|argument| argument.label.is_some())
            .count();
        if labeled != 0 && labeled != arguments.len() {
            self.error(format!(
                "cannot mix labeled and positional arguments in {constructor}"
            ));
            return Vec::new();
        }

        if labeled == 0 {
            if arguments.len() != fields.len() {
                self.error(format!(
                    "argument count mismatch for {constructor}: expected {}, found {}",
                    fields.len(),
                    arguments.len()
                ));
            }
            return arguments
                .iter()
                .zip(fields)
                .enumerate()
                .map(|(index, (argument, field))| {
                    (
                        index,
                        self.lower_expr(&argument.value, Some(&field.ty), context),
                    )
                })
                .collect();
        }

        if !labels_allowed {
            self.error(format!("{constructor} does not accept labeled arguments"));
            return Vec::new();
        }
        let mut initialized = HashSet::new();
        let mut lowered = Vec::new();
        for argument in arguments {
            let label = argument
                .label
                .as_deref()
                .expect("all arguments are labeled");
            let Some((index, field)) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == label)
            else {
                self.error(format!("unknown field `{label}` in {constructor}"));
                continue;
            };
            if !initialized.insert(index) {
                self.error(format!("duplicate field `{label}` in {constructor}"));
                continue;
            }
            lowered.push((
                index,
                self.lower_expr(&argument.value, Some(&field.ty), context),
            ));
        }
        for (index, field) in fields.iter().enumerate() {
            if !initialized.contains(&index) {
                self.error(format!("missing field `{}` in {constructor}", field.name));
            }
        }
        lowered
    }

    pub(super) fn resolve_short_variant(
        &mut self,
        name: &str,
        expected: Option<&Ty>,
        origin: &ItemOrigin,
    ) -> Option<(String, usize)> {
        if let Some(Ty::Enum(enum_name)) = expected {
            let layout = self.enum_layout_or_diagnostic(enum_name)?;
            let enum_is_accessible = self
                .nominal_accesses
                .get(enum_name)
                .is_some_and(|access| Self::access_boundary_allows(origin, access));
            if enum_is_accessible {
                if let Some(index) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                {
                    return Some((enum_name.clone(), index));
                }
            }
        }
        let candidates: Vec<_> = self
            .enum_layouts
            .iter()
            .filter_map(|(enum_name, layout)| {
                let is_non_generic = self
                    .nominal_instances
                    .get(enum_name)
                    .is_some_and(|instance| instance.key.arguments.is_empty());
                if !is_non_generic {
                    return None;
                }
                if !self
                    .nominal_accesses
                    .get(enum_name)
                    .is_some_and(|access| Self::access_boundary_allows(origin, access))
                {
                    return None;
                }
                layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                    .map(|variant| (enum_name.clone(), variant))
            })
            .collect();
        match candidates.as_slice() {
            [candidate] => Some(candidate.clone()),
            [] => None,
            _ => {
                self.error(format!(
                    "variant name `{name}` is ambiguous; qualify it with its enum"
                ));
                None
            }
        }
    }
}
