// A generator that only returns never yields anything.
export function* empty(): Generator<number> {
    return 1;
}

// A generator with an empty body never yields either.
function* blank(): Generator<number> {}

// A generator expression without a yield is reported too.
export const broken = function* (): Generator<number> {
    return 2;
};

// Generator methods in classes and object literals are checked.
class Empty {
    *values(): Generator<number> {
        return 1;
    }
}

const helpers = {
    *doubles(): Generator<number> {
        return 2;
    },
};

// A yield inside a nested function belongs to that function, not to this generator.
function* outer(): Generator<number> {
    function inner(): Generator<number> {
        yield 1;
    }

    inner();
}

// A yield inside a nested class body belongs to that class scope.
function* withClass(): Generator<number> {
    class Inner {
        field = yield 1;
    }
}
