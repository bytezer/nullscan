async function load(): Promise<number> {
  return 1;
}

const save = async () => {};
const typed = (): Promise<string> | undefined => undefined;

class Store {
  async flush() {}
  run() {
    this.flush();
  }
}

load();
save();
typed();
new Promise((resolve) => resolve(1));
Promise.all([load(), save()]);
fetch("/api");
load().then((value) => value);
load().finally(() => {});
(async () => {
  await load();
})();
