export function before() {
    use(value);
    let value = 1;
    value = 2;
    return value;
}
