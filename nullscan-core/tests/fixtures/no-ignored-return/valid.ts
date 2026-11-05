export function compute(): number {
    const parsed = parseInt("1", 10);
    const keys = Object.keys({ a: 1 });
    void parseInt("2", 10);
    use(Math.max(parsed, keys.length));
    return parsed;
}
