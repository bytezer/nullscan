export function outer() {
    const value = 1;
    function inner() {
        const other = value;
        return other;
    }
    return inner();
}
