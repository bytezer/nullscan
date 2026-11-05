export function Badge({ value }: { value: number }) {
    const same = value === value;
    return <span>{same}</span>;
}
