# Rule catalog

Every entry in `RULES` (`nullscan-core/src/rule.rs`) has one row here. The registry is
the source of truth for IDs and descriptions; this document adds the human-facing
category and the closest upstream catalog equivalent.

Upstream identifiers below are SonarJS rule keys (the `S####` IDs used by
`eslint-plugin-sonarjs` and SonarQube) and/or ESLint rule names. Where no reliable
equivalent exists, the cell says so rather than guessing an ID. Category values are
`bug`, `code smell`, `metric`, `duplication`, or `security`.

| nullscan ID | Category | Source / equivalent catalog rule | Implementation status |
|---|---|---|---|
| `no-console` | code smell | SonarJS S106 `no-console`; ESLint `no-console` | Implemented (file-scoped) |
| `no-debugger` | code smell | SonarJS S1525 `no-debugger`; ESLint `no-debugger` | Implemented (file-scoped) |
| `no-var` | code smell | SonarJS S3504 `no-var`; ESLint `no-var` | Implemented (file-scoped) |
| `eqeqeq` | bug | SonarJS S1440 `eqeqeq`; ESLint `eqeqeq` | Implemented (file-scoped) |
| `no-explicit-any` | code smell | SonarJS S4204 `no-explicit-any`; typescript-eslint `no-explicit-any` | Implemented (file-scoped) |
| `no-empty-catch` | bug | SonarJS S2486 `no-ignored-exceptions`; ESLint `no-empty` (catch blocks) | Implemented (file-scoped) |
| `cognitive-complexity` | metric | SonarJS S3776 `cognitive-complexity`; no ESLint core equivalent (ESLint `complexity` is cyclomatic) | Implemented (file-scoped) |
| `no-duplicate-string` | code smell | SonarJS S1192 `no-duplicate-string` | Implemented (file-scoped) |
| `no-identical-conditions` | bug | SonarJS S1862 `no-identical-conditions`; ESLint `no-dupe-else-if` | Implemented (file-scoped) |
| `no-redundant-boolean` | code smell | SonarJS S1125 `no-redundant-boolean` | Implemented (file-scoped) |
| `prefer-immediate-return` | code smell | SonarJS S1488 `prefer-immediate-return` | Implemented (file-scoped) |
| `no-collapsible-if` | code smell | SonarJS S1066 `no-collapsible-if` | Implemented (file-scoped) |
| `no-identical-expressions` | bug | SonarJS S1764 `no-identical-expressions`; ESLint `no-self-compare` (subset) | Implemented (file-scoped) |
| `no-duplicated-branches` | bug | SonarJS S1871 `no-duplicated-branches` | Implemented (file-scoped) |
| `no-all-duplicated-branches` | bug | SonarJS S3923 `no-all-duplicated-branches` | Implemented (file-scoped) |
| `no-useless-catch` | code smell | SonarJS S2737 `no-useless-catch`; ESLint `no-useless-catch` | Implemented (file-scoped) |
| `no-redundant-jump` | code smell | SonarJS S3626 `no-redundant-jump`; ESLint `no-useless-return` (return only) | Implemented (file-scoped) |
| `prefer-single-boolean-return` | code smell | SonarJS S1126 `prefer-single-boolean-return` | Implemented (file-scoped) |
| `no-nested-switch` | code smell | SonarJS S1821 `no-nested-switch` | Implemented (file-scoped) |
| `no-nested-template-literals` | code smell | SonarJS S4624 `no-nested-template-literals` | Implemented (file-scoped) |
| `no-small-switch` | code smell | SonarJS S1301 `no-small-switch` | Implemented (file-scoped; `no-small-switch-min-cases`) |
| `max-switch-cases` | code smell | SonarJS S1479 `max-switch-cases` | Implemented (file-scoped; `max-switch-cases`) |
| `no-same-line-conditional` | code smell | SonarJS S3972 `no-same-line-conditional` | Implemented (file-scoped) |
| `non-existent-operator` | bug | SonarJS S2757 `non-existent-operator` | Implemented (file-scoped) |
| `no-self-assign` | bug | SonarJS S1656 `no-self-assign`; ESLint `no-self-assign` | Implemented (file-scoped) |
| `no-constant-condition` | bug | ESLint `no-constant-condition`; no SonarJS equivalent | Implemented (file-scoped; `if` and ternary only) |
| `use-isnan` | bug | SonarJS S2688 `use-isnan`; ESLint `use-isnan` | Implemented (file-scoped) |
| `no-array-constructor` | code smell | SonarJS S1528 `array-constructor`; ESLint/typescript-eslint `no-array-constructor` | Implemented (file-scoped) |
| `no-new-wrappers` | bug | SonarJS S1533 `no-primitive-wrappers`; ESLint `no-new-wrappers` | Implemented (file-scoped) |
| `no-duplicate-imports` | code smell | SonarJS S3863 `no-duplicate-imports` (via eslint-plugin-import `no-duplicates`); ESLint `no-duplicate-imports` | Implemented (file-scoped) |
| `no-unused-vars` | code smell | SonarJS S1481 (unused local variables); ESLint/typescript-eslint `no-unused-vars` | Implemented (file-scoped; scope analysis) |
| `no-shadow` | code smell | SonarJS S1117 `no-shadow`; typescript-eslint `no-shadow`; ESLint `no-shadow` | Implemented (file-scoped; scope analysis) |
| `no-use-before-define` | bug | SonarJS S3827 (variables should be defined before being used); ESLint/typescript-eslint `no-use-before-define` | Implemented (file-scoped; same-scope, scope analysis) |
| `prefer-const` | code smell | SonarJS S3353 `prefer-const`; ESLint `prefer-const` | Implemented (file-scoped; scope analysis) |
| `no-redeclare` | code smell | SonarJS S2814 `no-redeclare`; ESLint/typescript-eslint `no-redeclare` | Implemented (file-scoped; scope analysis) |
| `no-unreachable` | bug | SonarJS S1763 `no-unreachable`; ESLint `no-unreachable` | Implemented (file-scoped; `return`/`throw`/`break`/`continue`, terminating blocks and `if`/`else`; no `try`/`switch`/loop analysis) |
| `no-fallthrough` | bug | SonarJS S128 `no-fallthrough`; ESLint `no-fallthrough` | Implemented (file-scoped; non-final `case` only, empty cases allowed, comment opt-out) |
| `consistent-return` | bug | SonarJS S3801 `no-inconsistent-returns`; ESLint `consistent-return` | Implemented (file-scoped; path-insensitive) |
| `no-one-iteration-loop` | code smell | SonarJS S1751 `no-one-iteration-loop`; ESLint `no-unreachable-loop` | Implemented (file-scoped; top-level unconditional exits only) |
| `no-unused-collection` | code smell | SonarJS S4030 `no-unused-collection`; no direct ESLint core equivalent | Implemented (file-scoped; scope analysis; local array/object/`Map`/`Set` only) |
| `no-element-overwrite` | bug | SonarJS `no-element-overwrite`; no direct ESLint core equivalent | Implemented (file-scoped; adjacent statements only) |
| `no-ignored-return` | bug | SonarJS `no-ignored-return`; no direct ESLint core equivalent (ESLint `no-unused-expressions` is broader) | Implemented (file-scoped; fixed pure-function allowlist) |
| `no-gratuitous-expressions` | bug | SonarJS `no-gratuitous-expressions`; no direct ESLint core equivalent | Implemented (file-scoped; conditions only, identical/negated operands) |
| `no-eval` | security | ESLint `no-eval`; no direct SonarJS equivalent | Implemented (file-scoped; direct calls and common global/member spellings only) |
| `no-implied-eval` | security | ESLint `no-implied-eval`; no direct SonarJS equivalent | Implemented (file-scoped; string literals and substitution-free templates only) |
| `no-new-func` | security | ESLint `no-new-func`; no direct SonarJS equivalent | Implemented (file-scoped; bare and common global/member forms) |
| `no-script-url` | security | ESLint `no-script-url`; no direct SonarJS equivalent | Implemented (file-scoped; string and substitution-free template literals) |
| `hardcoded-secrets` | security | SonarJS S6418 (hard-coded secrets); no ESLint core equivalent | Implemented (file-scoped; name/value heuristic, not a secret scanner) |
| `duplicate-code` | duplication | SonarQube duplication metrics (`duplicated_blocks`); not a SonarJS rule | Implemented (project-scoped, CLI; executable logic only, JSX markup opt-in via `[duplicates].compare-markup`) |

