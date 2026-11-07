//! Copy-paste detection, the same approach as jscpd / PMD CPD: tokenize each file
//! (ignoring comments), hash sliding windows of tokens, and for every pair of files
//! that share a window, extend the match to its true maximal length so one real
//! duplicate is reported once instead of once per overlapping sub-window.
//!
//! By default presentational JSX markup is ignored (see [`DuplicateOptions`]): static
//! markup and SVG icons never make two components look like clones, while the logic
//! inside `{...}` expressions still does.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tree_sitter::{Node, Tree};

use crate::Language;

pub struct Duplicate {
    pub token_count: usize,
    /// Lines spanned by the first side, the user-facing size of the clone.
    pub line_count: usize,
    pub a: DuplicateLocation,
    pub b: DuplicateLocation,
}

impl Duplicate {
    pub fn new(token_count: usize, a: DuplicateLocation, b: DuplicateLocation) -> Self {
        let line_count = a.end_line.saturating_sub(a.start_line) + 1;
        Self {
            token_count,
            line_count,
            a,
            b,
        }
    }
}

pub struct DuplicateLocation {
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
}

/// Copies plus the line-density metric of the files that went into the pass.
pub struct DuplicateReport {
    pub duplicates: Vec<Duplicate>,
    pub stats: DuplicationStats,
}

/// Human-facing duplication metric: how many lines of code live inside a clone.
#[derive(Default)]
pub struct DuplicationStats {
    /// Lines of code covered by at least one clone, each line counted once per file.
    pub duplicated_lines: usize,
    /// Lines of code across every file included in the pass.
    pub total_lines: usize,
    pub duplicated_blocks: usize,
    /// Files with at least one duplicated line of code.
    pub duplicated_files: usize,
    pub largest_block_lines: usize,
}

impl DuplicationStats {
    /// `duplicated_lines / total_lines` as a percentage rounded to one decimal.
    pub fn density(&self) -> f64 {
        if self.total_lines == 0 {
            return 0.0;
        }
        let percent = self.duplicated_lines as f64 / self.total_lines as f64 * 100.0;
        (percent * 10.0).round() / 10.0
    }
}

struct Token {
    text: String,
    line: usize,
}

struct FileTokens {
    path: PathBuf,
    tokens: Vec<Token>,
    /// One flag per source line, `true` for lines of code (see [`loc_lines`]).
    loc_lines: Vec<bool>,
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
) -> DuplicateReport {
    let file_tokens = parse_files(files, options.compare_markup);
    let duplicates = detect_duplicates(&file_tokens, options.min_tokens);
    let stats = duplication_stats(&file_tokens, &duplicates);
    DuplicateReport { duplicates, stats }
}

/// Parses every file once, in parallel (each call builds its own `Parser`, which isn't
/// shareable across concurrent calls). Files that fail to parse drop out of the pass.
fn parse_files(
    files: &[(PathBuf, Language, String)],
    compare_markup: bool,
) -> Vec<FileTokens> {
    files
        .par_iter()
        .filter_map(|(path, language, source)| {
            let tree = language.parse(source)?;
            Some(FileTokens {
                path: path.clone(),
                tokens: tokenize(&tree, source, compare_markup),
                loc_lines: loc_lines(&tree, source),
            })
        })
        .collect()
}

fn detect_duplicates(file_tokens: &[FileTokens], min_tokens: usize) -> Vec<Duplicate> {
    let buckets = build_windows(file_tokens, min_tokens);
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

                out.push(Duplicate::new(
                    len,
                    location(&file_tokens[fa], start_a, len),
                    location(&file_tokens[fb], start_b, len),
                ));
            }
        }
    }
    out
}

