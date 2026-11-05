export function check(a: boolean, b: boolean): boolean {
    if (a && b) {
        return true;
    }
    if (a && !b) {
        return false;
    }
    return a || b;
}
