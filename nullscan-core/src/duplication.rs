//! Copy-paste detection, the same approach as jscpd / PMD CPD: tokenize each file
//! (ignoring comments), hash sliding windows of tokens, and for every pair of files
//! that share a window, extend the match to its true maximal length so one real
//! duplicate is reported once instead of once per overlapping sub-window.

use std::collections::HashMap;
use std::path::PathBuf;

use rayon::prelude::*;
use tree_sitter::{Node, Parser, Tree};

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

pub fn find_duplicates(files: &[(PathBuf, String)], min_tokens: usize) -> Vec<Duplicate> {
    // Parsing dominates the cost here, so it's done in parallel; a `Parser` isn't
    // shareable across concurrent calls, so each closure invocation gets its own
    // (cheap to construct, unlike the tree it produces).
    let file_tokens: Vec<FileTokens> = files
        .par_iter()
        .filter_map(|(path, source)| {
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
                .expect("failed to load TypeScript grammar");
            let tree = parser.parse(source, None)?;
            Some(FileTokens {
                path: path.clone(),
                tokens: tokenize(&tree, source),
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

fn tokenize(tree: &Tree, source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    collect_leaves(tree.root_node(), source, &mut out);
    out
}

fn collect_leaves(node: Node, source: &str, out: &mut Vec<Token>) {
    if node.child_count() == 0 {
        if node.kind() != "comment" {
            if let Ok(text) = node.utf8_text(source.as_bytes()) {
                if !text.trim().is_empty() {
                    out.push(Token {
                        text: text.to_string(),
                        line: node.start_position().row + 1,
                    });
                }
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_leaves(child, source, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(pairs: &[(&str, &str)]) -> Vec<(PathBuf, String)> {
        pairs
            .iter()
            .map(|(name, src)| (PathBuf::from(name), src.to_string()))
            .collect()
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
        let dups = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), 30);
        assert_eq!(dups.len(), 1);
        assert!(dups[0].token_count >= 30);
    }

    #[test]
    fn does_not_flag_distinct_files() {
        let a = big_block("call");
        let b = big_block("other");
        let dups = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), 30);
        assert!(dups.is_empty());
    }

    #[test]
    fn reports_one_duplicate_not_one_per_window() {
        // A long identical block would hash-match at every sliding offset if we didn't
        // extend to the maximal run and mark it visited — assert we get exactly one.
        let block = big_block("step");
        let a = format!("function a() {{\n{block}\n}}");
        let b = format!("function b() {{\n{block}\n}}");
        let dups = find_duplicates(&files(&[("a.ts", &a), ("b.ts", &b)]), 30);
        assert_eq!(dups.len(), 1);
    }

    #[test]
    fn short_snippets_are_ignored() {
        let a = "const x = 1;\nconst y = 2;";
        let b = "const x = 1;\nconst y = 2;";
        let dups = find_duplicates(&files(&[("a.ts", a), ("b.ts", b)]), 30);
        assert!(dups.is_empty());
    }

    #[test]
    fn does_not_flag_overlapping_windows_within_the_same_block() {
        // A single big block should never be reported as a duplicate of itself.
        let a = big_block("x");
        let dups = find_duplicates(&files(&[("a.ts", &a)]), 30);
        assert!(dups.is_empty());
    }
}
