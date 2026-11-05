export function check(a: boolean): boolean {
    if (a && a) {
        return true;
    }
    if (a || !a) {
        return true;
    }
    while (a && !a) {
        tick();
    }
    return a;
}
