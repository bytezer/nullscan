//! Owned, annotation-only type summaries of a module.
//!
//! This is not type inference: it only records what a file writes down explicitly
//! (`type` aliases, `interface`s, annotated or `async` exported functions, imports) so a
//! rule can follow a declared type from one file to another. Nothing here stores
//! tree-sitter nodes, and nothing touches the filesystem; a [`ModuleResolver`] supplied
//! by the caller turns an import specifier into another file's summary.
//!
//! Not covered: generics, `ReturnType<...>`, mapped/conditional types, `export *`,
//! default imports, class members and declaration merging.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tree_sitter::{Node, Tree};

use crate::Language;

/// How many aliases, imports and `extends` hops are followed before giving up. Also the
/// cycle guard for `type A = B; type B = A`.
const MAX_DEPTH: usize = 8;

/// What a declared type is, as far as promise detection cares.
#[derive(Debug, Clone)]
pub enum TypeRef {
    /// A reference to a named type, possibly imported. `T | null | undefined` and the
    /// optional marker are already unwrapped.
    Named(String),
    /// An object type literal or an interface body.
    Object(Arc<ObjectType>),
    /// A function or method type; `promise` is whether it returns `Promise<...>`.
    Func { promise: bool },
    Other,
}

#[derive(Debug, Default)]
pub struct ObjectType {
    members: HashMap<String, TypeRef>,
    extends: Vec<String>,
}

/// Where a local name comes from: `imported` is the exported name, or `*` for a
/// namespace import.
#[derive(Debug, Clone)]
struct ImportRef {
    specifier: String,
    imported: String,
}

#[derive(Debug, Default)]
pub struct ModuleSummary {
    /// The file this summary describes; relative specifiers resolve against it.
    pub path: Option<PathBuf>,
    types: HashMap<String, TypeRef>,
    /// Exported functions that are `async` or declare a `Promise<...>` return type.
    promise_fns: HashSet<String>,
    /// Local name -> import. Default imports are skipped.
    imports: HashMap<String, ImportRef>,
    /// `export { A as B } from "./x"`: exported name -> source.
    reexports: HashMap<String, ImportRef>,
}

/// Turns an import specifier written in file `from` into that module's summary.
/// Implementations own all IO and caching; `None` means "cannot tell".
pub trait ModuleResolver: Send + Sync {
    fn summary(&self, from: &Path, specifier: &str) -> Option<Arc<ModuleSummary>>;
}

/// A declared type together with the module whose names it must be read in.
#[derive(Clone)]
pub struct Typed {
    pub ty: TypeRef,
    module: Arc<ModuleSummary>,
}

/// Follows declared types across aliases, interfaces and imports.
pub struct Types<'a> {
    local: Arc<ModuleSummary>,
    resolver: Option<&'a dyn ModuleResolver>,
}

/// Parses `source` and summarizes it; for resolvers that load modules from disk.
pub fn summarize(source: &str, language: Language, path: PathBuf) -> Option<ModuleSummary> {
    let tree = language.parse(source)?;
    Some(ModuleSummary::from_tree(&tree, source, Some(path)))
}

impl ModuleSummary {
    pub fn from_tree(tree: &Tree, source: &str, path: Option<PathBuf>) -> Self {
        let mut summary = ModuleSummary { path, ..Default::default() };
        let root = tree.root_node();
        let mut cursor = root.walk();
        for statement in root.named_children(&mut cursor) {
            summary.add_statement(statement, source, false);
        }
        summary
    }

    fn add_statement(&mut self, node: Node<'_>, source: &str, exported: bool) {
        match node.kind() {
            "export_statement" => self.add_export(node, source),
            "import_statement" => self.add_import(node, source),
            "type_alias_declaration" => {
                if let (Some(name), Some(value)) = (field_text(node, "name", source), node.child_by_field_name("value")) {
                    self.types.insert(name, classify(value, source));
                }
            }
            "interface_declaration" => self.add_interface(node, source),
            "function_declaration" => {
                if exported && declares_promise(node, source) {
                    self.promise_fns.extend(field_text(node, "name", source));
                }
            }
            "lexical_declaration" | "variable_declaration" if exported => {
                self.add_exported_variables(node, source);
            }
            _ => {}
        }
    }

