import { describe, expect, it } from "vitest";

import { densityLabel, duplicationCard, type DuplicationStats } from "../src/duplication";

function stats(overrides: Partial<DuplicationStats> = {}): DuplicationStats {
  return {
    density: 12.5,
    duplicatedLines: 25,
    totalLines: 200,
    duplicatedBlocks: 2,
    duplicatedFiles: 3,
    largestBlockLines: 42,
    ...overrides,
  };
}

describe("duplicationCard", () => {
  it("formats the density, the line share and the counts", () => {
    const card = duplicationCard(stats());

    expect(card.density).toBe("12.5%");
    expect(card.barWidth).toBe("12.5%");
    expect(card.lines).toBe("25 / 200 lines");
    expect(card.blocks).toBe("2 blocks");
    expect(card.files).toBe("3 files");
    expect(card.largestBlock).toBe("42 lines");
  });

  it("keeps whole percentages at one decimal", () => {
    expect(duplicationCard(stats({ density: 0 })).density).toBe("0.0%");
    expect(duplicationCard(stats({ density: 100 })).density).toBe("100.0%");
    expect(densityLabel(33.3)).toBe("33.3");
  });

  it("uses singular nouns for one", () => {
    const card = duplicationCard(
      stats({ duplicatedBlocks: 1, duplicatedFiles: 1, largestBlockLines: 1 }),
    );

    expect(card.blocks).toBe("1 block");
    expect(card.files).toBe("1 file");
    expect(card.largestBlock).toBe("1 line");
  });

  it("clamps the bar so it never overflows its track", () => {
    expect(duplicationCard(stats({ density: 0 })).barWidth).toBe("0%");
    expect(duplicationCard(stats({ density: -4 })).barWidth).toBe("0%");
    expect(duplicationCard(stats({ density: 140 })).barWidth).toBe("100%");
  });

  it("renders a clean project without duplications", () => {
    const card = duplicationCard(
      stats({
        density: 0,
        duplicatedLines: 0,
        totalLines: 120,
        duplicatedBlocks: 0,
        duplicatedFiles: 0,
        largestBlockLines: 0,
      }),
    );

    expect(card.density).toBe("0.0%");
    expect(card.lines).toBe("0 / 120 lines");
    expect(card.blocks).toBe("0 blocks");
    expect(card.files).toBe("0 files");
  });
});
