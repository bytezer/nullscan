export function Link() {
    return (
        <div>
            <a href="javascript:void(0)">bad</a>
            <a href={`JavaScript:alert(1)`}>also bad</a>
        </div>
    );
}
