export const config = {
    password: "changeme123",
    token: "test-token-value",
    apiKey: process.env.API_KEY,
    note: "hunter2supersecret",
};

declare function use(value: unknown): void;

export function connect(settings: { secret: string }): void {
    const { secret } = settings;
    use(secret);
}