/// Counts the line-density metric over the files that took part in the pass: clone
/// spans are line ranges, so blank and comment-only lines inside a clone are not code.
fn duplication_stats(files: &[FileTokens], duplicates: &[Duplicate]) -> DuplicationStats {
    let index: HashMap<&Path, usize> = files
        .iter()
        .enumerate()
        .map(|(position, file)| (file.path.as_path(), position))
        .collect();
    let mut duplicated: Vec<Vec<bool>> = files
        .iter()
        .map(|file| vec![false; file.loc_lines.len()])
        .collect();

    for duplicate in duplicates {
        mark_duplicate_side(files, &mut duplicated, &index, &duplicate.a);
        mark_duplicate_side(files, &mut duplicated, &index, &duplicate.b);
    }

    DuplicationStats {
        duplicated_lines: duplicated.iter().flatten().filter(|marked| **marked).count(),
        total_lines: files
            .iter()
            .flat_map(|file| file.loc_lines.iter())
            .filter(|is_code| **is_code)
            .count(),
        duplicated_blocks: duplicates.len(),
        duplicated_files: duplicated
            .iter()
            .filter(|lines| lines.iter().any(|marked| *marked))
            .count(),
        largest_block_lines: duplicates
            .iter()
            .map(|duplicate| duplicate.line_count)
            .max()
            .unwrap_or(0),
    }
}

