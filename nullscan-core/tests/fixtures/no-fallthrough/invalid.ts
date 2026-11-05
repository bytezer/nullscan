export function classify(value: number): string {
    switch (value) {
        case 0:
            prepare();
        case 1:
            return "one";
        case 2:
            return "two";
        default:
            return "other";
    }
}
