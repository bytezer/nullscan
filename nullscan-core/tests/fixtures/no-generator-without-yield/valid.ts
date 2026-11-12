// A generator that yields is fine.
export function* counted(): Generator<number> {
    yield 1;
    yield 2;
}

// `yield*` delegates to another iterable and counts as yielding.
function* delegating(): Generator<number> {
    yield* counted();
}

// Async generators are checked the same way.
async function* streaming(): AsyncGenerator<string> {
    yield "chunk";
}

// A generator expression with a yield is fine.
export const numbers = function* (): Generator<number> {
    yield 42;
};

// Generator methods in a class and in an object literal are fine.
class Counter {
    *values(): Generator<number> {
        yield 1;
    }

    async *chunks(): AsyncGenerator<string> {
        yield "a";
    }
}

const helpers = {
    *doubles(): Generator<number> {
        yield 2;
    },
};

// A yield in a nested block still belongs to this generator.
function* conditional(): Generator<number> {
    if (Math.random() > 0.5) {
        yield 1;
    }
}

// A plain function that returns a value is not a generator.
export function plain(): number {
    return 1;
}