/// Marks the code lines of one clone side. Spans from several clones union, so a line
/// shared by two blocks is still one duplicated line.
fn mark_duplicate_side(
    files: &[FileTokens],
    duplicated: &mut [Vec<bool>],
    index: &HashMap<&Path, usize>,
    side: &DuplicateLocation,
) {
    let Some(&position) = index.get(side.file.as_path()) else {
        return;
    };
    let loc_lines = &files[position].loc_lines;
    if loc_lines.is_empty() {
        return;
    }
    let start = side.start_line.saturating_sub(1).min(loc_lines.len() - 1);
    let end = side.end_line.saturating_sub(1).min(loc_lines.len() - 1);
    for (line, marked) in duplicated[position].iter_mut().enumerate().take(end + 1) {
        if line >= start && loc_lines[line] {
            *marked = true;
        }
    }
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

/// One flag per source line: a line is a line of code when it has a non-whitespace
/// character outside every comment node, so code with a trailing comment counts and
/// blank or comment-only lines do not. Comments come from the existing tree, no re-parse.
fn loc_lines(tree: &Tree, source: &str) -> Vec<bool> {
    let mut comments = Vec::new();
    collect_comment_ranges(tree.root_node(), &mut comments);

    let mut lines = Vec::new();
    let mut offset = 0;
    for line in source.split('\n') {
        lines.push(line_has_code(line, offset, &comments));
        offset += line.len() + 1;
    }
    lines
}

fn collect_comment_ranges(node: Node, out: &mut Vec<(usize, usize)>) {
    if node.kind() == "comment" {
        let range = node.byte_range();
        out.push((range.start, range.end));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_comment_ranges(child, out);
    }
}

fn line_has_code(line: &str, line_start: usize, comments: &[(usize, usize)]) -> bool {
    line.char_indices().any(|(offset, character)| {
        !character.is_whitespace() && !is_commented(line_start + offset, comments)
    })
}

fn is_commented(position: usize, comments: &[(usize, usize)]) -> bool {
    comments
        .iter()
        .any(|&(start, end)| position >= start && position < end)
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

    fn side(file: &str, start_line: usize, end_line: usize) -> DuplicateLocation {
        DuplicateLocation {
            file: PathBuf::from(file),
            start_line,
            end_line,
        }
    }

    fn source_loc_lines(source: &str) -> Vec<bool> {
        let tree = Language::TypeScript.parse(source).expect("parse");
        loc_lines(&tree, source)
    }

    fn file_with_loc(path: &str, loc_lines: &[bool]) -> FileTokens {
        FileTokens {
            path: PathBuf::from(path),
            tokens: Vec::new(),
            loc_lines: loc_lines.to_vec(),
        }
    }

    #[test]
    fn flags_identical_blocks_across_files() {
        let block = big_block("call");
        let a = format!("function a() {{\n{block}\n}}");
        let b = format!("function b() {{\n{block}\n}}");
        let report = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), options(30));
        assert_eq!(report.duplicates.len(), 1);
        assert!(report.duplicates[0].token_count >= 30);
    }

    #[test]
    fn does_not_flag_distinct_files() {
        let a = big_block("call");
        let b = big_block("other");
        let report = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), options(30));
        assert!(report.duplicates.is_empty());
    }

    #[test]
    fn reports_one_duplicate_not_one_per_window() {
        // A long identical block would hash-match at every sliding offset if we didn't
        // extend to the maximal run and mark it visited — assert we get exactly one.
        let block = big_block("step");
        let a = format!("function a() {{\n{block}\n}}");
        let b = format!("function b() {{\n{block}\n}}");
        let report = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), options(30));
        assert_eq!(report.duplicates.len(), 1);
    }

    #[test]
    fn short_snippets_are_ignored() {
        let a = "const x = 1;\nconst y = 2;";
        let b = "const x = 1;\nconst y = 2;";
        let report = find_duplicates(&files(&[("a.ts", a), ("b.ts", b)]), options(30));
        assert!(report.duplicates.is_empty());
    }

    #[test]
    fn does_not_flag_overlapping_windows_within_the_same_block() {
        // A single big block should never be reported as a duplicate of itself.
        let a = big_block("x");
        let report = find_duplicates(&files(&[("a.ts", &a)]), options(30));
        assert!(report.duplicates.is_empty());
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
        assert!(without_markup.duplicates.is_empty());

        let with_markup = find_duplicates(&pair, options_with_markup(30));
        assert!(!with_markup.duplicates.is_empty());
    }

    #[test]
    fn svg_expressions_do_not_look_like_markup_duplicates() {
        // Both icons carry the same `{color}` expression, but that is only three tokens.
        let a = svg_icon("M1 1").replace("><path", ">{color}<path");
        let b = svg_icon("M9 9").replace("><path", ">{color}<path");
        let report = find_duplicates(&tsx_files(&[("a.tsx", &a), ("b.tsx", &b)]), options(30));
        assert!(report.duplicates.is_empty());
    }

    #[test]
    fn static_jsx_markup_is_ignored_by_default() {
        let src = "const view = <div className=\"card\"><h2 className=\"title\">Title</h2>\
                   <p className=\"body\">Some body text here</p>\
                   <button className=\"cta\">Click me now</button>\
                   <span className=\"hint\">Hint text</span></div>;";
        let pair = tsx_files(&[("a.tsx", src), ("b.tsx", src)]);
        assert!(find_duplicates(&pair, options(30)).duplicates.is_empty());
        assert!(!find_duplicates(&pair, options_with_markup(30)).duplicates.is_empty());
    }

    #[test]
    fn duplicated_logic_inside_jsx_expressions_is_flagged() {
        let logic = "items.filter((item) => item.active)\
                     .map((item) => render(item, { key: item.id, label: item.name }))";
        let a = format!("const a = <section className=\"alpha\">{{{logic}}}</section>;");
        let b = format!("const b = <article data-role=\"beta\">{{{logic}}}</article>;");
        let report = find_duplicates(&tsx_files(&[("a.tsx", &a), ("b.tsx", &b)]), options(30));
        assert!(!report.duplicates.is_empty());
    }

    #[test]
    fn jsx_text_does_not_create_duplicates() {
        let text = "lorem ipsum dolor sit amet ".repeat(12);
        let a = format!("const first = <p className=\"alpha\">{{value}}{text}</p>;");
        let b = format!("const second = <section className=\"beta\">{{value}}{text}</section>;");
        let report = find_duplicates(&tsx_files(&[("a.tsx", &a), ("b.tsx", &b)]), options(10));
        assert!(report.duplicates.is_empty());

        let tree = Language::Tsx.parse(&a).expect("parse");
        let tokens = tokenize(&tree, &a, false);
        assert!(!tokens.iter().any(|token| token.text.starts_with("lorem")));
    }

    #[test]
    fn loc_lines_require_code_outside_comments() {
        let source = concat!(
            "// leading comment\n",
            "\n",
            "const a = 1;   // trailing comment\n",
            "  \n",
            "/* block\n",
            "   comment */\n",
            "const b = \"// not a comment\";\n",
            "const c = 3; /* block start\n",
            "   of a comment */\n",
        );
        let flags = source_loc_lines(source);

        assert_eq!(
            flags.iter().filter(|is_code| **is_code).count(),
            3,
            "flags: {flags:?}"
        );
        assert_eq!(flags[0], false, "comment-only line");
        assert_eq!(flags[1], false, "blank line");
        assert_eq!(flags[2], true, "code plus a trailing comment");
        assert_eq!(flags[3], false, "whitespace-only line");
        assert_eq!(flags[4], false, "block-comment opener");
        assert_eq!(flags[5], false, "block-comment body");
        assert_eq!(flags[6], true, "`//` inside a string is code");
        assert_eq!(flags[7], true, "code plus a block-comment opener");
        assert_eq!(flags[8], false, "block-comment continuation");
    }

    #[test]
    fn clone_metrics_skip_comment_and_blank_lines_inside_a_clone() {
        let block = big_block("call").replacen("call20();", "// shared comment\n\ncall20();", 1);
        let a = format!("function a() {{\n{block}\n}}");
        let b = format!("function b() {{\n{block}\n}}");
        let report = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), options(30));

        assert_eq!(report.duplicates.len(), 1);
        // The clone spans 44 lines but only the 42 code lines on each side count.
        assert_eq!(report.duplicates[0].line_count, 44);
        assert_eq!(report.stats.duplicated_lines, 84);
        assert_eq!(report.stats.total_lines, 84);
        assert_eq!(report.stats.duplicated_blocks, 1);
        assert_eq!(report.stats.duplicated_files, 2);
        assert_eq!(report.stats.largest_block_lines, 44);
        assert_eq!(report.stats.density(), 100.0);
    }

    #[test]
    fn clone_sides_are_unioned_per_file() {
        // Indices 3, 8 and 9 are blank or comment-only lines, so they never count as LOC.
        let loc: Vec<bool> = (0..10)
            .map(|line| line != 3 && line != 8 && line != 9)
            .collect();
        let duplicates = vec![
            // Overlapping sides: lines 2-6 and 5-8 share lines 5 and 6.
            Duplicate::new(30, side("a.ts", 2, 6), side("a.ts", 5, 8)),
            Duplicate::new(30, side("a.ts", 9, 10), side("missing.ts", 1, 2)),
        ];

        let stats = duplication_stats(&[file_with_loc("a.ts", &loc)], &duplicates);

        // Union of indices 1-7 minus the blank index 3; the missing file is ignored.
        assert_eq!(stats.duplicated_lines, 6);
        assert_eq!(stats.total_lines, 7);
        assert_eq!(stats.duplicated_files, 1);
        assert_eq!(stats.duplicated_blocks, 2);
        assert_eq!(stats.largest_block_lines, 5);
        assert_eq!(stats.density(), 85.7);
    }

    #[test]
    fn density_rounds_to_one_decimal() {
        let density = |duplicated_lines, total_lines| {
            DuplicationStats {
                duplicated_lines,
                total_lines,
                ..DuplicationStats::default()
            }
            .density()
        };

        assert_eq!(density(1, 3), 33.3);
        assert_eq!(density(2, 3), 66.7);
        assert_eq!(density(1, 8), 12.5);
        assert_eq!(density(1, 2), 50.0);
        assert_eq!(density(0, 4), 0.0);
    }

    #[test]
    fn no_duplicates_mean_zero_density() {
        let report = find_duplicates(&files(&[("a.ts", "const a = 1;")]), options(30));

        assert!(report.duplicates.is_empty());
        assert_eq!(report.stats.duplicated_lines, 0);
        assert_eq!(report.stats.total_lines, 1);
        assert_eq!(report.stats.duplicated_blocks, 0);
        assert_eq!(report.stats.duplicated_files, 0);
        assert_eq!(report.stats.largest_block_lines, 0);
        assert_eq!(report.stats.density(), 0.0);
    }

    #[test]
    fn density_is_zero_without_any_line_of_code() {
        let stats = duplication_stats(&[file_with_loc("a.ts", &[false, false])], &[]);

        assert_eq!(stats.total_lines, 0);
        assert_eq!(stats.density(), 0.0);
    }
}
