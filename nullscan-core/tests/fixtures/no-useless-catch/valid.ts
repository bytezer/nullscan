export function load(): void {
    try {
        read();
    } catch (error) {
        handle(error);
    }
}
