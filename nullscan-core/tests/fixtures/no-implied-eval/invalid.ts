export function schedule(): void {
    setTimeout("run()", 0);
    setInterval(`tick()`, 10);
    setImmediate("task()");
    execScript("legacy()");
    window.setTimeout("run()", 0);
    global.setInterval("tick()", 10);
    globalThis.setImmediate("task()");
}
