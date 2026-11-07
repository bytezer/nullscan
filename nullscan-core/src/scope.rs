//! Owned lexical-scope model for TypeScript/TSX.
//!
//! The graph is built once per file and stores only plain data (names, byte offsets,
//! 1-based line/column, scope ids, binding kinds, and read/write counts). It deliberately
//! keeps no tree-sitter [`Node`] values, so rules can inspect it without tying the graph to
//! a particular parse-tree lifetime.
//!
//! The model is intentionally conservative: it is a *lexical* scope graph, not a control
//! flow or data-flow analysis. References resolve to the nearest binding by walking the
//! scope chain; unresolved names are globals and are ignored. Type-position identifiers
//! count as reads (so a type-only use keeps an import alive), but interfaces, type aliases,
//! and type parameters do not create value bindings.

use tree_sitter::{Node, Tree};

/// Index into [`ScopeGraph::scopes`].
pub type ScopeId = usize;

/// How a name was introduced into a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Var,
    Let,
    Const,
    Function,
    Class,
    Param,
    Import,
    CatchParam,
}

/// The kinds of scopes modelled. Module scopes include namespaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Function,
    Block,
    Catch,
    Class,
    Loop,
    Switch,
}

/// One lexical scope. `start_byte`/`end_byte` cover the construct that introduced it.
#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    pub start_byte: usize,
    pub end_byte: usize,
    /// Indices into [`ScopeGraph::bindings`], in declaration order.
    pub bindings: Vec<usize>,
}

/// One declared name.
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub kind: BindingKind,
    pub scope: ScopeId,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    pub reads: usize,
    pub writes: usize,
}

/// One name occurrence in a value or type position.
#[derive(Debug, Clone)]
pub struct Reference {
    pub name: String,
    pub scope: ScopeId,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    pub read: bool,
    pub write: bool,
    /// Whether the occurrence sits in a type context (`type_identifier`, a type
    /// annotation, type arguments, `typeof` in a type query, ...). A type-only use keeps
    /// an import alive but cannot hit a temporal dead zone, so `no-use-before-define`
    /// ignores it. `class A extends B` stays a value reference; `implements I` does not.
    pub is_type_position: bool,
    /// Filled in after the whole file has been walked, so forward references work.
    pub binding: Option<usize>,
}

/// One `let`/`const`/`var` declarator, used by `prefer-const`.
#[derive(Debug, Clone)]
pub struct Declarator {
    pub kind: BindingKind,
    pub scope: ScopeId,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line: usize,
    pub column: usize,
    pub has_initializer: bool,
    /// Indices into [`ScopeGraph::bindings`] for every name in this declarator.
    pub bindings: Vec<usize>,
}

/// The complete scope graph for one parsed file.
#[derive(Debug, Clone, Default)]
pub struct ScopeGraph {
    pub scopes: Vec<Scope>,
    pub bindings: Vec<Binding>,
    pub references: Vec<Reference>,
    pub declarators: Vec<Declarator>,
}

impl ScopeGraph {
    /// Builds the graph from a parsed tree and its source text.
    pub fn build(tree: &Tree, source: &str) -> Self {
        let mut builder = Builder::new(source);
        let module = builder.new_scope(ScopeKind::Module, None, tree.root_node());
        builder.visit(tree.root_node(), module);
        builder.finish()
    }

    /// Nearest enclosing function or module scope, used for `var` hoisting.
    fn nearest_function_scope(&self, scope: ScopeId) -> ScopeId {
        let mut current = scope;
        loop {
            match self.scopes[current].kind {
                ScopeKind::Function | ScopeKind::Module => return current,
                _ => match self.scopes[current].parent {
                    Some(parent) => current = parent,
                    None => return current,
                },
            }
        }
    }

