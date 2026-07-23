use crate::ast::CallArg;

use super::Analyzer;

impl Analyzer {
    pub(super) fn resolve_function_overload(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
    ) -> Option<String> {
        let candidates = self.function_overloads.get(name)?.clone();
        if !groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| argument.label.is_some())
        {
            self.error(format!(
                "overloaded call `{name}` requires named arguments to select an overload"
            ));
            return None;
        }
        let matches = self.matching_function_overloads(&candidates, groups, 0);
        match matches.as_slice() {
            [selected] => Some(selected.clone()),
            [] => {
                let supplied = groups
                    .iter()
                    .map(|group| {
                        format!(
                            "({})",
                            group
                                .iter()
                                .filter_map(|argument| argument.label.as_deref())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("");
                self.error(format!(
                    "no overload of `{name}` matches named parameter groups {supplied}"
                ));
                None
            }
            _ => {
                self.error(format!(
                    "overloaded call `{name}` remains ambiguous; supply a parameter group whose names distinguish one overload"
                ));
                None
            }
        }
    }

    pub(super) fn resolve_inherent_overload(
        &mut self,
        target: &str,
        member: &str,
        is_method: bool,
        groups: &[&[CallArg]],
    ) -> Option<String> {
        let key = (target.to_owned(), member.to_owned(), is_method);
        let candidates = self.inherent_overloads.get(&key)?.clone();
        if !groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| argument.label.is_some())
        {
            self.error(format!(
                "overloaded call `{target}.{member}` requires named arguments to select an overload"
            ));
            return None;
        }
        let matches = self.matching_function_overloads(&candidates, groups, usize::from(is_method));
        match matches.as_slice() {
            [selected] => Some(selected.clone()),
            [] => {
                self.error(format!(
                    "no overload of `{target}.{member}` matches the supplied named parameter groups"
                ));
                None
            }
            _ => {
                self.error(format!(
                    "overloaded call `{target}.{member}` remains ambiguous; name a parameter from a distinguishing group"
                ));
                None
            }
        }
    }

    pub(super) fn matching_function_overloads(
        &self,
        candidates: &[String],
        groups: &[&[CallArg]],
        parameter_group_offset: usize,
    ) -> Vec<String> {
        candidates
            .iter()
            .filter(|candidate| {
                let parameter_names = if let Some(signature) = self.signatures.get(*candidate) {
                    signature.groups[parameter_group_offset..]
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                } else if let Some(template) = self.function_templates.get(*candidate) {
                    template.groups[parameter_group_offset..]
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                } else {
                    return false;
                };
                let matches_runtime = |runtime_groups: &[&[CallArg]]| {
                    if runtime_groups.len() > parameter_names.len() {
                        return false;
                    }
                    runtime_groups
                        .iter()
                        .zip(&parameter_names)
                        .all(|(arguments, parameters)| {
                            if arguments.len() != parameters.len() {
                                return false;
                            }
                            let labeled = arguments
                                .iter()
                                .filter(|argument| argument.label.is_some())
                                .count();
                            labeled == 0
                                || labeled == arguments.len()
                                    && parameters.iter().all(|parameter| {
                                        arguments
                                            .iter()
                                            .filter(|argument| {
                                                argument.label.as_deref()
                                                    == Some(parameter.as_str())
                                            })
                                            .count()
                                            == 1
                                    })
                        })
                };
                if self.signatures.contains_key(*candidate) {
                    matches_runtime(groups)
                } else {
                    let compile_group_count =
                        self.function_templates[*candidate].compile_groups.len();
                    (0..=compile_group_count.min(groups.len())).any(|runtime_start| {
                        groups[runtime_start..]
                            .iter()
                            .flat_map(|group| group.iter())
                            .any(|argument| argument.label.is_some())
                            && matches_runtime(&groups[runtime_start..])
                    })
                }
            })
            .cloned()
            .collect()
    }

    pub(super) fn ordered_call_arguments<'a>(
        &mut self,
        owner: &str,
        group_number: usize,
        arguments: &'a [CallArg],
        parameter_names: &[String],
    ) -> Option<Vec<&'a CallArg>> {
        if arguments.len() != parameter_names.len() {
            self.error(format!(
                "argument count mismatch in group {group_number} of `{owner}`: expected {}, found {}",
                parameter_names.len(),
                arguments.len()
            ));
            return None;
        }
        if arguments.iter().all(|argument| argument.label.is_none()) {
            return Some(arguments.iter().collect());
        }
        if arguments.iter().any(|argument| argument.label.is_none()) {
            self.error(format!(
                "cannot mix named and positional arguments in group {group_number} of `{owner}`"
            ));
            return None;
        }

        let mut ordered = vec![None; parameter_names.len()];
        for (source_index, argument) in arguments.iter().enumerate() {
            let label = argument.label.as_deref().expect("all arguments are named");
            let Some(index) = parameter_names.iter().position(|name| name == label) else {
                self.error(format!(
                    "unknown parameter `{label}` in group {group_number} of `{owner}`"
                ));
                return None;
            };
            if index != source_index {
                self.error(format!(
                    "named arguments in group {group_number} of `{owner}` must follow parameter declaration order; expected `{}` before `{label}`",
                    parameter_names[source_index]
                ));
                return None;
            }
            if ordered[index].replace(argument).is_some() {
                self.error(format!(
                    "duplicate argument for parameter `{label}` in group {group_number} of `{owner}`"
                ));
                return None;
            }
        }
        for (index, argument) in ordered.iter().enumerate() {
            if argument.is_none() {
                self.error(format!(
                    "missing argument for parameter `{}` in group {group_number} of `{owner}`",
                    parameter_names[index]
                ));
                return None;
            }
        }
        Some(ordered.into_iter().flatten().collect())
    }
}
