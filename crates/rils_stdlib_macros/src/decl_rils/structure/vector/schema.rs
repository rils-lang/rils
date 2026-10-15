//! Translate the declared entry shape to stable language-parameter indices.
use super::*;

pub(super) fn tokens(
    ty: &Type,
    parameters: &[syn::Ident],
) -> syn::Result<proc_macro2::TokenStream> {
    let mut seen = vec![false; parameters.len()];
    let result = shape(ty, parameters, &mut seen)?;
    if seen.iter().any(|seen| !seen) {
        return Err(Error::new_spanned(
            ty,
            "entry schema omits a language type parameter",
        ));
    }
    Ok(result)
}

fn shape(
    ty: &Type,
    parameters: &[syn::Ident],
    seen: &mut [bool],
) -> syn::Result<proc_macro2::TokenStream> {
    if let Some(index) = signature::parameter(ty, parameters) {
        if std::mem::replace(&mut seen[index], true) {
            return Err(Error::new_spanned(
                ty,
                "entry schema repeats a language type parameter",
            ));
        }
        return Ok(quote!(rils_stdlib::storage::EntryShape::Parameter(#index)));
    }
    match ty {
        Type::Tuple(tuple) => {
            let fields = tuple
                .elems
                .iter()
                .map(|ty| shape(ty, parameters, seen))
                .collect::<syn::Result<Vec<_>>>()?;
            Ok(quote!(rils_stdlib::storage::EntryShape::Tuple(&[#(#fields),*])))
        }
        Type::Paren(ty) => shape(&ty.elem, parameters, seen),
        Type::Group(ty) => shape(&ty.elem, parameters, seen),
        _ => Err(Error::new_spanned(
            ty,
            "native entry schema requires language parameters or tuples",
        )),
    }
}
