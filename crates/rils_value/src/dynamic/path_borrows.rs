//! Lexical native references retain checked paths, never pointers into bytes.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::{DynamicPathStep, DynamicValue, SequenceItemLease};

#[derive(Default)]
pub(super) struct PathBorrowLedger {
    entries: RefCell<Vec<PathBorrow>>,
}

struct PathBorrow {
    path: Vec<DynamicPathStep>,
    count: Rc<Cell<usize>>,
}

/// Keeps every sequence traversed by a path stable until the reference drops.
/// Several mutable leases to the same path are intentionally allowed.
pub struct DynamicPathLease {
    count: Rc<Cell<usize>>,
    _sequences: Vec<SequenceItemLease>,
}

impl Drop for DynamicPathLease {
    fn drop(&mut self) {
        self.count.set(self.count.get() - 1);
    }
}

impl DynamicValue {
    pub fn reference_path(&self, path: &[DynamicPathStep]) -> Result<DynamicPathLease, String> {
        self.view_path(path)?;
        let mut sequences = Vec::new();
        for (position, step) in path.iter().enumerate() {
            if let DynamicPathStep::Index(index) = step {
                let ledger = self.view_path(&path[..position])?.sequence_borrows()?;
                sequences.push(ledger.reference(*index)?);
            }
        }
        let ledger = self
            .path_borrows
            .get_or_init(|| Rc::new(PathBorrowLedger::default()));
        let mut entries = ledger.entries.borrow_mut();
        entries.retain(|entry| entry.count.get() > 0);
        let count = if let Some(entry) = entries.iter().find(|entry| entry.path == path) {
            entry.count.clone()
        } else {
            let count = Rc::new(Cell::new(0usize));
            entries.push(PathBorrow {
                path: path.to_vec(),
                count: count.clone(),
            });
            count
        };
        count.set(
            count
                .get()
                .checked_add(1)
                .ok_or("too many native path references")?,
        );
        Ok(DynamicPathLease {
            count,
            _sequences: sequences,
        })
    }

    pub fn has_path_references(&self) -> bool {
        self.path_borrows.get().is_some_and(|ledger| {
            ledger
                .entries
                .borrow()
                .iter()
                .any(|entry| entry.count.get() > 0)
        })
    }

    pub(super) fn check_path_move(&self, path: &[DynamicPathStep]) -> Result<(), String> {
        if self.path_borrows.get().is_some_and(|ledger| {
            ledger.entries.borrow().iter().any(|entry| {
                entry.count.get() > 0
                    && (entry.path.starts_with(path) || path.starts_with(&entry.path))
            })
        }) {
            return Err("cannot move a native field while it is referenced".into());
        }
        Ok(())
    }

    pub(super) fn check_path_write(
        &self,
        path: &[DynamicPathStep],
        through_reference: bool,
    ) -> Result<(), String> {
        if self.path_borrows.get().is_some_and(|ledger| {
            ledger.entries.borrow().iter().any(|entry| {
                entry.count.get() > 0
                    && entry.path.starts_with(path)
                    && (!through_reference || entry.path.len() > path.len())
            })
        }) {
            return Err("cannot replace a native field while it is referenced".into());
        }
        Ok(())
    }
}
