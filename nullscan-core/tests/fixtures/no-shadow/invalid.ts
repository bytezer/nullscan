export function outer() {
    const value = 1;
    function inner() {
        const value = 2;
        return value;
    }
    return inner() + value;
}
