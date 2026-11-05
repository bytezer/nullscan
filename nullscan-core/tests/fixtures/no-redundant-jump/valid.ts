export function done(): number {
    finish();
    return 1;
}

export function loop(items: number[]): void {
    for (const item of items) {
        if (item) {
            break;
        }
    }
}
