// The error is thrown later, so the binding is fine.
function thrownLater(): never {
  const error = new Error("failed");
  throw error;
}

// The error is returned, so the caller handles it.
function returned(): Error {
  return new Error("failed");
}

// The error is passed to another call, which may throw it.
function passedToCall(): void {
  report(new Error("failed"));
}

// The error is stored in a collection, so someone else may throw it later.
function storedInArray(): Error[] {
  return [new Error("failed")];
}

// The error is assigned to a property, so someone else may throw it later.
function storedInObject(): { error: Error } {
  return { error: new Error("failed") };
}

// The error is created inside a larger expression, which this rule ignores.
function createdInLargerExpression(): string {
  return new Error("failed").message;
}

// A custom error class is out of scope: without types we cannot know that it
// extends Error.
class MyError extends Error {}

function customErrorClass(): void {
  new MyError("failed");
}