    fn add_export(&mut self, node: Node<'_>, source: &str) {
        if let Some(declaration) = node.child_by_field_name("declaration") {
            self.add_statement(declaration, source, true);
            return;
        }
        let (Some(specifier), Some(clause)) = (
            node.child_by_field_name("source").map(|s| string_value(s, source)),
            named_child_of_kind(node, "export_clause"),
        ) else {
            return;
        };
        let mut cursor = clause.walk();
        for item in clause.named_children(&mut cursor).filter(|n| n.kind() == "export_specifier") {
            let Some(name) = field_text(item, "name", source) else { continue };
            let exported = field_text(item, "alias", source).unwrap_or_else(|| name.clone());
            self.reexports.insert(exported, ImportRef { specifier: specifier.clone(), imported: name });
        }
    }

    fn add_interface(&mut self, node: Node<'_>, source: &str) {
        let (Some(name), Some(body)) = (field_text(node, "name", source), node.child_by_field_name("body")) else {
            return;
        };
        let mut object = object_members(body, source);
        if let Some(clause) = named_child_of_kind(node, "extends_type_clause") {
            let mut cursor = clause.walk();
            for base in clause.children_by_field_name("type", &mut cursor) {
                if let TypeRef::Named(base) = classify(base, source) {
                    object.extends.push(base);
                }
            }
        }
        self.types.insert(name, TypeRef::Object(Arc::new(object)));
    }

    fn add_exported_variables(&mut self, node: Node<'_>, source: &str) {
        let mut cursor = node.walk();
        for declarator in node.named_children(&mut cursor).filter(|n| n.kind() == "variable_declarator") {
            let Some(name) = field_text(declarator, "name", source) else { continue };
            let annotated = declarator
                .child_by_field_name("type")
                .is_some_and(|t| matches!(classify(t, source), TypeRef::Func { promise: true }));
            let value = declarator.child_by_field_name("value");
            let valued = value.is_some_and(|v| {
                matches!(v.kind(), "arrow_function" | "function_expression" | "function")
                    && declares_promise(v, source)
            });
            if annotated || valued {
                self.promise_fns.insert(name);
            }
        }
    }

    fn add_import(&mut self, node: Node<'_>, source: &str) {
        let (Some(source_node), Some(clause)) = (
            node.child_by_field_name("source"),
            named_child_of_kind(node, "import_clause"),
        ) else {
            return;
        };
        let specifier = string_value(source_node, source);
        let mut cursor = clause.walk();
        for part in clause.named_children(&mut cursor) {
            match part.kind() {
                "named_imports" => self.add_named_imports(part, &specifier, source),
                "namespace_import" => {
                    if let Some(local) = part.named_child(0) {
                        let import = ImportRef { specifier: specifier.clone(), imported: "*".into() };
                        self.imports.insert(text(local, source).to_string(), import);
                    }
                }
                _ => {}
            }
        }
    }

    fn add_named_imports(&mut self, node: Node<'_>, specifier: &str, source: &str) {
        let mut cursor = node.walk();
        for item in node.named_children(&mut cursor).filter(|n| n.kind() == "import_specifier") {
            let Some(imported) = field_text(item, "name", source) else { continue };
            let local = field_text(item, "alias", source).unwrap_or_else(|| imported.clone());
            self.imports.insert(local, ImportRef { specifier: specifier.to_string(), imported });
        }
    }
}

impl<'a> Types<'a> {
    pub fn new(local: Arc<ModuleSummary>, resolver: Option<&'a dyn ModuleResolver>) -> Self {
        Types { local, resolver }
    }

