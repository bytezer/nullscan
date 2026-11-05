export function Badge({ label }: { label: string }) {
    if (label) {
        return <span>{label}</span>;
    }
    return <span />;
}

export function Always() {
    return <div />;
    function helper() {
        return <span />;
    }
}