    fn resolve_name(&self, name: &str, scope: ScopeId, offset: usize) -> Option<usize> {
        let mut current = Some(scope);
        while let Some(scope_id) = current {
            let candidates: Vec<usize> = self.scopes[scope_id]
                .bindings
                .iter()
                .copied()
                .filter(|&index| self.bindings[index].name == name)
                .collect();

            if !candidates.is_empty() {
                // Prefer the latest declaration at or before the reference. If every
                // candidate is later (a `let`/`const`/`class` in its temporal dead zone),
                // fall back to the earliest one so `no-use-before-define` can see it.
                let before = candidates
                    .iter()
                    .copied()
                    .filter(|&index| self.bindings[index].offset <= offset)
                    .max_by_key(|&index| self.bindings[index].offset);
                return before.or_else(|| {
                    candidates
                        .iter()
                        .copied()
                        .min_by_key(|&index| self.bindings[index].offset)
                });
            }

            current = self.scopes[scope_id].parent;
        }
        None
    }
}

struct Builder<'a> {
    source: &'a str,
    graph: ScopeGraph,
}

impl<'a> Builder<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            graph: ScopeGraph::default(),
        }
    }

    fn finish(mut self) -> ScopeGraph {
        for index in 0..self.graph.references.len() {
            let (name, scope, offset, read, write) = {
                let reference = &self.graph.references[index];
                (
                    reference.name.clone(),
                    reference.scope,
                    reference.offset,
                    reference.read,
                    reference.write,
                )
            };
            let resolved = self.graph.resolve_name(&name, scope, offset);
            self.graph.references[index].binding = resolved;
            if let Some(binding) = resolved {
                if read {
                    self.graph.bindings[binding].reads += 1;
                }
                if write {
                    self.graph.bindings[binding].writes += 1;
                }
            }
        }
        self.graph
    }

    fn text(&self, node: Node<'_>) -> &'a str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    fn new_scope(&mut self, kind: ScopeKind, parent: Option<ScopeId>, node: Node<'_>) -> ScopeId {
        let id = self.graph.scopes.len();
        self.graph.scopes.push(Scope {
            id,
            kind,
            parent,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            bindings: Vec::new(),
        });
        id
    }

    /// Declares the identifier-like node in `scope` and returns its binding index.
    fn declare_node(&mut self, node: Node<'_>, kind: BindingKind, scope: ScopeId) -> Option<usize> {
        let name = self.text(node);
        if name.is_empty() {
            return None;
        }
        let start = node.start_position();
        let index = self.graph.bindings.len();
        self.graph.bindings.push(Binding {
            name: name.to_string(),
            kind,
            scope,
            offset: node.start_byte(),
            line: start.row + 1,
            column: start.column + 1,
            reads: 0,
            writes: 0,
        });
        self.graph.scopes[scope].bindings.push(index);
        Some(index)
    }

    fn visit_children(&mut self, node: Node<'_>, scope: ScopeId) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, scope);
        }
    }

    fn visit(&mut self, node: Node<'_>, scope: ScopeId) {
        match node.kind() {
            "function_declaration" | "generator_function_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.declare_node(name, BindingKind::Function, scope);
                }
                self.visit_function(node, scope, false);
            }
            "function_signature" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.declare_node(name, BindingKind::Function, scope);
                }
                self.visit_function(node, scope, false);
            }
            "function_expression" | "generator_function" => {
                self.visit_function(node, scope, true);
            }
            "arrow_function" => self.visit_function(node, scope, false),
            "method_definition" | "abstract_method_signature" => {
                self.visit_method(node, scope, true);
            }
            "method_signature" => self.visit_method(node, scope, false),
            "class_declaration" | "abstract_class_declaration" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.declare_node(name, BindingKind::Class, scope);
                }
                self.visit_class(node, scope, false);
            }
            "class" => self.visit_class(node, scope, true),
            "interface_declaration" => {
                self.visit_type_declaration(node, scope);
            }
            "type_alias_declaration" => {
                self.visit_type_declaration(node, scope);
            }
            "enum_declaration" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if Some(child.id()) == node.child_by_field_name("name").map(|n| n.id()) {
                        continue;
                    }
                    self.visit(child, scope);
                }
            }
            "type_parameter" => {
                // The type parameter's own name is not a value reference.
                if let Some(constraint) = node.child_by_field_name("constraint") {
                    self.visit(constraint, scope);
                }
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value, scope);
                }
            }
            "infer_type" => {
                // `infer T` introduces a type-only binding; only its constraint is a use.
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() != "type_identifier" {
                        self.visit(child, scope);
                    }
                }
            }
            "internal_module" => {
                // Namespaces behave like module scopes: their top-level declarations are
                // not reported as unused locals.
                let namespace = self.new_scope(ScopeKind::Module, Some(scope), node);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, namespace);
                }
            }
            "statement_block" => {
                let block = self.new_scope(ScopeKind::Block, Some(scope), node);
                self.visit_children(node, block);
            }
            "catch_clause" => {
                let catch_scope = self.new_scope(ScopeKind::Catch, Some(scope), node);
                if let Some(parameter) = node.child_by_field_name("parameter") {
                    self.declare_pattern(parameter, BindingKind::CatchParam, catch_scope);
                }
                if let Some(type_annotation) = node.child_by_field_name("type") {
                    self.visit(type_annotation, catch_scope);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_body(body, catch_scope);
                }
            }
            "switch_statement" => {
                let switch_scope = self.new_scope(ScopeKind::Switch, Some(scope), node);
                self.visit_children(node, switch_scope);
            }
            "for_statement" | "while_statement" | "do_statement" => {
                let loop_scope = self.new_scope(ScopeKind::Loop, Some(scope), node);
                self.visit_loop(node, loop_scope);
            }
            "for_in_statement" => {
                let loop_scope = self.new_scope(ScopeKind::Loop, Some(scope), node);
                self.visit_for_in(node, loop_scope);
            }
            "import_statement" => self.visit_import(node, scope),
            "export_statement" => self.visit_export(node, scope),
            "variable_declaration" | "lexical_declaration" => {
                self.visit_variable_declaration(node, scope);
            }
            "variable_declarator" => self.visit_declarator(node, scope),
            "assignment_expression" | "augmented_assignment_expression" => {
                self.visit_assignment(node, scope);
            }
            "update_expression" => {
                if let Some(argument) = node.child_by_field_name("argument") {
                    self.write_target(argument, scope, true);
                }
            }
            "identifier" | "shorthand_property_identifier" | "type_identifier" => {
                self.add_reference(node, scope, true, false);
            }
            _ => self.visit_children(node, scope),
        }
    }

    fn visit_function(&mut self, node: Node<'_>, scope: ScopeId, bind_own_name: bool) {
        let function_scope = self.new_scope(ScopeKind::Function, Some(scope), node);
        if bind_own_name {
            if let Some(name) = node.child_by_field_name("name") {
                self.declare_node(name, BindingKind::Function, function_scope);
            }
        }
        self.visit_type_parameters(node, function_scope);
        if let Some(parameters) = node.child_by_field_name("parameters") {
            self.declare_parameters(parameters, function_scope, true);
        }
        if let Some(parameter) = node.child_by_field_name("parameter") {
            self.declare_pattern(parameter, BindingKind::Param, function_scope);
        }
        if let Some(return_type) = node.child_by_field_name("return_type") {
            self.visit(return_type, function_scope);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_body(body, function_scope);
        }
    }

    /// A function/catch body block belongs to the function/catch scope itself; only
    /// explicitly nested `{ ... }` blocks introduce another `Block` scope. This keeps
    /// parameters and top-level body declarations in the same scope, so a
    /// parameter/`let` conflict is a redeclaration rather than a shadow.
    fn visit_body(&mut self, body: Node<'_>, scope: ScopeId) {
        if body.kind() == "statement_block" {
            self.visit_children(body, scope);
        } else {
            self.visit(body, scope);
        }
    }

    fn visit_method(&mut self, node: Node<'_>, scope: ScopeId, bind_params: bool) {
        let function_scope = self.new_scope(ScopeKind::Function, Some(scope), node);
        if let Some(name) = node.child_by_field_name("name") {
            if name.kind() == "computed_property_name" {
                self.visit(name, function_scope);
            }
        }
        self.visit_type_parameters(node, function_scope);
        if let Some(parameters) = node.child_by_field_name("parameters") {
            self.declare_parameters(parameters, function_scope, bind_params);
        }
        if let Some(return_type) = node.child_by_field_name("return_type") {
            self.visit(return_type, function_scope);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_body(body, function_scope);
        }
    }

    fn visit_class(&mut self, node: Node<'_>, scope: ScopeId, bind_own_name: bool) {
        let class_scope = self.new_scope(ScopeKind::Class, Some(scope), node);
        if bind_own_name {
            if let Some(name) = node.child_by_field_name("name") {
                self.declare_node(name, BindingKind::Class, class_scope);
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if Some(child.id()) == node.child_by_field_name("name").map(|n| n.id()) {
                continue;
            }
            // Heritage is evaluated in the enclosing scope, not the class body: `extends`
            // reads a value there and `implements` a type there. Only the body and the
            // class's own name belong to the class scope.
            if matches!(
                child.kind(),
                "class_heritage" | "extends_clause" | "implements_clause"
            ) {
                self.visit(child, scope);
            } else {
                self.visit(child, class_scope);
            }
        }
    }

    fn visit_type_declaration(&mut self, node: Node<'_>, scope: ScopeId) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if Some(child.id()) == node.child_by_field_name("name").map(|n| n.id()) {
                continue;
            }
            self.visit(child, scope);
        }
    }

    fn visit_type_parameters(&mut self, node: Node<'_>, scope: ScopeId) {
        if let Some(type_parameters) = node.child_by_field_name("type_parameters") {
            self.visit(type_parameters, scope);
        }
    }

    fn declare_parameters(&mut self, parameters: Node<'_>, scope: ScopeId, bind: bool) {
        let mut cursor = parameters.walk();
        for parameter in parameters.named_children(&mut cursor) {
            match parameter.kind() {
                "required_parameter" | "optional_parameter" => {
                    if bind {
                        if let Some(pattern) = parameter
                            .child_by_field_name("pattern")
                            .or_else(|| parameter.child_by_field_name("name"))
                        {
                            self.declare_pattern(pattern, BindingKind::Param, scope);
                        }
                    }
                    if let Some(type_annotation) = parameter.child_by_field_name("type") {
                        self.visit(type_annotation, scope);
                    }
                    if let Some(value) = parameter.child_by_field_name("value") {
                        self.visit(value, scope);
                    }
                }
                "identifier" => {
                    if bind {
                        self.declare_node(parameter, BindingKind::Param, scope);
                    }
                }
                _ => {
                    if bind {
                        self.declare_pattern(parameter, BindingKind::Param, scope);
                    } else {
                        self.visit(parameter, scope);
                    }
                }
            }
        }
    }

    fn visit_loop(&mut self, node: Node<'_>, scope: ScopeId) {
        if let Some(initializer) = node.child_by_field_name("initializer") {
            self.visit(initializer, scope);
        }
        let mut cursor = node.walk();
        for condition in node.children_by_field_name("condition", &mut cursor) {
            self.visit(condition, scope);
        }
        if let Some(increment) = node.child_by_field_name("increment") {
            self.visit(increment, scope);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit(body, scope);
        }
    }

    fn visit_for_in(&mut self, node: Node<'_>, scope: ScopeId) {
        let left = node.child_by_field_name("left");
        if let Some(kind_node) = node.child_by_field_name("kind") {
            let kind = match self.text(kind_node) {
                "var" => BindingKind::Var,
                "let" => BindingKind::Let,
                _ => BindingKind::Const,
            };
            if let Some(left) = left {
                let binding_scope = if kind == BindingKind::Var {
                    self.graph.nearest_function_scope(scope)
                } else {
                    scope
                };
                self.declare_pattern(left, kind, binding_scope);
            }
        } else if let Some(left) = left {
            self.write_pattern(left, scope);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit(right, scope);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit(body, scope);
        }
    }

    fn visit_import(&mut self, node: Node<'_>, scope: ScopeId) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "import_clause" => {
                    let mut clause_cursor = child.walk();
                    for clause_child in child.named_children(&mut clause_cursor) {
                        match clause_child.kind() {
                            "identifier" => {
                                self.declare_node(clause_child, BindingKind::Import, scope);
                            }
                            "namespace_import" => {
                                let mut namespace_cursor = clause_child.walk();
                                for name in clause_child.named_children(&mut namespace_cursor) {
                                    if name.kind() == "identifier" {
                                        self.declare_node(name, BindingKind::Import, scope);
                                    }
                                }
                            }
                            "named_imports" => {
                                let mut imports_cursor = clause_child.walk();
                                for specifier in clause_child.named_children(&mut imports_cursor) {
                                    if specifier.kind() != "import_specifier" {
                                        continue;
                                    }
                                    let local = specifier
                                        .child_by_field_name("alias")
                                        .or_else(|| specifier.child_by_field_name("name"));
                                    if let Some(local) = local {
                                        self.declare_node(local, BindingKind::Import, scope);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "import_require_clause" => {
                    let mut require_cursor = child.walk();
                    for name in child.named_children(&mut require_cursor) {
                        if name.kind() == "identifier" {
                            self.declare_node(name, BindingKind::Import, scope);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_export(&mut self, node: Node<'_>, scope: ScopeId) {
        if let Some(declaration) = node.child_by_field_name("declaration") {
            self.visit(declaration, scope);
        }
        if let Some(value) = node.child_by_field_name("value") {
            self.visit(value, scope);
        }
        // `export { a, b }` and `export * as ns from` only contain specifier names; they
        // are not references and are intentionally skipped.
    }

    fn visit_variable_declaration(&mut self, node: Node<'_>, scope: ScopeId) {
        let mut kind = BindingKind::Let;
        let mut kind_cursor = node.walk();
        for child in node.children(&mut kind_cursor) {
            match self.text(child) {
                "var" => {
                    kind = BindingKind::Var;
                    break;
                }
                "const" => {
                    kind = BindingKind::Const;
                    break;
                }
                "let" => break,
                _ => {}
            }
        }
        let mut cursor = node.walk();
        for declarator in node.named_children(&mut cursor) {
            if declarator.kind() != "variable_declarator" {
                continue;
            }
            let name = declarator.child_by_field_name("name");
            let value = declarator.child_by_field_name("value");
            let has_initializer = value.is_some();
            let binding_scope = if kind == BindingKind::Var {
                self.graph.nearest_function_scope(scope)
            } else {
                scope
            };
            let bindings = match name {
                Some(name) => self.declare_pattern(name, kind, binding_scope),
                None => Vec::new(),
            };
            let start = declarator.start_position();
            self.graph.declarators.push(Declarator {
                kind,
                scope: binding_scope,
                start_byte: declarator.start_byte(),
                end_byte: declarator.end_byte(),
                line: start.row + 1,
                column: start.column + 1,
                has_initializer,
                bindings,
            });
            if let Some(type_annotation) = declarator.child_by_field_name("type") {
                self.visit(type_annotation, scope);
            }
            if let Some(value) = value {
                self.visit(value, scope);
            }
        }
    }

    fn visit_declarator(&mut self, node: Node<'_>, scope: ScopeId) {
        if let Some(name) = node.child_by_field_name("name") {
            self.declare_pattern(name, BindingKind::Let, scope);
        }
        if let Some(value) = node.child_by_field_name("value") {
            self.visit(value, scope);
        }
    }

    fn visit_assignment(&mut self, node: Node<'_>, scope: ScopeId) {
        let augmented = node.kind() == "augmented_assignment_expression";
        if let Some(left) = node.child_by_field_name("left") {
            self.write_target(left, scope, augmented);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit(right, scope);
        }
    }

    /// Records a binding pattern. Default values and computed keys are still traversed as
    /// expressions, but every name in the pattern becomes a declaration.
    fn declare_pattern(&mut self, node: Node<'_>, kind: BindingKind, scope: ScopeId) -> Vec<usize> {
        let mut out = Vec::new();
        match node.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                if let Some(index) = self.declare_node(node, kind, scope) {
                    out.push(index);
                }
            }
            "object_pattern" | "array_pattern" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    out.extend(self.declare_pattern(child, kind, scope));
                }
            }
            "pair_pattern" => {
                if let Some(key) = node.child_by_field_name("key") {
                    if key.kind() == "computed_property_name" {
                        self.visit(key, scope);
                    }
                }
                if let Some(value) = node.child_by_field_name("value") {
                    out.extend(self.declare_pattern(value, kind, scope));
                }
            }
            "rest_pattern" => {
                if let Some(child) = node.named_child(0) {
                    out.extend(self.declare_pattern(child, kind, scope));
                }
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(left) = node.child_by_field_name("left") {
                    out.extend(self.declare_pattern(left, kind, scope));
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right, scope);
                }
            }
            _ => {}
        }
        out
    }

    fn write_pattern(&mut self, node: Node<'_>, scope: ScopeId) {
        match node.kind() {
            "identifier" | "shorthand_property_identifier_pattern" => {
                self.add_reference(node, scope, false, true);
            }
            "object_pattern" | "array_pattern" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.write_pattern(child, scope);
                }
            }
            "pair_pattern" => {
                if let Some(key) = node.child_by_field_name("key") {
                    if key.kind() == "computed_property_name" {
                        self.visit(key, scope);
                    }
                }
                if let Some(value) = node.child_by_field_name("value") {
                    self.write_pattern(value, scope);
                }
            }
            "rest_pattern" => {
                if let Some(child) = node.named_child(0) {
                    self.write_pattern(child, scope);
                }
            }
            "assignment_pattern" | "object_assignment_pattern" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.write_pattern(left, scope);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right, scope);
                }
            }
            "member_expression" | "subscript_expression" | "parenthesized_expression" => {
                self.visit(node, scope);
            }
            _ => {}
        }
    }

    fn write_target(&mut self, node: Node<'_>, scope: ScopeId, also_read: bool) {
        match node.kind() {
            "identifier" | "shorthand_property_identifier" => {
                self.add_reference(node, scope, also_read, true);
            }
            "object_pattern"
            | "array_pattern"
            | "pair_pattern"
            | "rest_pattern"
            | "assignment_pattern"
            | "object_assignment_pattern" => {
                self.write_pattern(node, scope);
            }
            _ => self.visit(node, scope),
        }
    }

    fn add_reference(&mut self, node: Node<'_>, scope: ScopeId, read: bool, write: bool) {
        let name = self.text(node);
        if name.is_empty() {
            return;
        }
        let start = node.start_position();
        self.graph.references.push(Reference {
            name: name.to_string(),
            scope,
            offset: node.start_byte(),
            line: start.row + 1,
            column: start.column + 1,
            read,
            write,
            is_type_position: is_type_position(node),
            binding: None,
        });
    }
}

