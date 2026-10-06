use super::*;

impl DeclarationTypeResolver {
    pub fn inaccessible_type(&self, ty: &Type, module: &[String]) -> Option<String> {
        let child = |ty: &Type| self.inaccessible_type(ty, module);
        let children = |types: &[Type]| types.iter().find_map(child);
        match ty {
            Type::Named { name, arguments } => self
                .inaccessible_type_path(name, module)
                .or_else(|| children(arguments)),
            Type::Tuple(types) => children(types),
            Type::Option(ty)
            | Type::Slice(ty)
            | Type::Array { element: ty, .. }
            | Type::ArrayParameter { element: ty, .. }
            | Type::Reference { inner: ty, .. } => child(ty),
            Type::Result(ok, error) => child(ok).or_else(|| child(error)),
            Type::Function {
                parameters,
                return_type,
            } => parameters
                .as_ref()
                .and_then(|types| children(types))
                .or_else(|| child(return_type)),
            Type::BoundVariable { bounds, .. } => children(bounds),
            Type::Associated {
                base, arguments, ..
            } => child(base).or_else(|| children(arguments)),
            _ => None,
        }
    }

    /// Rejects private type paths used outside their declaring module tree.
    /// This checks source paths before transparent aliases are expanded.
    pub fn inaccessible_type_path(&self, name: &str, module: &[String]) -> Option<String> {
        let path = self.path(name, module, &mut HashSet::new()).or_else(|| {
            let (owner, _) = name.rsplit_once("::")?;
            self.path(owner, module, &mut HashSet::new())
        })?;
        let mut prefix = path.as_str();
        loop {
            if let Some(owner) = self.private_paths.get(prefix)
                && !module.starts_with(owner)
            {
                return Some(prefix.to_owned());
            }
            match prefix.rsplit_once("::") {
                Some((parent, _)) => prefix = parent,
                None => return None,
            }
        }
    }
}
