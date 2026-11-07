const numbers = [3, 1, 2];
numbers.sort((a, b) => a - b);

const ascending = numbers.toSorted((a, b) => a - b);
const joined = numbers.join(",");
const text = numbers.map(String).join("");
