use proc_macro2::TokenStream;
use syn::{punctuated::Punctuated, Attribute, Meta, Token};

/// Matches an attribute produced by a `cfg_attr` whose predicate is provably
/// active in every Cargo configuration, without guessing feature values.
pub fn active_cfg_attr_path_matches(attr: &Attribute, segments: &[&str]) -> bool {
    if !attr.path().is_ident("cfg_attr") {
        return false;
    }
    let Ok(arguments) = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
    else {
        return false;
    };
    let mut arguments = arguments.iter();
    let Some(predicate) = arguments.next() else {
        return false;
    };
    if cfg_predicate_truth(predicate) != Some(true) {
        return false;
    }

    arguments.any(|attribute| meta_path_matches(attribute, segments))
}

/// Returns the arguments of a `cfg_attr` attribute that is provably active in
/// every Cargo configuration, without guessing feature values.
pub fn active_cfg_attr_args(attr: &Attribute, segments: &[&str]) -> Option<TokenStream> {
    if !attr.path().is_ident("cfg_attr") {
        return None;
    }
    let arguments = attr
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()?;
    let mut arguments = arguments.iter();
    let predicate = arguments.next()?;
    if cfg_predicate_truth(predicate) != Some(true) {
        return None;
    }

    arguments.find_map(|attribute| match attribute {
        Meta::List(list) if meta_path_matches(attribute, segments) => Some(list.tokens.clone()),
        _ => None,
    })
}

fn meta_path_matches(meta: &Meta, segments: &[&str]) -> bool {
    let path = match meta {
        Meta::Path(path) => path,
        Meta::List(list) => &list.path,
        Meta::NameValue(name_value) => &name_value.path,
    };
    let actual = path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();
    actual
        .iter()
        .map(String::as_str)
        .eq(segments.iter().copied())
        || (actual.len() >= segments.len()
            && actual
                .iter()
                .map(String::as_str)
                .rev()
                .zip(segments.iter().rev().copied())
                .all(|(actual, expected)| actual == expected))
}

fn cfg_predicate_truth(meta: &Meta) -> Option<bool> {
    let Meta::List(list) = meta else {
        return None;
    };
    let operator = list.path.get_ident()?.to_string();
    let predicates = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .ok()?;

    match operator.as_str() {
        "all" => {
            if predicates.is_empty() {
                return Some(true);
            }
            let mut result = true;
            for predicate in &predicates {
                match cfg_predicate_truth(predicate) {
                    Some(false) => return Some(false),
                    Some(true) => {}
                    None => result = false,
                }
            }
            result.then_some(true)
        }
        "any" => {
            if predicates.is_empty() {
                return Some(false);
            }
            let mut saw_unknown = false;
            for predicate in &predicates {
                match cfg_predicate_truth(predicate) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => saw_unknown = true,
                }
            }
            (!saw_unknown).then_some(false)
        }
        "not" if predicates.len() == 1 => cfg_predicate_truth(&predicates[0]).map(|value| !value),
        _ => None,
    }
}
