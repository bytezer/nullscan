const simple = items.map((item, index) => <li key={index}>{item.name}</li>);

const template = items.map((item, i) => <li key={`item-${i}`}>{item.name}</li>);

const method = items.map((item, i) => <li key={i.toString()}>{item.name}</li>);

const longhand = items.map(function (item, i) {
  return <li key={i}>{item.name}</li>;
});

const destructured = items.map(({ id }, index) => <li key={index}>{id}</li>);

const flattened = items.flatMap((item, index) => <li key={index}>{item.name}</li>);

const nested = groups.map((group, groupIndex) => (
  <ul key={groupIndex}>
    {group.items.map((item, itemIndex) => (
      <li key={itemIndex}>{item.name}</li>
    ))}
  </ul>
));
