export function update(values: number[], target: { x: number }): void {
    values[0] = 1;
    use(values[0]);
    values[0] = 2;
    target.x = 1;
    target.x += 1;
}
