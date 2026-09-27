use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use quote::ToTokens;
use syn::{Attribute, ImplItem, Item, TraitItem};

pub(super) const MIN_CLONE_LINES: usize = 8;
pub(super) const MIN_CLONE_TOKENS: usize = 50;

fn sorted_directory_entries(directory: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("read source entry").path())
        .collect::<Vec<_>>();
    entries.sort_unstable();
    entries
}

fn read_source(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {} as UTF-8: {error}", path.display()))
}

#[derive(Clone, Debug)]
pub(super) struct FunctionBody {
    file: String,
    symbol: String,
    tokens: Vec<NormalizedToken>,
}

#[derive(Clone, Debug)]
struct NormalizedToken {
    text: String,
    line: usize,
}

pub(super) fn load_production_functions(
    repository_root: &Path,
    directories: &[&Path],
) -> Vec<FunctionBody> {
    let mut functions = Vec::new();
    for directory in directories {
        collect_production_functions(repository_root, directory, &mut functions);
    }
    functions.sort_by(|left, right| (&left.file, &left.symbol).cmp(&(&right.file, &right.symbol)));
    functions
}

fn collect_production_functions(
    repository_root: &Path,
    directory: &Path,
    functions: &mut Vec<FunctionBody>,
) {
    for path in sorted_directory_entries(directory) {
        if path.is_dir() {
            collect_production_functions(repository_root, &path, functions);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let relative = path.strip_prefix(repository_root).unwrap_or(&path);
            functions.extend(parse_production_functions(relative, &read_source(&path)));
        }
    }
}

fn is_test_only_path(path: &Path) -> bool {
    if path
        .components()
        .any(|component| component.as_os_str() == "tests")
    {
        return true;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem == "tests" || stem.ends_with("_tests"))
}

fn parse_production_functions(path: &Path, source: &str) -> Vec<FunctionBody> {
    if is_test_only_path(path) {
        return Vec::new();
    }
    let syntax = syn::parse_file(source)
        .unwrap_or_else(|error| panic!("parse {} as Rust: {error}", path.display()));
    let mut functions = Vec::new();
    collect_items(path, &syntax.items, &[], &mut functions);
    functions
}

fn collect_items(
    path: &Path,
    items: &[Item],
    module_path: &[String],
    functions: &mut Vec<FunctionBody>,
) {
    for item in items
        .iter()
        .filter(|item| !attributes_are_test_only(item_attrs(item)))
    {
        collect_item(path, item, module_path, functions);
    }
}

fn collect_item(
    path: &Path,
    item: &Item,
    module_path: &[String],
    functions: &mut Vec<FunctionBody>,
) {
    match item {
        Item::Fn(item_fn) => push_function(
            path,
            qualified_name(module_path, &item_fn.sig.ident.to_string()),
            &item_fn.block,
            functions,
        ),
        Item::Impl(item_impl) => collect_impl_methods(path, item_impl, module_path, functions),
        Item::Trait(item_trait) => {
            collect_trait_methods(path, item_trait, module_path, functions);
        }
        Item::Mod(item_mod) => collect_inline_module(path, item_mod, module_path, functions),
        _ => {}
    }
}

fn collect_impl_methods(
    path: &Path,
    item_impl: &syn::ItemImpl,
    module_path: &[String],
    functions: &mut Vec<FunctionBody>,
) {
    let owner = compact_tokens(item_impl.self_ty.as_ref());
    for method in item_impl.items.iter().filter_map(|member| match member {
        ImplItem::Fn(method) if !attributes_are_test_only(&method.attrs) => Some(method),
        _ => None,
    }) {
        push_function(
            path,
            qualified_name(module_path, &format!("{owner}::{}", method.sig.ident)),
            &method.block,
            functions,
        );
    }
}

fn collect_trait_methods(
    path: &Path,
    item_trait: &syn::ItemTrait,
    module_path: &[String],
    functions: &mut Vec<FunctionBody>,
) {
    for method in item_trait.items.iter().filter_map(|member| match member {
        TraitItem::Fn(method) if !attributes_are_test_only(&method.attrs) => Some(method),
        _ => None,
    }) {
        if let Some(block) = &method.default {
            push_function(
                path,
                qualified_name(
                    module_path,
                    &format!("{}::{}", item_trait.ident, method.sig.ident),
                ),
                block,
                functions,
            );
        }
    }
}

fn collect_inline_module(
    path: &Path,
    item_mod: &syn::ItemMod,
    module_path: &[String],
    functions: &mut Vec<FunctionBody>,
) {
    if let Some((_, nested)) = &item_mod.content {
        let mut nested_path = module_path.to_vec();
        nested_path.push(item_mod.ident.to_string());
        collect_items(path, nested, &nested_path, functions);
    }
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn attributes_are_test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        if attribute.path().is_ident("test") {
            return true;
        }
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        let syn::Meta::List(list) = &attribute.meta else {
            return false;
        };
        cfg_tokens_require_test(&list.tokens.to_string().replace(' ', ""))
    })
}

