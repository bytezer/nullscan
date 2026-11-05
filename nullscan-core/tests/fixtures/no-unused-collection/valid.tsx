export function List({ items }: { items: string[] }) {
    const lines: string[] = [];
    lines.push("first");
    return (
        <ul>
            {lines.map((line) => (
                <li key={line}>{line}</li>
            ))}
        </ul>
    );
}
