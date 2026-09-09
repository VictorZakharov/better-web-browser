{
  const owner = document.currentScript.id;
  orderedRuns.push(owner); record('run:' + owner);
  Promise.resolve().then(() => record('micro:' + owner + ':' + (document.currentScript === null)));
  const box = document.getElementById('ordered');
  box.textContent = 'Ordered content ready (' + orderedRuns.length + ' elements)';
  box.style.background = '#c5ddff';
}
