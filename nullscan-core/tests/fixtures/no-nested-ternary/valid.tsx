const label = ready ? "go" : "wait";

const sign = (value: number) => (value > 0 ? "positive" : "negative");

const message = ready ? (() => (count > 1 ? "many" : "one"))() : "none";
