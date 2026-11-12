// Each of these throws `TypeError: ... is not a function` at runtime.
const handler = "onClick"();
const lookup = "getUser"[0]();
const direct = "doIt"();
const numeric = 42();
const boolean = true();
const nothing = null();
const missing = undefined();
const object = { a: 1 }();
const array = [1, 2]();
const template = `static`();
const parenthesized = ("x")();
