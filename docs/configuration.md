# Configuration reference

Nullscan reads one `nullscan.toml` per run. Everything is optional: a missing file, or a
file that only sets a few keys, falls back to the defaults below.

- CLI: [CLI reference](./cli.md) for flags and exit codes.
- Rules: [RULES.md](../RULES.md) for the full id list and what each rule detects.
- JSON output: [JSON contract](./json.md).

## Where the file is read from

| Order | Source | When |
|---|---|---|
| 1 | `--config <path>` | Always wins. The file must exist and parse. |
| 2 | `./nullscan.toml` | Only when `--config` is absent **and** the file exists in the process working directory. |
| 3 | Built-in defaults | When neither of the above applies. |

Two details that surprise people:

- `./nullscan.toml` is resolved against the **process working directory**, not against the
  path you pass to nullscan. Running `nullscan packages/app/src` from the repository root
  reads `<repo>/nullscan.toml`; a `nullscan.toml` inside `packages/app` is not picked up
  unless you pass `--config packages/app/nullscan.toml`.
- Relative ignore patterns are resolved against the **directory that contains the config
  file**. With the built-in defaults (no file) that root is the current working directory.

The resolved path is reported as `config.path` in `--format json` (absolute, or `null` when
the defaults were used).

### In the editor

The language server follows the same rules with two differences:

- Without `nullscan.configPath`, each workspace folder resolves its **own**
  `folder/nullscan.toml`, so a multi-root workspace can use different rule sets per folder.
