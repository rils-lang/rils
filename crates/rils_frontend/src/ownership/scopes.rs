use super::*;

impl Checker<'_> {
    pub(super) fn push_scope(&mut self) {
        let parent = self.scopes.last().expect("scope exists").region;
        self.scopes.push(Scope {
            region: self.regions.child(parent),
            bindings: HashMap::new(),
            retained_borrows: Vec::new(),
        });
    }

    pub(super) fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            for borrow in scope.retained_borrows {
                self.remove_borrow(&borrow);
            }
        }
    }

    pub(super) fn retain(&mut self, borrows: Vec<Borrow>) {
        self.scopes
            .last_mut()
            .expect("scope exists")
            .retained_borrows
            .extend(borrows);
    }

    pub(super) fn discard(&mut self, value: ExpressionValue) {
        for borrow in value.borrows {
            self.remove_borrow(&borrow);
        }
    }

    pub(super) fn add_borrow(&mut self, borrow: &Borrow) {
        let counts = self.active_borrows.entry(borrow.root.clone()).or_default();
        if borrow.interior {
            counts.1 += 1;
        } else {
            counts.0 += 1;
        }
    }

    pub(super) fn remove_borrow(&mut self, borrow: &Borrow) {
        let Some(counts) = self.active_borrows.get_mut(&borrow.root) else {
            return;
        };
        if borrow.interior {
            counts.1 = counts.1.saturating_sub(1);
        } else {
            counts.0 = counts.0.saturating_sub(1);
        }
        if *counts == (0, 0) {
            self.active_borrows.remove(&borrow.root);
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name))
    }

    pub(super) fn lookup_mut(&mut self, name: &str) -> Option<&mut Binding> {
        self.scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.bindings.get_mut(name))
    }

    pub(super) fn binding_scope(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rposition(|scope| scope.bindings.contains_key(name))
    }

    pub(super) fn diagnostic(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(AnalysisDiagnostic::error(message, span));
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        (self.scopes.clone(), self.active_borrows.clone())
    }

    pub(super) fn restore(&mut self, snapshot: Snapshot) {
        self.scopes = snapshot.0;
        self.active_borrows = snapshot.1;
    }

    pub(super) fn merge_moved(&mut self, states: &[Snapshot]) {
        if states.is_empty() {
            return;
        }
        for scope_index in 0..self.scopes.len() {
            let names = self.scopes[scope_index]
                .bindings
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            for name in names {
                let moved = states.iter().all(|(scopes, _)| {
                    scopes
                        .get(scope_index)
                        .and_then(|scope| scope.bindings.get(&name))
                        .is_some_and(|binding| binding.moved)
                });
                if moved && let Some(binding) = self.scopes[scope_index].bindings.get_mut(&name) {
                    binding.moved = true;
                }
                let moved_places = states
                    .iter()
                    .filter_map(|(scopes, _)| {
                        scopes
                            .get(scope_index)
                            .and_then(|scope| scope.bindings.get(&name))
                            .map(|binding| binding.moved_places.clone())
                    })
                    .reduce(|left, right| left.intersection(&right).cloned().collect())
                    .unwrap_or_default();
                if let Some(binding) = self.scopes[scope_index].bindings.get_mut(&name) {
                    binding.moved_places.extend(moved_places);
                }
            }
        }
    }

    pub(super) fn merge_active_borrows(&mut self, states: &[Snapshot]) {
        let mut merged = HashMap::new();
        for (_, active) in states {
            for (root, counts) in active {
                let entry = merged.entry(root.clone()).or_insert((0, 0));
                entry.0 = entry.0.max(counts.0);
                entry.1 = entry.1.max(counts.1);
            }
        }
        self.active_borrows = merged;
    }
}
