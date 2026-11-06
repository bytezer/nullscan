//! Copy-paste detection, the same approach as jscpd / PMD CPD: tokenize each file
//! (ignoring comments), hash sliding windows of tokens, and for every pair of files
//! that share a window, extend the match to its true maximal length so one real
//! duplicate is reported once instead of once per overlapping sub-window.
//!
//! By default presentational JSX markup is ignored (see [`DuplicateOptions`]): static
//! markup and SVG icons never make two components look like clones, while the logic
//! inside `{...}` expressions still does.

use std::collections::HashMap;
use std::path::PathBuf;

use rayon::prelude::*;
use tree_sitter::{Node, Tree};

use crate::Language;

pub struct Duplicate {
    pub token_count: usize,
    pub a: DuplicateLocation,
    pub b: DuplicateLocation,
}

pub struct DuplicateLocation {
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
}

struct Token {
    text: String,
    line: usize,
}

struct FileTokens {
    path: PathBuf,
    tokens: Vec<Token>,
}

pub struct DuplicateOptions {
    /// Minimum number of consecutive tokens a clone must share.
    pub min_tokens: usize,
    /// Count JSX markup (tags, attributes, text) as tokens. Off by default so only the
    /// logic inside `{...}` expressions is compared.
    pub compare_markup: bool,
}

pub fn find_duplicates(
    files: &[(PathBuf, Language, String)],
    options: DuplicateOptions,
) -> Vec<Duplicate> {
    let min_tokens = options.min_tokens;
    // Parsing dominates the cost here, so it's done in parallel (each call builds its
    // own `Parser`, which isn't shareable across concurrent calls).
    let file_tokens: Vec<FileTokens> = files
        .par_iter()
        .filter_map(|(path, language, source)| {
            let tree = language.parse(source)?;
            Some(FileTokens {
                path: path.clone(),
                tokens: tokenize(&tree, source, options.compare_markup),
            })
        })
        .collect();

    let buckets = build_windows(&file_tokens, min_tokens);
    let mut visited: Vec<Vec<bool>> = file_tokens.iter().map(|f| vec![false; f.tokens.len()]).collect();

    let mut keys: Vec<&String> = buckets.keys().collect();
    keys.sort();

    let mut out = Vec::new();
    for key in keys {
        let occurrences = &buckets[key];
        for i in 0..occurrences.len() {
            for j in (i + 1)..occurrences.len() {
                let (fa, sa) = occurrences[i];
                let (fb, sb) = occurrences[j];
                if fa == fb && ranges_overlap(sa, sb, min_tokens) {
                    continue;
                }

                let (start_a, start_b, len) =
                    extend_match(&file_tokens[fa].tokens, sa, &file_tokens[fb].tokens, sb);
                if visited[fa][start_a] || visited[fb][start_b] {
                    continue;
                }
                mark_visited(&mut visited[fa], start_a, len);
                mark_visited(&mut visited[fb], start_b, len);

                out.push(Duplicate {
                    token_count: len,
                    a: location(&file_tokens[fa], start_a, len),
                    b: location(&file_tokens[fb], start_b, len),
                });
            }
        }
    }
    out
}

fn location(file: &FileTokens, start: usize, len: usize) -> DuplicateLocation {
    DuplicateLocation {
        file: file.path.clone(),
        start_line: file.tokens[start].line,
        end_line: file.tokens[start + len - 1].line,
    }
}

fn ranges_overlap(a_start: usize, b_start: usize, len: usize) -> bool {
    a_start.abs_diff(b_start) < len
}

/// Extends a matching window both backward and forward to its true maximal length, so
/// any window inside the same real duplicate run converges to the same boundaries
/// regardless of which sub-window found it first.
fn extend_match(a: &[Token], sa: usize, b: &[Token], sb: usize) -> (usize, usize, usize) {
    let mut back = 0;
    while back < sa && back < sb && a[sa - back - 1].text == b[sb - back - 1].text {
        back += 1;
    }
    let mut fwd = 0;
    while sa + fwd < a.len() && sb + fwd < b.len() && a[sa + fwd].text == b[sb + fwd].text {
        fwd += 1;
    }
    (sa - back, sb - back, back + fwd)
}

fn mark_visited(visited: &mut [bool], start: usize, len: usize) {
    for v in &mut visited[start..start + len] {
        *v = true;
    }
}

