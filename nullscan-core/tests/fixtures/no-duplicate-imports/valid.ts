import { a } from "./a";
import { b } from "./b";
export { a, b };
import { c } from "./c";
import type { C } from "./c";
const typed: C = c;
export { typed };
