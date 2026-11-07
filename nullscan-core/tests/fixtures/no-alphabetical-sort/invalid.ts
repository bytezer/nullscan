const numbers = [10, 9, 1];
numbers.sort();

const ascending = numbers.toSorted();
const explicit = numbers.sort(undefined);

const words = ["b", "a"];
words.sort();

const pair = [3, 1];
const copy = [...pair].toSorted();
