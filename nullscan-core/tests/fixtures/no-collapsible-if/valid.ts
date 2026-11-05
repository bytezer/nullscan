export function run(a: boolean, b: boolean): void {
    if (a) {
        if (b) {
            action();
        } else {
            fallback();
        }
    }
}
