export function render(flag: boolean) {
    if (flag) {
        return <span>on</span>;
    }
    const label = flag ? "on" : "off";
    return <span>{label}</span>;
}