/// Maps each `min_tokens`-token window's exact content to every (file, start) it occurs
/// at. The key is the tokens joined by a separator that can't appear inside a token, so
/// there's no risk of hash-style false collisions.
fn build_windows(files: &[FileTokens], min_tokens: usize) -> HashMap<String, Vec<(usize, usize)>> {
    let mut buckets: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    for (fi, file) in files.iter().enumerate() {
        if file.tokens.len() < min_tokens {
            continue;
        }
        for start in 0..=file.tokens.len() - min_tokens {
            let key = file.tokens[start..start + min_tokens]
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join("\u{1}");
            buckets.entry(key).or_default().push((fi, start));
        }
    }
    buckets
}

fn tokenize(tree: &Tree, source: &str, compare_markup: bool) -> Vec<Token> {
    let mut out = Vec::new();
    collect_leaves(tree.root_node(), source, compare_markup, &mut out);
    out
}

fn collect_leaves(node: Node, source: &str, compare_markup: bool, out: &mut Vec<Token>) {
    if node.child_count() == 0 {
        push_leaf(node, source, compare_markup, out);
        return;
    }
    if !compare_markup && is_skipped_markup(node, source) {
        collect_expression_leaves(node, source, out);
        return;
    }
    let children = children_of(node);
    for child in children {
        collect_leaves(child, source, compare_markup, out);
    }
}

fn push_leaf(node: Node, source: &str, compare_markup: bool, out: &mut Vec<Token>) {
    if node.kind() == "comment" || (!compare_markup && node.kind() == "jsx_text") {
        return;
    }
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    out.push(Token {
        text: text.to_string(),
        line: node.start_position().row + 1,
    });
}

fn children_of(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// Whether this markup subtree contributes nothing but the logic inside its `{...}`
/// expressions while markup is being ignored: SVG icons and purely static markup.
fn is_skipped_markup(node: Node, source: &str) -> bool {
    match node.kind() {
        "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => {
            is_svg_element(node, source) || !has_jsx_expression(node)
        }
        _ => false,
    }
}

/// Exact, case-sensitive `svg` tag name, either `<svg>` or `<svg />`.
fn is_svg_element(node: Node, source: &str) -> bool {
    let name = match node.kind() {
        "jsx_element" => node
            .child_by_field_name("open_tag")
            .and_then(|opening| opening.child_by_field_name("name")),
        "jsx_self_closing_element" => node.child_by_field_name("name"),
        _ => None,
    };
    match name {
        Some(name) if name.kind() == "identifier" => {
            name.utf8_text(source.as_bytes()).is_ok_and(|text| text == "svg")
        }
        _ => false,
    }
}

fn has_jsx_expression(node: Node) -> bool {
    children_of(node)
        .into_iter()
        .any(|child| child.kind() == "jsx_expression" || has_jsx_expression(child))
}

