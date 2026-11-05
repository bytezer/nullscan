export function absolute(value: number): number {
    if (value < 0) {
        return -value;
    }
    return value;
}

export function fail(message: string): never {
    throw new Error(message);
}

export function logOnly(message: string): void {
    console.log(message);
}
