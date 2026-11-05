export function Badge({ value }: { value: number }) {
    const isPositive = value > 0;
    return <span>{isPositive ? "yes" : "no"}</span>;
}