### Conservative limits

These rules are syntax-only by design (no scope analysis, CFG, or type information):

- `no-identical-expressions` compares trimmed source text and excludes `+` and `*`, where
  repeating an operand is often intentional.
- `no-duplicated-branches` / `no-all-duplicated-branches` compare trimmed source text;
  empty bodies are ignored, and semantically equal but differently formatted bodies are
  treated as different. The all-identical shape is reported only by
  `no-all-duplicated-branches`.
- `no-useless-catch` only matches a body with exactly one `throw <caught-parameter>`.
- `no-redundant-jump` only matches a bare `return;` directly at the end of a function body
  and a bare `continue;` directly at the end of a loop body. `break` and valued returns
  are never reported.
- `prefer-single-boolean-return` only matches a direct `if`/`else` whose branches are
  literal `return true;` / `return false;`.
- `no-nested-switch` stops at a function boundary between the switches.
- `no-nested-template-literals` reports each nested template once.
- `no-same-line-conditional` only checks statement-list positions and never flags an
  `else if`.
- `non-existent-operator` is whitespace-sensitive by design (`=+` fires, `= +` does not).
- `no-self-assign` uses trimmed source-text equality, not symbol resolution.
- `no-constant-condition` checks `if` and ternary conditions only, so intentional
  `while (true)` loops are not reported.
