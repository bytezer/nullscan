export const password = "hunter2supersecret";

export const config = {
    apiKey: "abcdefghijkl",
    client_secret: `zyxwvutsrq`,
};

export function update(target: { accessKey: string }): void {
    target.accessKey = "qwertyuiopas";
}
