export function scan(items: number[]): number {
    for (const item of items) {
        if (item > 10) {
            return item;
        }
    }
    return 0;
}

export function first(items: number[]): number {
    for (const item of items) {
        if (item) {
            break;
        }
    }
    return 0;
}
