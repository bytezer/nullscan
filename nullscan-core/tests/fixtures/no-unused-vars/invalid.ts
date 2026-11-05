import { unused } from "./unused";

export function compute() {
    const value = 1;
    const { kept, dropped } = { kept: 1, dropped: 2 };
    return kept;
}
