//! Lexical regions owned by one ownership analysis. IDs are never recycled when
//! restoring a control-flow snapshot: sibling blocks must remain distinct.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RegionId(usize);

pub(super) struct Regions {
    parents: Vec<Option<RegionId>>,
}

impl Regions {
    pub(super) const ROOT: RegionId = RegionId(0);

    pub(super) fn new() -> Self {
        Self {
            parents: vec![None],
        }
    }

    pub(super) fn child(&mut self, parent: RegionId) -> RegionId {
        assert!(
            parent.0 < self.parents.len(),
            "region belongs to this analysis"
        );
        let id = RegionId(self.parents.len());
        self.parents.push(Some(parent));
        id
    }

    /// `source` can support a reference used throughout `destination` only if
    /// source is destination itself or one of its lexical ancestors.
    pub(super) fn outlives(&self, source: RegionId, destination: RegionId) -> bool {
        let mut current = Some(destination);
        while let Some(region) = current {
            if region == source {
                return true;
            }
            current = self.parents[region.0];
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_constraints_distinguish_ancestors_and_siblings() {
        let mut regions = Regions::new();
        let outer = regions.child(Regions::ROOT);
        let left = regions.child(outer);
        let right = regions.child(outer);
        let inner = regions.child(left);
        for (source, destination, expected) in [
            (outer, inner, true),
            (inner, outer, false),
            (left, right, false),
            (right, left, false),
            (inner, inner, true),
            (Regions::ROOT, right, true),
        ] {
            assert_eq!(regions.outlives(source, destination), expected);
        }
        assert_ne!(left, right);
    }
}
