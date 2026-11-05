export function schedule(
    callback: () => void,
    delay: number,
    name: string,
): void {
    setTimeout(callback, delay);
    setInterval(() => callback(), delay);
    setImmediate(callback);
    execScript(name);
    window.setTimeout(callback, delay);
    setTimeout(`run ${name}`, delay);
}
