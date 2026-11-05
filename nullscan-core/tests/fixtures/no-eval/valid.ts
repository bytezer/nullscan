export function run(code: string): void {
    const reference = eval;
    reference(code);
    logger.eval(code);
}