fn cfg_tokens_require_test(expression: &str) -> bool {
    if expression == "test" {
        return true;
    }
    if expression.starts_with("not(") {
        return false;
    }
    expression
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|word| word == "test")
}

fn compact_tokens(tokens: &impl ToTokens) -> String {
    tokens.to_token_stream().to_string().replace(' ', "")
}

fn qualified_name(module_path: &[String], symbol: &str) -> String {
    if module_path.is_empty() {
        symbol.to_owned()
    } else {
        format!("{}::{symbol}", module_path.join("::"))
    }
}

fn push_function(
    path: &Path,
    symbol: String,
    block: &syn::Block,
    functions: &mut Vec<FunctionBody>,
) {
    let mut tokens = Vec::new();
    flatten_tokens(block.to_token_stream(), &mut tokens);
    functions.push(FunctionBody {
        file: path.to_string_lossy().replace('\\', "/"),
        symbol,
        tokens,
    });
}

fn flatten_tokens(stream: TokenStream, tokens: &mut Vec<NormalizedToken>) {
    for token in stream {
        match token {
            TokenTree::Group(group) => {
                let delimiter = group.delimiter();
                if delimiter != Delimiter::None {
                    tokens.push(normalized_token(
                        open_delimiter(delimiter),
                        group.span_open(),
                    ));
                }
                flatten_tokens(group.stream(), tokens);
                if delimiter != Delimiter::None {
                    tokens.push(normalized_token(
                        close_delimiter(delimiter),
                        group.span_close(),
                    ));
                }
            }
            TokenTree::Ident(ident) => {
                tokens.push(normalized_token(format!("i:{ident}"), ident.span()));
            }
            TokenTree::Punct(punct) => tokens.push(normalized_token(
                format!("p:{}:{:?}", punct.as_char(), punct.spacing()),
                punct.span(),
            )),
            TokenTree::Literal(literal) => {
                tokens.push(normalized_token(format!("l:{literal}"), literal.span()));
            }
        }
    }
}

fn normalized_token(text: impl Into<String>, span: Span) -> NormalizedToken {
    NormalizedToken {
        text: text.into(),
        line: span.start().line,
    }
}

const fn open_delimiter(delimiter: Delimiter) -> &'static str {
    match delimiter {
        Delimiter::Parenthesis => "d:(",
        Delimiter::Brace => "d:{",
        Delimiter::Bracket => "d:[",
        Delimiter::None => "",
    }
}

const fn close_delimiter(delimiter: Delimiter) -> &'static str {
    match delimiter {
        Delimiter::Parenthesis => "d:)",
        Delimiter::Brace => "d:}",
        Delimiter::Bracket => "d:]",
        Delimiter::None => "",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CloneLocation {
    file: String,
    symbol: String,
    line_start: usize,
    line_end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CloneMatch {
    left: CloneLocation,
    right: CloneLocation,
    token_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CloneKey {
    left_body: usize,
    left_start: usize,
    right_body: usize,
    right_start: usize,
    token_count: usize,
}

pub(super) fn find_exact_clones(
    functions: &[FunctionBody],
    minimum_tokens: usize,
    minimum_lines: usize,
) -> Vec<CloneMatch> {
    let mut windows: HashMap<u64, Vec<(usize, usize)>> = HashMap::new();
    for (body_index, body) in functions.iter().enumerate() {
        if body.tokens.len() < minimum_tokens {
            continue;
        }
        for start in 0..=body.tokens.len() - minimum_tokens {
            windows
                .entry(token_window_hash(
                    &body.tokens[start..start + minimum_tokens],
                ))
                .or_default()
                .push((body_index, start));
        }
    }

    let mut candidates = HashSet::new();
    for occurrences in windows.values() {
        for left_index in 0..occurrences.len() {
            for right_index in left_index + 1..occurrences.len() {
                let (left_body, left_start) = occurrences[left_index];
                let (right_body, right_start) = occurrences[right_index];
                if left_body == right_body && right_start - left_start < minimum_tokens {
                    continue;
                }
                if !same_tokens(
                    &functions[left_body].tokens[left_start..left_start + minimum_tokens],
                    &functions[right_body].tokens[right_start..right_start + minimum_tokens],
                ) {
                    continue;
                }
                let candidate = maximal_clone(
                    functions,
                    left_body,
                    left_start,
                    right_body,
                    right_start,
                    minimum_tokens,
                );
                if clone_spans_enough_lines(functions, candidate, minimum_lines) {
                    candidates.insert(candidate);
                }
            }
        }
    }

    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .token_count
            .cmp(&left.token_count)
            .then_with(|| clone_key_order(left).cmp(&clone_key_order(right)))
    });
    let mut selected: Vec<CloneKey> = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .all(|existing| !clone_ranges_overlap(candidate, *existing))
        {
            selected.push(candidate);
        }
    }

    let mut clones = selected
        .into_iter()
        .map(|key| clone_match(functions, key))
        .collect::<Vec<_>>();
    clones.sort_by(|left, right| {
        clone_location_order(&left.left)
            .cmp(&clone_location_order(&right.left))
            .then_with(|| {
                clone_location_order(&left.right).cmp(&clone_location_order(&right.right))
            })
    });
    clones
}

