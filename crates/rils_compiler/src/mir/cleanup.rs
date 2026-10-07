//! Release lexical values on normal exits and loop control-flow edges.

use super::*;

#[derive(Clone, Copy)]
pub(super) enum ScopeCleanup {
    Local(LocalId),
    Register(Register),
}

impl Builder {
    pub(super) fn clean_scope_value(&mut self, cleanup: ScopeCleanup, span: Span) {
        match cleanup {
            ScopeCleanup::Local(local) => self.emit(MirInstruction::DropLocal { local }, span),
            ScopeCleanup::Register(destination) => {
                let source = self.unit(span);
                self.emit(
                    MirInstruction::Move {
                        destination,
                        source,
                    },
                    span,
                );
            }
        }
    }

    pub(super) fn clean_exiting_scopes(&mut self, depth: usize, span: Span) {
        let cleanups = self.scope_cleanups[depth..]
            .iter()
            .rev()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        for cleanup in cleanups {
            self.clean_scope_value(cleanup, span);
        }
    }
}
