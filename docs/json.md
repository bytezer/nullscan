# JSON output contract

`nullscan --format json` prints one JSON document followed by a newline. It is the stable
interface used by editors and CI, and the language server returns the same `summary` object
from `nullscan/analyzeWorkspace` and `nullscan/summary`.

## Example

```json
{
  "schemaVersion": 1,
  "tool": {
    "name": "nullscan",
    "version": "0.1.0-beta.4"
  },
  "config": {
    "path": "/repo/nullscan.toml",
    "disabledRules": ["no-console"]
  },
  "summary": {
    "filesScanned": 128,
    "filesWithIssues": 17,
    "durationMs": 4123,
    "bySeverity": {
      "error": 2,
      "warning": 41,
      "info": 9
    },
    "byRule": [
      { "rule": "no-duplicate-string", "count": 12 },
      { "rule": "no-console", "count": 9 }
    ],
    "duplicates": 3,
    "duplication": {
      "density": 3.4,
      "duplicatedLines": 1240,
      "totalLines": 36000,
      "duplicatedBlocks": 3,
      "duplicatedFiles": 5,
      "largestBlockLines": 42
    },
    "truncated": false
  },
  "diagnostics": [
    {
      "file": "/repo/src/app.ts",
      "rule": "no-console",
      "severity": "info",
      "message": "remove this console call or use the logger",
      "range": {
        "start": { "line": 11, "character": 2 },
        "end": { "line": 11, "character": 16 }
      }
    }
  ],
  "duplicates": [
    {
      "rule": "duplicate-code",
      "severity": "warning",
      "tokenCount": 312,
      "lineCount": 42,
      "a": { "file": "/repo/src/a.ts", "startLine": 10, "endLine": 51 },
      "b": { "file": "/repo/src/b.ts", "startLine": 40, "endLine": 81 }
    }
  ],
  "warnings": [
    "warning: skipping /repo/src/legacy.js: unsupported file type"
  ]
}
```

## Top-level fields

| Field | Type | Notes |
|---|---|---|
| `schemaVersion` | integer | Always `1` for this shape. Bumped only for a breaking change. |
| `tool.name` | string | Always `"nullscan"`. |
| `tool.version` | string | Version of the binary that produced the document. |
| `config.path` | string \| null | Absolute path of the loaded `nullscan.toml`, or `null` when the defaults were used. In a multi-root editor scan it is `null` unless exactly one folder contributed a config file. |
| `config.disabledRules` | array of strings | Rule ids disabled for this run, sorted alphabetically. |
| `summary` | object | See below. |
| `diagnostics` | array | Per-file findings, in file order then rule-emission order. |
| `duplicates` | array | One entry per clone pair. |
| `warnings` | array of strings | Non-fatal problems, sorted and deduplicated. Warnings live here, never on stderr, so a JSON consumer only parses stdout. |

## `summary`

| Field | Type | Meaning |
|---|---|---|
| `filesScanned` | integer | Files that were read and analyzed successfully. |
| `filesWithIssues` | integer | Files with at least one diagnostic (duplicates are not counted here). |
| `durationMs` | integer | Time spent analyzing, in milliseconds. A multi-root editor scan sums the per-folder durations. |
| `bySeverity.error` | integer | Diagnostics with severity `error`. |
| `bySeverity.warning` | integer | Diagnostics with severity `warning`. |
| `bySeverity.info` | integer | Diagnostics with severity `info`. |
| `byRule` | array | `{ "rule": <id>, "count": <n> }`, sorted by count descending, then rule id ascending. |
| `duplicates` | integer | Number of entries in `duplicates`. |
| `duplication` | object | Line-density metric; see below. Always present. |
| `truncated` | boolean | `true` when `--max-diagnostics` (or the editor's `maxDiagnostics`) dropped diagnostics. |

### `duplication`

| Field | Type | Meaning |
|---|---|---|
| `density` | number | Percentage of lines of code that belong to at least one reported clone, rounded to one decimal. |
| `duplicatedLines` | integer | Lines of code covered by a clone; overlapping clones count a line once. |
| `totalLines` | integer | Lines of code in the files that entered the duplicate pass. |
| `duplicatedBlocks` | integer | Number of clone pairs (`= duplicates.length`). |
| `duplicatedFiles` | integer | Files with at least one duplicated line. |
| `largestBlockLines` | integer | Size of the biggest clone, in lines. |

A line of code is non-empty and not entirely a comment; a line with trailing code counts.
Only files that were not excluded by `[duplicates].ignore` contribute to either side of the
ratio. When `duplicate-code` is disabled, every field is `0` and `density` is `0.0`.

## `diagnostics`

| Field | Type | Meaning |
|---|---|---|
| `file` | string | Absolute path. |
| `rule` | string | Rule id, for example `no-console`. |
| `severity` | string | `"error"`, `"warning"` or `"info"`. |
| `message` | string | Human-readable text for this specific finding. |
| `range.start.line` | integer | **0-based** line. |
| `range.start.character` | integer | **0-based UTF-16 code unit** in that line. |
| `range.end.line` | integer | 0-based line. |
| `range.end.character` | integer | 0-based UTF-16 code unit; the end is exclusive, so a zero-length range has `start == end`. |

The 0-based UTF-16 shape matches the Language Server Protocol, so an editor can forward it
without conversion. Core diagnostics use 1-based byte columns internally; the conversion
happens when the document is rendered. Non-BMP characters (emoji) and accented letters
therefore get the same column an LSP client expects.

## `duplicates`

| Field | Type | Meaning |
|---|---|---|
| `rule` | string | Always `"duplicate-code"`. |
| `severity` | string | Always `"warning"` (the registry severity). |
| `tokenCount` | integer | Length of the duplicated run in tokens. |
| `lineCount` | integer | Number of lines in the run. |
| `a.file` | string | Absolute path of one side. |
| `a.startLine` | integer | **1-based** first line of that side. |
| `a.endLine` | integer | **1-based** last line of that side (inclusive). |
| `b.*` | object | Same shape for the other side. |

The two sides reflect the order in which the occurrences were found; each pair is emitted
once, and the pass is deterministic (files are scanned in sorted order). The language server
expands the pair into two diagnostics, one per file, when it publishes results.

> Note the asymmetry: diagnostic ranges are 0-based LSP positions, while duplicate spans
> keep core's **1-based** inclusive line numbers. This is intentional — a duplicate is a
> line block, not a character range — but consumers must not apply the same line offset to
> both sections.

## Stability

`schemaVersion: 1` is the current contract. Additive fields may appear within the same
version; consumers should ignore unknown keys. Removing or changing a field, changing a
unit (for example a 0-based line to 1-based), or changing the meaning of an existing key
requires a new `schemaVersion`.

Output order is deterministic: files are sorted, diagnostics follow rule registry order,
`byRule` is sorted, and `warnings` are sorted and deduplicated.
