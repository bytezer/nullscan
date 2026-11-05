import { Button } from "./button";

export function Panel({ label }: { label: string }) {
    const text = label.toUpperCase();
    return <Button title={text} />;
}
