async function load(): Promise<number> {
  return 1;
}

function sync() {
  return 1;
}

async function main() {
  await load();
  void load();
  load().catch(() => {});
  load().then((v) => v, () => {});
  const p = load();
  const all = Promise.all([load()]);
  sync();
  return load();
}

function shadow(fetch: () => void) {
  fetch();
}

main().catch(() => {});
use(load());
