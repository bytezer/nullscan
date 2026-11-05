export function load(): void {
    try {
        read();
    } catch (error) {
        throw error;
    }
}