/// Node kinds that put their descendants in a type context. The list is intentionally
/// broad: a missed kind only means `no-use-before-define` reports a type-only use, while
/// an extra kind can hide a genuine value use, so `extends_clause` is deliberately absent.
const TYPE_CONTEXT_KINDS: &[&str] = &[
    "type_annotation",
    "type_alias_declaration",
    "interface_declaration",
    "type_arguments",
    "type_parameters",
    "type_parameter",
    "type_query",
    "index_type_query",
    "predefined_type",
    "generic_type",
    "object_type",
    "union_type",
    "intersection_type",
    "function_type",
    "array_type",
    "tuple_type",
    "conditional_type",
    "mapped_type_clause",
    "infer_type",
    "type_predicate",
    "type_predicate_annotation",
    "asserts_annotation",
    "constructor_type",
    "literal_type",
    "parenthesized_type",
    "optional_type",
    "rest_type",
    "template_literal_type",
    "implements_clause",
];

/// Whether an identifier occurrence is type-only.
fn is_type_position(node: Node<'_>) -> bool {
    if node.kind() == "type_identifier" {
        return true;
    }
    let mut ancestor = node.parent();
    while let Some(current) = ancestor {
        if TYPE_CONTEXT_KINDS.contains(&current.kind()) {
            return true;
        }
        ancestor = current.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use crate::Language;

    use super::*;

    fn graph(source: &str) -> ScopeGraph {
        let tree = Language::TypeScript
            .parse(source)
            .expect("source should parse");
        ScopeGraph::build(&tree, source)
    }

    fn binding<'a>(graph: &'a ScopeGraph, name: &str) -> &'a Binding {
        graph
            .bindings
            .iter()
            .find(|binding| binding.name == name)
            .unwrap_or_else(|| panic!("no binding named '{name}'"))
    }

    fn scope_kind(graph: &ScopeGraph, binding: &Binding) -> ScopeKind {
        graph.scopes[binding.scope].kind
    }

    #[test]
    fn var_is_function_scoped_and_let_is_block_scoped() {
        let graph_with_block =
            graph("function f() { if (true) { var hoisted = 1; let block = 2; } }");
        assert_eq!(
            scope_kind(&graph_with_block, binding(&graph_with_block, "hoisted")),
            ScopeKind::Function
        );
        assert_eq!(
            scope_kind(&graph_with_block, binding(&graph_with_block, "block")),
            ScopeKind::Block
        );

        // A reference after the block still resolves to the function-scoped `var`.
        let graph = graph("function f() { if (true) { var hoisted = 1; } return hoisted; }");
        let reference = graph
            .references
            .iter()
            .find(|reference| reference.name == "hoisted")
            .expect("reference to hoisted");
        assert!(reference.binding.is_some());
    }

    #[test]
    fn resolves_shadowed_names_to_the_nearest_binding() {
        let graph =
            graph("function f() { let value = 1; { let value = 2; use(value); } use(value); }");
        let value_bindings: Vec<_> = graph
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, binding)| binding.name == "value")
            .collect();
        assert_eq!(value_bindings.len(), 2);
        let (outer_index, outer) = value_bindings[0];
        let (inner_index, inner) = value_bindings[1];

        let inner_reference = graph
            .references
            .iter()
            .find(|reference| reference.name == "value" && reference.scope == inner.scope)
            .expect("inner reference");
        let outer_reference = graph
            .references
            .iter()
            .find(|reference| reference.name == "value" && reference.scope == outer.scope)
            .expect("outer reference");
        assert_eq!(inner_reference.binding, Some(inner_index));
        assert_eq!(outer_reference.binding, Some(outer_index));
    }

    #[test]
    fn forward_references_resolve_for_hoisted_and_non_hoisted_bindings() {
        let graph =
            graph("function f() { use(later); var later = 1; use(blocked); let blocked = 2; }");
        let later_index = graph
            .bindings
            .iter()
            .position(|binding| binding.name == "later")
            .expect("later binding");
        let blocked_index = graph
            .bindings
            .iter()
            .position(|binding| binding.name == "blocked")
            .expect("blocked binding");
        let later = &graph.bindings[later_index];
        let blocked = &graph.bindings[blocked_index];
        assert_eq!(later.kind, BindingKind::Var);
        assert_eq!(blocked.kind, BindingKind::Let);

        let earlier_later = graph
            .references
            .iter()
            .find(|reference| reference.name == "later")
            .expect("later reference");
        let earlier_blocked = graph
            .references
            .iter()
            .find(|reference| reference.name == "blocked")
            .expect("blocked reference");
        assert_eq!(earlier_later.binding, Some(later_index));
        assert_eq!(earlier_blocked.binding, Some(blocked_index));
        assert!(earlier_later.offset < later.offset);
        assert!(earlier_blocked.offset < blocked.offset);
    }

    #[test]
    fn parameters_bind_in_the_function_scope() {
        let graph = graph("function f(a, { b }: { b: number }) { return a + b; }");
        let a = binding(&graph, "a");
        let b = binding(&graph, "b");
        assert_eq!(a.kind, BindingKind::Param);
        assert_eq!(b.kind, BindingKind::Param);
        assert_eq!(scope_kind(&graph, a), ScopeKind::Function);
        assert!(a.reads > 0 && b.reads > 0);
    }

    #[test]
    fn imports_bind_and_type_uses_count_as_reads() {
        let graph = graph("import def, { a as b, c } from \"./m\";\nconst value: b = def; use(c);");
        let def = binding(&graph, "def");
        let b = binding(&graph, "b");
        let c = binding(&graph, "c");
        assert!(matches!(def.kind, BindingKind::Import));
        assert!(matches!(b.kind, BindingKind::Import));
        assert!(matches!(c.kind, BindingKind::Import));
        assert!(def.reads > 0);
        assert!(b.reads > 0, "type-only usage should count as a read");
        assert!(c.reads > 0);
    }

    #[test]
    fn destructuring_creates_each_binding() {
        let graph = graph(
            "const { a, b: c, d = fallback, ...rest } = obj;\nconst [e, , ...f] = arr;\nuse(a, c, d, rest, e, f);",
        );
        for name in ["a", "c", "d", "rest", "e", "f"] {
            assert!(matches!(binding(&graph, name).kind, BindingKind::Const));
        }
        assert_eq!(graph.bindings.len(), 6);
    }

    #[test]
    fn loop_and_switch_bindings_live_in_their_own_scopes() {
        let for_in_graph = graph("function f(items) { for (const item of items) { use(item); } }");
        let item = binding(&for_in_graph, "item");
        assert_eq!(item.kind, BindingKind::Const);
        assert_eq!(scope_kind(&for_in_graph, item), ScopeKind::Loop);
        assert!(item.reads > 0);

        let for_graph = graph("function f() { for (let i = 0; i < 3; i++) { use(i); } }");
        let i = binding(&for_graph, "i");
        assert_eq!(i.kind, BindingKind::Let);
        assert_eq!(scope_kind(&for_graph, i), ScopeKind::Loop);
        assert!(i.reads > 0 && i.writes > 0);

        let switch_graph =
            graph("function f(x) { switch (x) { case 1: let local = 1; use(local); break; } }");
        let local = binding(&switch_graph, "local");
        assert_eq!(scope_kind(&switch_graph, local), ScopeKind::Switch);
        assert!(local.reads > 0);
    }

    #[test]
    fn tracks_reads_and_writes_separately() {
        let graph =
            graph("function f() { let value = 1; value = 2; value += 3; value++; return value; }");
        let value = binding(&graph, "value");
        assert_eq!(value.writes, 3);
        assert_eq!(value.reads, 3);
    }

    #[test]
    fn leaves_unresolved_globals_unbound() {
        let graph = graph("missing();\nglobal = 1;");
        assert!(graph.bindings.is_empty());
        assert_eq!(graph.references.len(), 2);
        assert!(
            graph
                .references
                .iter()
                .all(|reference| reference.binding.is_none())
        );
    }

    #[test]
    fn type_declarations_do_not_create_value_bindings() {
        let graph = graph("interface I { value: string }\ntype T = I;\nconst x: T = 1;");
        assert!(graph.bindings.iter().all(|binding| binding.name != "I"));
        assert!(graph.bindings.iter().all(|binding| binding.name != "T"));
        assert!(binding(&graph, "x").kind == BindingKind::Const);
    }

    #[test]
    fn marks_type_positions_but_keeps_extends_a_value_reference() {
        let graph = graph(
            "interface I { k: typeof K }\nconst x: Foo = 1;\nclass A extends B {}\nclass C implements I {}",
        );
        let is_type = |name: &str| {
            graph
                .references
                .iter()
                .find(|reference| reference.name == name)
                .unwrap_or_else(|| panic!("no reference named '{name}'"))
                .is_type_position
        };
        assert!(is_type("K"), "typeof K in an interface is a type query");
        assert!(is_type("Foo"), "a type annotation is a type position");
        assert!(is_type("I"), "implements is a type position");
        assert!(!is_type("B"), "extends evaluates a runtime value");
    }
}
