export function label(name: string) {
    const text = `outer ${`inner ${name}`}`;
    return <span>{text}</span>;
}