    /// Wraps a type written in the current file.
    pub fn local(&self, ty: TypeRef) -> Typed {
        Typed { ty, module: self.local.clone() }
    }

    /// Classifies a type annotation or type node written in the current file.
    pub fn from_node(&self, node: Node<'_>, source: &str) -> Typed {
        self.local(classify(node, source))
    }

    /// Whether `local_name` is imported from another file that exports it as an `async` or
    /// `Promise`-returning function.
    pub fn imported_fn_returns_promise(&self, local_name: &str) -> bool {
        let Some(import) = self.local.imports.get(local_name) else { return false };
        self.find_function(&self.local, import, 0)
    }

    /// Whether member `member` of namespace import `namespace` is such a function.
    pub fn namespace_fn_returns_promise(&self, namespace: &str, member: &str) -> bool {
        let Some(import) = self.local.imports.get(namespace).filter(|i| i.imported == "*") else {
            return false;
        };
        self.load(&self.local, import)
            .is_some_and(|module| self.exports_promise_fn(&module, member, 0))
    }

    fn find_function(&self, from: &ModuleSummary, import: &ImportRef, depth: usize) -> bool {
        if import.imported == "*" || depth >= MAX_DEPTH {
            return false;
        }
        self.load(from, import)
            .is_some_and(|module| self.exports_promise_fn(&module, &import.imported, depth + 1))
    }

    fn exports_promise_fn(&self, module: &ModuleSummary, name: &str, depth: usize) -> bool {
        module.promise_fns.contains(name)
            || module
                .reexports
                .get(name)
                .is_some_and(|re| self.find_function(module, re, depth))
    }

    fn load(&self, from: &ModuleSummary, import: &ImportRef) -> Option<Arc<ModuleSummary>> {
        self.resolver?.summary(from.path.as_deref()?, &import.specifier)
    }

    /// Follows `Named` references until an object, function or unknown type is reached.
    pub fn resolve(&self, typed: &Typed) -> Typed {
        self.resolve_at(typed, 0)
    }

    fn resolve_at(&self, typed: &Typed, depth: usize) -> Typed {
        let TypeRef::Named(name) = &typed.ty else { return typed.clone() };
        match (depth < MAX_DEPTH).then(|| self.lookup(&typed.module, name, depth)).flatten() {
            Some(next) => self.resolve_at(&next, depth + 1),
            None => Typed { ty: TypeRef::Other, module: typed.module.clone() },
        }
    }

    /// The declaration behind `name` as seen from `module`: its own types, then imports,
    /// then re-exports.
    fn lookup(&self, module: &Arc<ModuleSummary>, name: &str, depth: usize) -> Option<Typed> {
        if depth >= MAX_DEPTH {
            return None;
        }
        if let Some(ty) = module.types.get(name) {
            return Some(Typed { ty: ty.clone(), module: module.clone() });
        }
        let import = module.imports.get(name).or_else(|| module.reexports.get(name))?;
        if import.imported == "*" {
            return None;
        }
        let remote = self.load(module, import)?;
        self.lookup(&remote, &import.imported, depth + 1)
    }

    /// The declared type of property `name` on `typed`, searching `extends` bases.
    pub fn member(&self, typed: &Typed, name: &str) -> Option<Typed> {
        self.member_at(typed, name, 0)
    }

    fn member_at(&self, typed: &Typed, name: &str, depth: usize) -> Option<Typed> {
        if depth >= MAX_DEPTH {
            return None;
        }
        let resolved = self.resolve(typed);
        let TypeRef::Object(object) = &resolved.ty else { return None };
        if let Some(ty) = object.members.get(name) {
            return Some(Typed { ty: ty.clone(), module: resolved.module.clone() });
        }
        object.extends.iter().find_map(|base| {
            let base = Typed { ty: TypeRef::Named(base.clone()), module: resolved.module.clone() };
            self.member_at(&base, name, depth + 1)
        })
    }

