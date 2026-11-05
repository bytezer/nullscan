export function run(code: string): void {
    eval(code);
    window.eval(code);
    global.eval(code);
    globalThis.eval(code);
    self.eval(code);
    this.eval(code);
}
