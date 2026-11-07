/**
 * Line-density duplication metric published as `summary.duplication`.
 *
 * Kept free of the `vscode` API so the formatting can be unit-tested and reused by any
 * view. The server computes the numbers; this module only turns them into labels.
 */

/** Matches `summary.duplication` of `--format json` and `nullscan/analyzeWorkspace`. */
export interface DuplicationStats {
  /** Percentage of lines of code inside a clone, rounded to one decimal by the server. */
  density: number;
  duplicatedLines: number;
  totalLines: number;
  duplicatedBlocks: number;
  duplicatedFiles: number;
  largestBlockLines: number;
}

/** Everything the Duplication card renders, pre-formatted for the webview. */
export interface DuplicationCard {
  /** `12.5%` */
  density: string;
  /** `12.5%`, clamped so the bar can never overflow its track. */
  barWidth: string;
  /** `120 / 980 lines` */
  lines: string;
  /** `3 blocks` */
  blocks: string;
  /** `2 files` */
  files: string;
  /** `42 lines` */
  largestBlock: string;
}

export function duplicationCard(stats: DuplicationStats): DuplicationCard {
  return {
    density: `${densityLabel(stats.density)}%`,
    barWidth: `${clampPercent(stats.density)}%`,
    lines: `${stats.duplicatedLines} / ${stats.totalLines} lines`,
    blocks: countOf(stats.duplicatedBlocks, "block"),
    files: countOf(stats.duplicatedFiles, "file"),
    largestBlock: countOf(stats.largestBlockLines, "line"),
  };
}

/** One decimal even for whole percentages, so `0` reads as `0.0`. */
export function densityLabel(density: number): string {
  return density.toFixed(1);
}

function clampPercent(density: number): number {
  return Math.min(100, Math.max(0, density));
}

function countOf(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}