- With `--config <path>` (set by the extension's `nullscan.configPath`), that one file
  applies to every open document and every scanned folder; a missing or broken file is
  reported with `window/logMessage` and the server keeps running with the defaults.

A broken `nullscan.toml` is fatal for the CLI (exit `2`) but not for the server, which
degrades to the defaults so the editor stays usable.

## Full example

```toml
# Rule ids to turn off. Unknown ids are warned about but do not fail the run.
[rules]
disabled = ["no-console", "no-var"]

# Literal values that no-duplicate-string should not count, without the quotes.
[rules.no-duplicate-string]
ignore = ["transparent", "currentColor", "100%"]

[thresholds]
cognitive-complexity = 20        # default: 15
duplicate-code-min-tokens = 50   # default: 30
duplicate-string-threshold = 3   # default: 3
duplicate-string-min-length = 10 # default: 10
max-switch-cases = 10            # default: 10
no-small-switch-min-cases = 3    # default: 3

# Excluded from every analysis (file rules and duplicate detection).
[files]
ignore = ["tests/**", "**/*.test.ts", "src/generated/**"]

# Still analyzed for rules, but excluded from clone detection.
[duplicates]
ignore = ["tests/mocks/**", "**/__fixtures__/**"]
compare-markup = false           # default: false
```

## `[rules]`

### `[rules].disabled`

| Type | Default | Scope |
|---|---|---|
| array of rule ids (strings) | `[]` | every analysis |

Rules listed here are skipped. Ids are the ones in [RULES.md](../RULES.md) (for example
`no-console`, `eqeqeq`); they are matched exactly and case-sensitively.

- An unknown id prints `warning: <file> disables unknown rule '<id>'` and the run continues,
  so typos are visible without breaking CI.
- Disabling `duplicate-code` skips the cross-file clone pass entirely instead of computing
  and hiding it. This is the most expensive rule, so disabling it is the fastest way to
  shorten a scan. The duplication summary then reports zeroes.

### `[rules.no-duplicate-string]`

#### `[rules.no-duplicate-string].ignore`

| Type | Default | Scope |
|---|---|---|
| array of strings | `[]` | `no-duplicate-string` |

Literal contents that the rule skips entirely. Entries are written **without the
surrounding quotes** and are compared against the raw literal content with escape sequences
still encoded, so write them the way they appear in the source:

```toml
[rules.no-duplicate-string]
ignore = ["transparent", "currentColor", "100%"]
```

```ts
const a = "transparent"; // ignored, never counted
const b = 'transparent'; // ignored too: quotes are not part of the comparison
```

Matching is exact and case-sensitive (`transparent` does not ignore `Transparent`) and
there is no globbing. Ignored literals are dropped before counting, so they cannot push an
otherwise rare string over the threshold.

## `[thresholds]`

All thresholds are non-negative integers. A float, a negative number or a string is a
config error.

| Key | Default | Affects | Meaning |
|---|---|---|---|
| `cognitive-complexity` | `15` | `cognitive-complexity` | A function is reported when its score is **greater than** this value. |
| `duplicate-code-min-tokens` | `30` | `duplicate-code` | Minimum length, in tokens, of a duplicated run that is reported. Lower values find more (and noisier) clones. |
| `duplicate-string-threshold` | `3` | `no-duplicate-string` | Number of occurrences at which a literal is reported. The rule reports each occurrence from the Nth one onward. |
| `duplicate-string-min-length` | `10` | `no-duplicate-string` | Minimum literal length, in bytes of the unquoted content, before it is counted at all. |
| `max-switch-cases` | `10` | `max-switch-cases` | A `switch` is reported when it has **more than** this many non-`default` `case` clauses. |
| `no-small-switch-min-cases` | `3` | `no-small-switch` | A `switch` is reported when it has **fewer than** this many non-`default` `case` clauses. |

Raising `duplicate-code-min-tokens` is the usual way to keep clone detection while
ignoring short, idiomatic repetitions.

## `[files]`

### `[files].ignore`

| Type | Default | Scope |
|---|---|---|
| array of glob patterns | `[]` | the whole run |

Files matching these patterns are removed from the run: they are not walked, not parsed
and not analyzed, and they never enter clone detection. Passing an ignored path explicitly
still produces the warning `ignoring <path>: excluded by nullscan.toml`.

Use `[files].ignore` for generated code, vendored dependencies and test fixtures that are
not part of the codebase you want to measure.

## `[duplicates]`

### `[duplicates].ignore`

| Type | Default | Scope |
|---|---|---|
| array of glob patterns | `[]` | clone detection only |

Files matching these patterns are still analyzed by every per-file rule, but they are not
compared for clones and do not contribute to either side of the duplication density
(neither duplicated nor total lines). This is the right knob for fixtures and mocks that
are intentionally copy-pasted.

### `[duplicates].compare-markup`

| Type | Default | Scope |
|---|---|---|
| boolean | `false` | `duplicate-code` |

By default clone detection counts **executable logic**: JSX tags, attributes, attribute
strings, text, static JSX and `<svg>` subtrees are treated as data and ignored, while the
logic inside `{...}` expressions still counts. This avoids reporting two components as
clones just because they share an icon or a wrapper.

Set `compare-markup = true` to restore the older behavior that tokenizes presentational
markup too. A wrong TOML type is a config error.

## Glob syntax

Patterns use `/` separators on every platform and follow these rules:

| Pattern | Matches |
|---|---|
| `tests` | any path segment named `tests`, at any depth (`tests/`, `src/tests/`) |
| `*.test.ts` | one segment ending in `.test.ts`, at any depth |
| `src/generated` | that path relative to the config directory |
| `src/generated/` | the same path and everything below it |
| `**/__fixtures__/**` | every file under any `__fixtures__` directory |
| `/abs/path/to/file.ts` | an absolute path (a Windows drive letter counts as absolute) |

- `*` and `?` never cross a `/`; `**` spans any number of segments.
- A trailing `/` covers the whole subtree.
- There is **no negation** and no character-class support. A pattern starting with `!`
  is ignored with `warning: unsupported ignore pattern '<pattern>': negation is not
  supported`.
- Paths outside the config root can only be matched by an absolute pattern.

## Validation and errors

| Situation | Result |
|---|---|
| Unknown top-level key or unknown key inside a table | Ignored silently (the file still loads). |
| Wrong TOML type for a known key (`ignore = "tests"`, `compare-markup = "yes"`, a string threshold) | Config error: the CLI prints the file and the TOML error and exits `2`. |
| Unknown id in `[rules].disabled` | Warning, run continues. |
| Negated ignore pattern | Warning, pattern ignored, run continues. |
| Explicit input path excluded by `[files].ignore` | Warning, that path is skipped. |
| Broken TOML syntax | Config error (exit `2` for the CLI, warning + defaults in the server). |

Warnings are non-fatal. With `--format json` they are part of the document under
`warnings`; with `--format text` and `--format sarif` they are printed to stderr.

## Recipes

### Strict CI gate

Fail on more of everything, but keep the run fast by skipping clone detection:

```toml
[rules]
disabled = ["duplicate-code", "no-console"]

[thresholds]
cognitive-complexity = 10
max-switch-cases = 6
```

### React / TSX codebase

Ignore presentational markup (the default) and keep generated icon folders out of the
clone pass:

```toml
[duplicates]
ignore = ["src/icons/**", "**/__fixtures__/**"]
compare-markup = false
```

### Noisy duplicated strings

Keep the rule but exclude the project's vocabulary:

```toml
[rules.no-duplicate-string]
ignore = ["transparent", "currentColor", "application/json", "100%"]
```

### Excluding tests entirely

```toml
[files]
ignore = ["tests/", "**/*.test.ts", "**/*.spec.ts"]
```
