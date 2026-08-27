use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro::TokenStream;
use syn::{LitStr, parse_macro_input};

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let config_path = parse_macro_input!(input as LitStr).value();
    let (config_path, members) = match load(&config_path) {
        Ok(loaded) => loaded,
        Err(error) => return compile_error(error),
    };
    declarations(&config_path, &members)
}

pub(crate) type Members = BTreeMap<String, (u32, String)>;

pub(crate) fn load(relative_path: &str) -> Result<(PathBuf, Members), String> {
    let config_path = manifest_path(relative_path)?;
    let config = fs::read_to_string(&config_path)
        .map_err(|error| format!("failed to read `{}`: {error}", config_path.display()))?;
    Ok((config_path, members(&config)?))
}

fn declarations(config_path: &Path, members: &BTreeMap<String, (u32, String)>) -> TokenStream {
    let constants = members
        .values()
        .map(|(id, name)| format!("pub const {name}: Self = Self::from_raw({id}u32);"))
        .collect::<String>();
    let pattern_constants = members
        .values()
        .map(|(_, name)| format!("pub const {name}: BuiltinId = BuiltinId::{name};"))
        .collect::<String>();
    let builtin_id_arms = members
        .iter()
        .map(|(key, (id, _))| format!("({key:?}) => {{ $crate::BuiltinId::from_raw({id}u32) }};"))
        .collect::<String>();
    let all_ids = members
        .values()
        .map(|(_, name)| format!("Self::{name},"))
        .collect::<String>();
    let canonical_path_arms = members
        .iter()
        .map(|(key, (_, name))| format!("Self::{name} => {key:?},"))
        .collect::<String>();
    let member_name_arms = members
        .iter()
        .map(|(key, (_, name))| {
            let member_name = key.rsplit("::").next().expect("non-empty built-in path");
            format!("Self::{name} => {member_name:?},")
        })
        .collect::<String>();
    let config_path_literal = format!("{config_path:?}");
    format!(
        r#"const _: &str = include_str!({config_path_literal});
         #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
         #[repr(transparent)]
         pub struct BuiltinId(u32);
         #[allow(non_upper_case_globals)]
         impl BuiltinId {{
             pub const ALL: &'static [Self] = &[{all_ids}];
             pub const fn from_raw(raw: u32) -> Self {{ Self(raw) }}
             pub const fn as_raw(self) -> u32 {{ self.0 }}
             pub const fn canonical_path(self) -> Option<&'static str> {{
                 Some(match self {{ {canonical_path_arms} _ => return None }})
             }}
             pub const fn member_name(self) -> Option<&'static str> {{
                 Some(match self {{ {member_name_arms} _ => return None }})
             }}
             {constants}
         }}
         #[doc(hidden)]
         #[allow(non_upper_case_globals)]
         pub mod builtin_ids {{
             use super::BuiltinId;
             {pattern_constants}
         }}
         #[doc = "Resolves a canonical built-in path to its stable ID at compile time."]
         #[macro_export]
         macro_rules! builtin_id {{
             {builtin_id_arms}
             ($key:literal) => {{
                 compile_error!(concat!("unknown built-in `", $key, "`"))
             }};
         }}"#
    )
    .parse()
    .expect("valid BuiltinId declarations")
}

fn manifest_path(relative_path: &str) -> Result<PathBuf, String> {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or("CARGO_MANIFEST_DIR is not available while expanding built-in IDs")?;
    Ok(PathBuf::from(manifest_dir).join(relative_path))
}

fn members(config: &str) -> Result<BTreeMap<String, (u32, String)>, String> {
    let value: toml::Value = config
        .parse()
        .map_err(|error| format!("invalid builtin_ids.toml: {error}"))?;
    let mut output = BTreeMap::new();
    flatten(
        value
            .as_table()
            .ok_or("built-in ID config must be a table")?,
        &mut Vec::new(),
        &mut output,
    )?;
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for (key, (id, name)) in &output {
        if !ids.insert(*id) {
            return Err(format!("duplicate built-in ID {id:#x}"));
        }
        if !names.insert(name.clone()) {
            return Err(format!("duplicate generated name `{name}` for `{key}`"));
        }
    }
    Ok(output)
}

fn flatten(
    table: &toml::Table,
    path: &mut Vec<String>,
    output: &mut BTreeMap<String, (u32, String)>,
) -> Result<(), String> {
    for (key, value) in table {
        path.push(key.clone());
        match value {
            toml::Value::Table(child) => flatten(child, path, output)?,
            toml::Value::Integer(id) if (0..=u32::MAX as i64).contains(id) => {
                let canonical = path.join("::");
                output.insert(canonical, (*id as u32, rust_name(path)?));
            }
            _ => return Err(format!("`{}` must be a u32 integer", path.join("."))),
        }
        path.pop();
    }
    Ok(())
}

fn rust_name(path: &[String]) -> Result<String, String> {
    let parts = path.get(1..).ok_or("built-in key must start with `core`")?;
    if path.first().is_none_or(|part| part != "core") || parts.is_empty() {
        return Err(format!("invalid built-in path `{}`", path.join(".")));
    }
    let mut name = String::new();
    for part in parts {
        let part = match part.as_str() {
            "fmt" => "formatter",
            other => other,
        };
        for word in part.split('_') {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                name.extend(first.to_uppercase());
                name.extend(chars);
            }
        }
    }
    Ok(name)
}

fn compile_error(message: impl AsRef<str>) -> TokenStream {
    format!("compile_error!({:?});", message.as_ref())
        .parse()
        .expect("valid compile_error")
}
