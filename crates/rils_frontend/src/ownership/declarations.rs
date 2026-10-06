use super::*;

impl Checker<'_> {
    pub(super) fn collect_nominals(&mut self, statements: &[Stmt]) {
        for statement in statements {
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect_nominals(statements),
                Stmt::Impl {
                    target,
                    trait_name,
                    methods,
                    ..
                } => {
                    let Type::Named { name, .. } = target else {
                        continue;
                    };
                    if trait_name.as_deref() == Some("Iterator") {
                        for member in rils_builtins::builtin("Iterator")
                            .into_iter()
                            .flat_map(|declaration| declaration.members)
                        {
                            if !rils_builtins::is_iterator_default_method(member.name) {
                                continue;
                            }
                            let Some(receiver) = member.receiver else {
                                continue;
                            };
                            let mode = match receiver {
                                rils_builtins::ReceiverMode::Owned => ReceiverMode::Owned,
                                rils_builtins::ReceiverMode::Shared => {
                                    ReceiverMode::Borrowed { mutable: false }
                                }
                                rils_builtins::ReceiverMode::Mutable => {
                                    ReceiverMode::Borrowed { mutable: true }
                                }
                            };
                            self.receivers
                                .insert((name.clone(), member.name.into()), mode);
                        }
                    }
                    for method in methods {
                        let Some(receiver) = method
                            .parameters
                            .first()
                            .filter(|parameter| parameter.name == "self")
                        else {
                            continue;
                        };
                        let mode = match &receiver.type_annotation {
                            Some(Type::Reference { mutable, .. }) => {
                                ReceiverMode::Borrowed { mutable: *mutable }
                            }
                            _ => ReceiverMode::Owned,
                        };
                        self.receivers
                            .insert((name.clone(), method.name.clone()), mode);
                    }
                }
                _ => {}
            }
        }
    }
}