- `use-isnan` recognizes the literal operand text `NaN` / `Number.NaN`, not aliases.
- `no-array-constructor` / `no-new-wrappers` only match bare `Array`, `String`, `Number`,
  and `Boolean` identifiers.
- `no-duplicate-imports` compares exact module source strings and ignores `export ... from`.
- `no-unreachable` only recognises `return`, `throw`, `break`, `continue`, a block whose last
  executing statement terminates, and an `if` whose two branches both terminate. `try`,
  `switch`, and loops are never treated as terminating, so code after them is not reported.
  A following hoisted function declaration is skipped, and the first real statement of a
  contiguous dead region is reported once.
- `no-fallthrough` checks only non-final, non-empty `case` bodies. A terminating last
  statement (see `no-unreachable`) suppresses it, and a comment between the body and the
  next label containing `fallthrough` or `falls through` marks the fallthrough as
  intentional. `default` is not checked as a falling case, and the last case is never
  reported.
- `consistent-return` is path-insensitive: it reports a function with at least one valued
  `return` plus either a bare `return;` or a body whose last executing statement does not
  terminate. Returns in nested functions are ignored; loops and `switch` are not proven
  terminating, so `while (true) { return x; }` can still be reported.
- `no-one-iteration-loop` only sees top-level `break`/`return`/`throw` before any top-level
  `continue` in the loop body. Exits inside `if`/`else`, blocks, nested loops, `switch`, or
  functions are ignored.
- `no-element-overwrite` compares only two adjacent `assignment_expression` statements in
  the same statement list and uses trimmed target text. Intervening statements, aliases,
  and control flow are not considered.
- `no-ignored-return` uses a fixed allowlist of pure global functions and static methods
  (`parseInt`, `parseFloat`, `isNaN`, `isFinite`, `Number`, `String`, `Boolean`,
  `encodeURI`/`decodeURI`/`encodeURIComponent`/`decodeURIComponent`, `Math.*`,
  `JSON.parse`/`stringify`, selected `Object.*`, and `Array.isArray`/`from`/`of`). It never
  infers purity for instance methods, and ignores `await`, `void`, and calls nested in
  larger expressions.
- `no-gratuitous-expressions` only inspects `if`, ternary, `while`, `do`/`while`, and `for`
  conditions, and only flags `&&`/`||` expressions whose operands are textually identical
  or where one is the `!` negation of the other. Broader constant-folding and always-true
  analysis is out of scope.
- `no-eval` reports only direct calls to bare `eval` or `.eval` on `window`, `global`,
  `globalThis`, `self`, or `this`. References that are not called (`const f = eval`),
  aliases, computed access (`window["eval"]`), and other receiver objects are ignored.
- `no-implied-eval` inspects only the first argument of bare or `window`/`global`/
  `globalThis` calls to `setTimeout`, `setInterval`, `setImmediate`, and `execScript`,
  and only when that argument is a string literal or a substitution-free template.
  Identifiers, functions, dynamic templates, aliases, and other objects are skipped.
