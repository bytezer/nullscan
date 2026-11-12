// A literal stored in a variable is not reported: without type information nullscan
// cannot know whether the variable holds a function.
const handler = "onClick";
handler();

const lookup = "getUser"[0];
lookup();

// A member expression on a literal is a real method call, not a literal call.
"abc".toUpperCase();
(42).toFixed(2);

// A template literal with a substitution is dynamic.
const name = "world";
`hello ${name}`();

// A function value is callable.
const fn = () => undefined;
fn();

function callMe(): void {
    return undefined;
}
callMe();

// `new` is not a call, so it is never reported.
const instance = new Date();
