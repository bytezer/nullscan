export function create(): Date {
    const reference = Function;
    void reference;
    return new Date();
}

export function call(obj: { Function: (source: string) => void }): void {
    obj.Function("run()");
}