/// Walks a skipped markup subtree without tokenizing any of its markup, so only the
/// contents of `{...}` expressions survive. Nested markup is not visited twice: it is
/// either ignored the same way or, when it carries the compared logic itself, handed
/// back to the normal collector.
fn collect_expression_leaves(node: Node, source: &str, out: &mut Vec<Token>) {
    if node.child_count() == 0 {
        return;
    }
    for child in children_of(node) {
        match child.kind() {
            "jsx_expression" => collect_leaves(child, source, false, out),
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment" => {
                if is_skipped_markup(child, source) {
                    collect_expression_leaves(child, source, out);
                } else {
                    collect_leaves(child, source, false, out);
                }
            }
            _ => collect_expression_leaves(child, source, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> Vec<(PathBuf, Language, String)> {
        pairs
            .iter()
            .map(|(name, src)| (PathBuf::from(name), Language::TypeScript, src.to_string()))
            .collect()
    }

    fn tsx_files(pairs: &[(&str, &str)]) -> Vec<(PathBuf, Language, String)> {
        pairs
            .iter()
            .map(|(name, src)| (PathBuf::from(name), Language::Tsx, src.to_string()))
            .collect()
    }

    fn options(min_tokens: usize) -> DuplicateOptions {
        DuplicateOptions {
            min_tokens,
            compare_markup: false,
        }
    }

    fn options_with_markup(min_tokens: usize) -> DuplicateOptions {
        DuplicateOptions {
            min_tokens,
            compare_markup: true,
        }
    }

    /// 40 distinct statements so the block is well over the 30-token threshold used below.
    fn big_block(prefix: &str) -> String {
        (0..40)
            .map(|i| format!("{prefix}{i}();"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn flags_identical_blocks_across_files() {
        let block = big_block("call");
        let a = format!("function a() {{\n{block}\n}}");
        let b = format!("function b() {{\n{block}\n}}");
        let dups = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), options(30));
        assert_eq!(dups.len(), 1);
        assert!(dups[0].token_count >= 30);
    }

    #[test]
    fn does_not_flag_distinct_files() {
        let a = big_block("call");
        let b = big_block("other");
        let dups = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), options(30));
        assert!(dups.is_empty());
    }

    #[test]
    fn reports_one_duplicate_not_one_per_window() {
        // A long identical block would hash-match at every sliding offset if we didn't
        // extend to the maximal run and mark it visited — assert we get exactly one.
        let block = big_block("step");
        let a = format!("function a() {{\n{block}\n}}");
        let b = format!("function b() {{\n{block}\n}}");
        let dups = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), options(30));
        assert_eq!(dups.len(), 1);
    }

    #[test]
    fn short_snippets_are_ignored() {
        let a = "const x = 1;\nconst y = 2;";
        let b = "const x = 1;\nconst y = 2;";
        let dups = find_duplicates(&files(&[("a.ts", a), ("b.ts", b)]), options(30));
        assert!(dups.is_empty());
    }

    #[test]
    fn does_not_flag_overlapping_windows_within_the_same_block() {
        // A single big block should never be reported as a duplicate of itself.
        let a = big_block("x");
        let dups = find_duplicates(&files(&[("a.ts", &a)]), options(30));
        assert!(dups.is_empty());
    }

    /// An inline SVG icon with well over 30 markup tokens; only `d` differs per file.
    fn svg_icon(path_d: &str) -> String {
        format!(
            "const icon = <svg width=\"800px\" height=\"800px\" viewBox=\"0 0 32 32\" \
             id=\"svg5\" version=\"1.1\" xmlns=\"http://www.w3.org/2000/svg\" \
             fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" aria-hidden=\"true\">\
             <path d=\"{path_d}\"/><path d=\"M2 2\"/><path d=\"M3 3\"/></svg>;"
        )
    }

    #[test]
    fn svg_markup_is_ignored_by_default() {
        let a = svg_icon("M1 1");
        let b = svg_icon("M9 9");
        let pair = tsx_files(&[("a.tsx", &a), ("b.tsx", &b)]);
        let without_markup = find_duplicates(&pair, options(30));
        assert!(without_markup.is_empty());

        let with_markup = find_duplicates(&pair, options_with_markup(30));
        assert!(!with_markup.is_empty());
    }

    #[test]
    fn svg_expressions_do_not_look_like_markup_duplicates() {
        // Both icons carry the same `{color}` expression, but that is only three tokens.
        let a = svg_icon("M1 1").replace("><path", ">{color}<path");
        let b = svg_icon("M9 9").replace("><path", ">{color}<path");
        let dups = find_duplicates(&tsx_files(&[("a.tsx", &a), ("b.tsx", &b)]), options(30));
        assert!(dups.is_empty());
    }

    #[test]
    fn static_jsx_markup_is_ignored_by_default() {
        let src = "const view = <div className=\"card\"><h2 className=\"title\">Title</h2>\
                   <p className=\"body\">Some body text here</p>\
                   <button className=\"cta\">Click me now</button>\
                   <span className=\"hint\">Hint text</span></div>;";
        let pair = tsx_files(&[("a.tsx", src), ("b.tsx", src)]);
        assert!(find_duplicates(&pair, options(30)).is_empty());
        assert!(!find_duplicates(&pair, options_with_markup(30)).is_empty());
    }

    #[test]
    fn duplicated_logic_inside_jsx_expressions_is_flagged() {
        let logic = "items.filter((item) => item.active)\
                     .map((item) => render(item, { key: item.id, label: item.name }))";
        let a = format!("const a = <section className=\"alpha\">{{{logic}}}</section>;");
        let b = format!("const b = <article data-role=\"beta\">{{{logic}}}</article>;");
        let dups = find_duplicates(&tsx_files(&[("a.tsx", &a), ("b.tsx", &b)]), options(30));
        assert!(!dups.is_empty());
    }

    #[test]
    fn jsx_text_does_not_create_duplicates() {
        let text = "lorem ipsum dolor sit amet ".repeat(12);
        let a = format!("const first = <p className=\"alpha\">{{value}}{text}</p>;");
        let b = format!("const second = <section className=\"beta\">{{value}}{text}</section>;");
        let dups = find_duplicates(&tsx_files(&[("a.tsx", &a), ("b.tsx", &b)]), options(10));
        assert!(dups.is_empty());

        let tree = Language::Tsx.parse(&a).expect("parse");
        let tokens = tokenize(&tree, &a, false);
        assert!(!tokens.iter().any(|token| token.text.starts_with("lorem")));
    }
}
