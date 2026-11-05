export function list() {
    const values = [1, 2, 3];
    const sized = new Array(500);
    return <ul>{values.map((value) => <li key={value}>{value}</li>)}</ul>;
}