fn token_window_hash(tokens: &[NormalizedToken]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for token in tokens {
        token.text.hash(&mut hasher);
    }
    hasher.finish()
}

fn same_tokens(left: &[NormalizedToken], right: &[NormalizedToken]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| left.text == right.text)
}

fn maximal_clone(
    functions: &[FunctionBody],
    left_body: usize,
    mut left_start: usize,
    right_body: usize,
    mut right_start: usize,
    minimum_tokens: usize,
) -> CloneKey {
    let left_tokens = &functions[left_body].tokens;
    let right_tokens = &functions[right_body].tokens;
    while left_start > 0
        && right_start > 0
        && left_tokens[left_start - 1].text == right_tokens[right_start - 1].text
    {
        left_start -= 1;
        right_start -= 1;
    }

    let maximum = if left_body == right_body {
        right_start - left_start
    } else {
        usize::MAX
    };
    let mut token_count = minimum_tokens.min(maximum);
    while token_count < maximum
        && left_start + token_count < left_tokens.len()
        && right_start + token_count < right_tokens.len()
        && left_tokens[left_start + token_count].text
            == right_tokens[right_start + token_count].text
    {
        token_count += 1;
    }

    CloneKey {
        left_body,
        left_start,
        right_body,
        right_start,
        token_count,
    }
}

fn clone_spans_enough_lines(
    functions: &[FunctionBody],
    clone: CloneKey,
    minimum_lines: usize,
) -> bool {
    token_line_span(
        &functions[clone.left_body].tokens[clone.left_start..clone.left_start + clone.token_count],
    ) >= minimum_lines
        && token_line_span(
            &functions[clone.right_body].tokens
                [clone.right_start..clone.right_start + clone.token_count],
        ) >= minimum_lines
}

fn token_line_span(tokens: &[NormalizedToken]) -> usize {
    let start = tokens.iter().map(|token| token.line).find(|line| *line > 0);
    let end = tokens
        .iter()
        .rev()
        .map(|token| token.line)
        .find(|line| *line > 0);
    start
        .zip(end)
        .map_or(0, |(start, end)| end.saturating_sub(start) + 1)
}

fn clone_ranges_overlap(left: CloneKey, right: CloneKey) -> bool {
    left.left_body == right.left_body
        && left.right_body == right.right_body
        && ranges_overlap(
            left.left_start,
            left.token_count,
            right.left_start,
            right.token_count,
        )
        && ranges_overlap(
            left.right_start,
            left.token_count,
            right.right_start,
            right.token_count,
        )
}

const fn ranges_overlap(
    left_start: usize,
    left_length: usize,
    right_start: usize,
    right_length: usize,
) -> bool {
    left_start < right_start + right_length && right_start < left_start + left_length
}

const fn clone_key_order(clone: &CloneKey) -> (usize, usize, usize, usize) {
    (
        clone.left_body,
        clone.left_start,
        clone.right_body,
        clone.right_start,
    )
}

fn clone_match(functions: &[FunctionBody], clone: CloneKey) -> CloneMatch {
    CloneMatch {
        left: clone_location(
            &functions[clone.left_body],
            clone.left_start,
            clone.token_count,
        ),
        right: clone_location(
            &functions[clone.right_body],
            clone.right_start,
            clone.token_count,
        ),
        token_count: clone.token_count,
    }
}

fn clone_location(body: &FunctionBody, start: usize, token_count: usize) -> CloneLocation {
    let tokens = &body.tokens[start..start + token_count];
    CloneLocation {
        file: body.file.clone(),
        symbol: body.symbol.clone(),
        line_start: tokens.first().map_or(0, |token| token.line),
        line_end: tokens.last().map_or(0, |token| token.line),
    }
}

fn clone_location_order(location: &CloneLocation) -> (&str, &str, usize, usize) {
    (
        &location.file,
        &location.symbol,
        location.line_start,
        location.line_end,
    )
}

pub(super) fn format_clone_report(clones: &[CloneMatch]) -> String {
    clones
        .iter()
        .map(|clone| {
            format!(
                "{}::{}:{}-{} <-> {}::{}:{}-{} ({} tokens)",
                clone.left.file,
                clone.left.symbol,
                clone.left.line_start,
                clone.left.line_end,
                clone.right.file,
                clone.right.symbol,
                clone.right.line_start,
                clone.right.line_end,
                clone.token_count,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
