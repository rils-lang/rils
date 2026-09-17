//! host manifests for the analyzer service.
use super::*;

impl Server {
    pub(super) fn load_host_manifests(&mut self, initialization: &Value) -> Result<(), AnyError> {
        let mut paths = match initialization.get("initializationOptions") {
            Some(options) if options.get("hostManifestPaths").is_some() => {
                match manifest_paths(options) {
                    Ok(paths) => paths,
                    Err(error) => {
                        self.show_workspace_error(error.to_string())?;
                        Vec::new()
                    }
                }
            }
            _ => Vec::new(),
        };
        if paths.is_empty() {
            for project in &self.projects {
                paths.extend(project.host_manifests().iter().cloned());
            }
        }
        let (contract, errors) = read_manifests(paths);
        self.install_host_contract(contract);
        for error in errors {
            self.show_workspace_error(error)?;
        }
        Ok(())
    }

    pub(super) fn reload_host_manifests(&mut self, paths: Vec<PathBuf>) -> Result<(), AnyError> {
        let (contract, errors) = read_manifests(paths);
        if !errors.is_empty() {
            // A failed refresh must not destroy the last working host model.
            return Err(invalid_data(errors.join("\n")));
        }
        self.install_host_contract(contract);
        Ok(())
    }

    fn install_host_contract(&mut self, contract: HostContract) {
        self.host_contract = contract;
        self.host_functions = self.host_contract.signatures();
        self.host_types = self
            .host_contract
            .types()
            .map(|ty| ty.name.clone())
            .collect();
    }
}

pub(super) fn manifest_paths(params: &Value) -> Result<Vec<PathBuf>, AnyError> {
    params
        .get("hostManifestPaths")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_data("hostManifestPaths must be an array of paths"))?
        .iter()
        .map(|path| {
            path.as_str()
                .filter(|path| !path.trim().is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| invalid_data("hostManifestPaths contains an invalid path"))
        })
        .collect()
}

fn read_manifests(mut paths: Vec<PathBuf>) -> (HostContract, Vec<String>) {
    paths.sort();
    paths.dedup();
    let mut merged: Option<HostContract> = None;
    let mut errors = Vec::new();
    for path in paths {
        let result = (|| -> Result<HostContract, AnyError> {
            let bytes = fs::read(&path).map_err(|error| {
                invalid_data(format!(
                    "failed to read host manifest `{}`: {error}",
                    path.display()
                ))
            })?;
            let contract = HostContract::from_manifest_bytes(&bytes).map_err(|error| {
                invalid_data(format!(
                    "invalid host manifest `{}`: {error}",
                    path.display()
                ))
            })?;
            if contract.host_abi_version() != HOST_CONTRACT_ABI_VERSION {
                return Err(invalid_data(format!(
                    "host manifest `{}` uses ABI {}, but analyzer supports ABI {HOST_CONTRACT_ABI_VERSION}",
                    path.display(),
                    contract.host_abi_version()
                )));
            }
            if let Some(target) = &merged {
                // merge can mutate before failing: publish only a complete fragment.
                let mut candidate = target.clone();
                candidate.merge(&contract).map_err(|error| {
                    invalid_data(format!(
                        "failed to merge host manifest `{}`: {error}",
                        path.display()
                    ))
                })?;
                Ok(candidate)
            } else {
                Ok(contract)
            }
        })();
        match result {
            Ok(contract) => merged = Some(contract),
            Err(error) => errors.push(error.to_string()),
        }
    }
    (merged.unwrap_or_default(), errors)
}
