export function hoisted() {
    use(later);
    var later = 1;
    return later;
}

export function ordered() {
    const value = 1;
    return value;
}
