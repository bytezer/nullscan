export function classify(value: number): string {
    switch (value) {
        case 0:
            return "zero";
        case 1:
        case 2:
            return "small";
        case 3:
            prepare();
            // falls through
        case 4:
            return "four";
        default:
            return "other";
    }
}
