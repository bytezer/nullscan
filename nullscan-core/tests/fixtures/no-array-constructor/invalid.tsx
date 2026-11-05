export function list() {
    const values = new Array(1, 2, 3);
    return <ul>{values.map((value) => <li key={value}>{value}</li>)}</ul>;
}