    /// Whether `typed` resolves to a function type returning a promise.
    pub fn is_promise_fn(&self, typed: &Typed) -> bool {
        matches!(self.resolve(typed).ty, TypeRef::Func { promise: true })
    }
}

fn text<'s>(node: Node<'_>, source: &'s str) -> &'s str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

fn field_text(node: Node<'_>, field: &str, source: &str) -> Option<String> {
    let value = text(node.child_by_field_name(field)?, source);
    Some(value.trim_matches(['"', '\'']).to_string())
}

fn string_value(node: Node<'_>, source: &str) -> String {
    text(node, source).trim_matches(['"', '\'']).to_string()
}

fn named_child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find(|n| n.kind() == kind)
}

fn is_async(function: Node<'_>) -> bool {
    let mut cursor = function.walk();
    function.children(&mut cursor).any(|c| c.kind() == "async")
}

/// Whether a type node is `Promise<...>`, possibly parenthesized or inside a union.
pub(crate) fn is_promise_type(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "type_annotation" | "parenthesized_type" => {
            node.named_child(0).is_some_and(|n| is_promise_type(n, source))
        }
        "union_type" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).any(|n| is_promise_type(n, source))
        }
        "generic_type" => node.child_by_field_name("name").is_some_and(|n| text(n, source) == "Promise"),
        _ => false,
    }
}

/// Whether a function-like node is `async` or declares a `Promise<...>` return type.
pub(crate) fn declares_promise(function: Node<'_>, source: &str) -> bool {
    is_async(function)
        || function.child_by_field_name("return_type").is_some_and(|t| is_promise_type(t, source))
}

/// Classifies a type node (or a `type_annotation` wrapping one).
fn classify(node: Node<'_>, source: &str) -> TypeRef {
    match node.kind() {
        "type_annotation" | "parenthesized_type" => {
            node.named_child(0).map_or(TypeRef::Other, |n| classify(n, source))
        }
        "type_identifier" => TypeRef::Named(text(node, source).to_string()),
        "generic_type" => match node.child_by_field_name("name") {
            Some(name) if name.kind() == "type_identifier" && text(name, source) != "Promise" => {
                TypeRef::Named(text(name, source).to_string())
            }
            _ => TypeRef::Other,
        },
        "function_type" => TypeRef::Func {
            promise: node.child_by_field_name("return_type").is_some_and(|t| is_promise_type(t, source)),
        },
        "object_type" => TypeRef::Object(Arc::new(object_members(node, source))),
        "union_type" => classify_union(node, source),
        _ => TypeRef::Other,
    }
}

/// `T | null | undefined` is `T`; a union with a promise-returning function counts as one.
fn classify_union(node: Node<'_>, source: &str) -> TypeRef {
    let mut cursor = node.walk();
    let mut kinds: Vec<TypeRef> = node
        .named_children(&mut cursor)
        .filter(|n| !matches!(text(*n, source), "null" | "undefined"))
        .map(|n| classify(n, source))
        .collect();
    if let Some(func) = kinds.iter().find(|k| matches!(k, TypeRef::Func { promise: true })) {
        return func.clone();
    }
    if kinds.len() == 1 { kinds.remove(0) } else { TypeRef::Other }
}

