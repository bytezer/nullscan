use std::path::Path;

/// A source language nullscan can parse. TypeScript and TSX are separate variants
/// because they need different tree-sitter grammars (TSX adds JSX), even though they
/// share one rule set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Language {
    TypeScript,
    Tsx,
}

impl Language {
    /// Picks the language from a file extension; `None` means "not a supported file".
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    fn grammar(self) -> tree_sitter::Language {
        match self {
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }

    pub(crate) fn parse(self, source: &str) -> Option<tree_sitter::Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.grammar()).expect("failed to load grammar");
        parser.parse(source, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_extensions() {
        assert_eq!(Language::from_path(Path::new("a.ts")), Some(Language::TypeScript));
        assert_eq!(Language::from_path(Path::new("a.tsx")), Some(Language::Tsx));
        assert_eq!(Language::from_path(Path::new("a.js")), None);
        assert_eq!(Language::from_path(Path::new("Makefile")), None);
    }
}
