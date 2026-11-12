// The error is created and dropped without ever being thrown.
new Error("failed");

// The same, without the `new` keyword.
Error("failed");

// The error is stored in a variable that is never thrown.
function droppedBinding(): void {
  const error = new Error("failed");
  void error;
}
