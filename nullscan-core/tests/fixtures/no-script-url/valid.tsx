export function Link({ id }: { id: string }) {
    return (
        <div>
            <a href="https://example.com">safe</a>
            <a href={`/items/${id}`}>dynamic</a>
        </div>
    );
}
