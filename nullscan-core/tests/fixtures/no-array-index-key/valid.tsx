const list = items.map((item) => <li key={item.id}>{item.name}</li>);

const generated = items.map((item) => <li key={uuid()}>{item.name}</li>);

const index = 0;
const outsideMap = <li key={index}>{index}</li>;

const shadowed = items.map((item, position) => {
  const render = (index: number) => <li key={index}>{item.name}</li>;
  return render(position);
});