/// Members of an `object_type` or `interface_body`.
fn object_members(node: Node<'_>, source: &str) -> ObjectType {
    let mut object = ObjectType::default();
    let mut cursor = node.walk();
    for member in node.named_children(&mut cursor) {
        let Some(name) = field_text(member, "name", source) else { continue };
        match member.kind() {
            "property_signature" => {
                let ty = member.child_by_field_name("type").map_or(TypeRef::Other, |t| classify(t, source));
                object.members.insert(name, ty);
            }
            "method_signature" => {
                let promise = member.child_by_field_name("return_type").is_some_and(|t| is_promise_type(t, source));
                object.members.insert(name, TypeRef::Func { promise });
            }
            _ => {}
        }
    }
    object
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(source: &str) -> ModuleSummary {
        ModuleSummary::from_tree(&Language::TypeScript.parse(source).unwrap(), source, None)
    }

    fn object(summary: &ModuleSummary, name: &str) -> Arc<ObjectType> {
        match summary.types.get(name) {
            Some(TypeRef::Object(object)) => object.clone(),
            other => panic!("{name} is not an object: {other:?}"),
        }
    }

    #[test]
    fn alias_with_promise_returning_function_member() {
        let s = summary("export type P = { onSave: (l: L) => void | Promise<void>; sync: () => void };");
        let p = object(&s, "P");
        assert!(matches!(p.members["onSave"], TypeRef::Func { promise: true }));
        assert!(matches!(p.members["sync"], TypeRef::Func { promise: false }));
    }

    #[test]
    fn interface_methods_and_extends() {
        let s = summary("interface A extends B, C<X> { load(): Promise<void>; plain(): void; n: number }");
        let a = object(&s, "A");
        assert!(matches!(a.members["load"], TypeRef::Func { promise: true }));
        assert!(matches!(a.members["plain"], TypeRef::Func { promise: false }));
        assert!(matches!(a.members["n"], TypeRef::Other));
        assert_eq!(a.extends, vec!["B", "C"]);
    }

    #[test]
    fn optional_and_nullable_members_unwrap_to_the_named_type() {
        let s = summary("type O = { a?: T; b: T | null | undefined; c: T | U };");
        let o = object(&s, "O");
        assert!(matches!(&o.members["a"], TypeRef::Named(n) if n == "T"));
        assert!(matches!(&o.members["b"], TypeRef::Named(n) if n == "T"));
        assert!(matches!(o.members["c"], TypeRef::Other));
    }

    #[test]
    fn inline_object_literal_is_an_object_type() {
        let s = summary("type W = { inner: { run: () => Promise<void> } };");
        let w = object(&s, "W");
        let TypeRef::Object(inner) = &w.members["inner"] else { panic!("not an object") };
        assert!(matches!(inner.members["run"], TypeRef::Func { promise: true }));
    }

    #[test]
    fn exported_promise_functions_only() {
        let s = summary(
            "export async function a() {}\nexport function b(): Promise<void> | null { return null }\n\
             export const c = async () => {};\nexport const d: () => Promise<void> = x;\n\
             export function e() {}\nasync function hidden() {}",
        );
        let mut names: Vec<_> = s.promise_fns.iter().cloned().collect();
        names.sort();
        assert_eq!(names, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn imports_keep_aliases_and_type_only_forms() {
        let s = summary(
            "import type { A, B as C } from \"./t\";\nimport { type D, e } from '../u';\n\
             import * as ns from \"./n\";\nimport def from \"./d\";\nexport { F as G } from \"./f\";",
        );
        let pairs = |name: &str| s.imports.get(name).map(|i| (i.specifier.clone(), i.imported.clone()));
        assert_eq!(pairs("A"), Some(("./t".into(), "A".into())));
        assert_eq!(pairs("C"), Some(("./t".into(), "B".into())));
        assert_eq!(pairs("D"), Some(("../u".into(), "D".into())));
        assert_eq!(pairs("e"), Some(("../u".into(), "e".into())));
        assert_eq!(pairs("ns"), Some(("./n".into(), "*".into())));
        assert_eq!(pairs("def"), None);
        assert_eq!(s.reexports["G"].imported, "F");
    }

    #[test]
    fn alias_cycles_terminate() {
        let s = Arc::new(summary("type A = B; type B = A;"));
        let types = Types::new(s, None);
        let typed = types.local(TypeRef::Named("A".into()));
        assert!(matches!(types.resolve(&typed).ty, TypeRef::Other));
    }
}
