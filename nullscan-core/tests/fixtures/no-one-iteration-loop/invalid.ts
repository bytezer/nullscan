export function first(items: number[]): number {
    for (const item of items) {
        return item;
    }
    return 0;
}

export function stop(items: number[]): void {
    for (const item of items) {
        throw new Error(String(item));
    }
}