- `no-new-func` reports `new Function(...)` and `Function(...)` (bare or on `window`/
  `global`/`globalThis`). Aliases, constructor references that are not called, computed
  access, and other objects are not resolved.
- `no-script-url` only checks string literals and substitution-free templates, comparing
  trimmed lowercased text against the `javascript:` prefix. Dynamic templates and comments
  are never reported.
- `hardcoded-secrets` is a heuristic, **not** a secret scanner. It matches a fixed list of
  secret-like names after removing `_`/`-` and lowercasing, and only a directly assigned
  string literal or substitution-free template of at least 8 trimmed characters. It
  excludes a fixed placeholder prefix list (`changeme`, `change_me`, `password`, `secret`,
  `token`, `example`, `test`, `dummy`, `placeholder`, `your_`, `xxx`, `***`) and values
  containing `${`. Computed values, concatenation, destructuring, class fields, computed
  property access (`obj["password"]`), aliases, and non-string expressions such as
  `process.env.X` are ignored. Expect false positives for fixtures/documentation and false
  negatives for secrets built at runtime or hidden behind other names.

### Scope-analysis limits

These rules use the per-file lexical scope graph (`nullscan-core/src/scope.rs`). It is not
a control-flow graph, data-flow analysis, or type checker:

- `no-unused-vars` reports unused imports anywhere and unused `let`/`const`/`var`
  bindings inside functions or nested blocks. Module-level variables, parameters,
  function/class declarations, and names starting with `_` are ignored. Destructured
  bindings are checked individually. Type-position uses count as reads, but export
  specifier names do not, so `import { a } ...; export { a };` can still be reported as
  unused.
- `no-shadow` reports a binding whose name exists in an enclosing lexical scope. Same-scope
  duplicates are intentionally left to `no-redeclare`. Parameters, catch parameters,
  imports, functions, classes, and `let`/`const`/`var` are covered.
- `no-use-before-define` only reports `let`/`const`/`class` references that appear earlier
  in the *same* lexical scope. References inside nested functions or blocks are not
  reported even if they might run sooner; `var` and function declarations are hoisted and
  never reported, and unresolved globals are ignored.
- `prefer-const` reports a `let` declarator with an initializer when its bindings have no
  recorded writes. Writes are lexical, not path-sensitive, so a write in dead code still
  suppresses the diagnostic (a deliberate false negative). Declarators without an
  initializer and `for ... of`/`for ... in` bindings are skipped; destructuring is reported
  once per declarator.
- `no-redeclare` reports the second binding with the same name in one scope. Because
  tree-sitter is error-tolerant, syntax-invalid redeclarations that a TypeScript compiler
  would reject can still produce a diagnostic.
- `no-unused-collection` reports a local `let`/`const`/`var` binding initialized to an array
  literal, object literal, `new Map(...)`, or `new Set(...)` when every resolved reference
  is a standalone mutation call (`push`, `set`, `add`, ...) or a direct index/property
  assignment. Any other reference counts as a read and suppresses it. Module-level
  bindings are skipped, and the rule is not type-aware, so an object whose own method is
  named like a collection mutator is treated the same way.

## Adding a rule

1. Implement it under `nullscan-core/src/lang/typescript/rules/` as
   `pub fn run(&FileContext<'_>) -> Vec<Diagnostic>`.
2. Register it exactly once in `RULES` in `nullscan-core/src/rule.rs` with its
   `RuleMeta`, `RuleScope`, `requires_scope`, and runner (project-scoped rules use
   `run: None` plus an orchestrator). Scope-dependent file rules set
   `requires_scope: true` and read `ctx.scopes`.
3. Add `nullscan-core/tests/fixtures/<rule-id>/valid.ts` (or `.tsx`) and
   `invalid.ts` (or `.tsx`). The fixture harness at `nullscan-core/tests/rules.rs`
   discovers the directory from the rule ID and asserts that `valid.*` is clean for
   that rule while `invalid.*` produces at least one diagnostic. Other rules may fire
   on a fixture; only the rule named by the directory is checked.
4. Run `cargo test --workspace`. The harness fails any file-scoped registry rule that
   has no valid/invalid fixture pair, so a new rule cannot land without fixtures.
5. Add or update the row in this table.

Project-scoped rules (`duplicate-code`) are skipped by the per-file fixture harness and
need their own orchestration test instead.
