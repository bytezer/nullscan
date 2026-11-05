export function label(value: number): string {
    if (value === 1) {
        return "one";
    } else if (value === 2) {
        return "two";
    }
    return "many";
}
