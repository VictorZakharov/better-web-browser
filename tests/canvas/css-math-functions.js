'use strict';
// CSS Values 4 §§10.3–10.9. Fixed geometry makes this usable as a differential
// fixture without a dependency on fonts, live network assets or screenshots.
const mathCases = [
  ['round(50%, 30px)', 120],
  ['round(up, 49%, 30px)', 120],
  ['round(down, 49%, 30px)', 90],
  ['round(to-zero, -49%, 30px)', -90],
  ['round(-45px, 30px)', -30],
  ['round(45px, -30px)', 60],
  ['mod(-18px, 5px)', 2],
  ['mod(18px, -5px)', -2],
  ['rem(-18px, 5px)', -3],
  ['rem(18px, -5px)', 3],
  ['abs(50% - 140px)', 20],
  ['hypot(30px, 40px)', 50],
  ['hypot(-30px)', 30],
  ['hypot(30px, 50%)', Math.hypot(30, 120)],
  ['calc(sign(50% - 140px) * 20px)', -20],
  ['calc(sign(50% - 100px) * 20px)', 20],
  ['calc(pow(2, 3) * 10px)', 80],
  ['calc(sqrt(25) * 10px)', 50],
  ['calc(log(8, 2) * 10px)', 30],
  ['calc(log(e) * 10px)', 10],
  ['calc(exp(0) * 10px)', 10],
  ['calc(sin(90deg) * 100px)', 100],
  ['calc(cos(180deg) * 100px)', -100],
  ['calc(cos(.25turn) * 100px)', 0],
  ['calc(sin(100grad) * 100px)', 100],
  ['calc(sin(pi / 2) * 100px)', 100],
  ['calc(sin(asin(1)) * 100px)', 100],
  ['calc(cos(acos(-1)) * 100px)', -100],
  ['calc(tan(atan(2)) * 100px)', 200],
  ['calc(sin(atan2(3px, 4px)) * 100px)', 60],
  ['calc(sin(atan2(30%, 40%)) * 100px)', 60],
  ['calc((1turn / 90deg) * 100px)', 400],
  ['calc(pow(30px / 1px, 2) * 1px)', 900],
  ['round(up, 2em, 30px)', 60],
  ['round(down, 2rem, 30px)', 30],
  ['calc(sqrt(-1) * 10px)', 0],
  ['mod(20px, 0px)', 0],
  ['calc(pow(NaN, 0) * 10px)', 0],
  ['calc(sign(hypot(infinity, NaN)) * 10px)', 0],
  ['calc(min(2, NaN) * 10px)', 0],
  ['calc(sign(1 / (-1 * 0)) * 10px)', -10],
  ['calc(sign(1 / -0) * 10px)', -10],
  ['calc(sign(1 / round(up, -.1, 1)) * 10px)', -10],
  ['calc(sign(1 / min(0, -1 * 0)) * 10px)', -10],
  ['calc(sign(1 / max(0, -1 * 0)) * 10px)', 10],
  ['calc(sign(1 / mod(10, -5)) * 10px)', -10],
  ['calc(sign(1 / rem(-10, 5)) * 10px)', -10],
  ['calc(sign(1 / abs(-1 * 0)) * 10px)', 10],
  ['calc(sign(mod(-10, infinity)) * 10px)', 0],
  ['calc(sign(rem(-10, infinity)) * 10px)', -10],
  ['calc(sign(round(up, 1, infinity)) * 10px)', 10],
  ['calc(sign(round(down, -1, infinity)) * 10px)', -10]
];
const invalidMathCases = [
  'round(20px)', 'round(sideways, 20px, 2px)', 'round(20px, 2)',
  'hypot(20px, 2)', 'calc(sin(1px) * 10px)',
  'calc(asin(1deg) * 10px)', 'calc(atan2(1px, 1deg) * 10px)',
  'calc(1px + 1deg)', 'calc(0 + 10px)', 'calc(1deg * 10px)',
  'calc(1px+ 2px)', 'calc(1px +2px)'
];
const parent = document.getElementById('parent');
const samples = [];
for (let index = 0; index < mathCases.length + invalidMathCases.length; index++) {
  const valid = index < mathCases.length;
  const entry = valid ? mathCases[index] : [invalidMathCases[index - mathCases.length], 77];
  const element = document.createElement('div');
  element.className = 'sample';
  element.style.cssText = 'top:' + (index * 3) + 'px;left:77px;left:' + entry[0];
  parent.appendChild(element);
  samples.push({element, expression:entry[0], expected:entry[1], valid});
}
// The native harness waits for this marker. RAF ensures pending style/layout
// work has been applied in both engines before reading rectangles.
requestAnimationFrame(() => {
  const origin = parent.getBoundingClientRect().x;
  const rows = samples.map(sample => {
    const actual = sample.element.getBoundingClientRect().x - origin;
    const supported = CSS.supports('left', sample.expression);
    return {expression:sample.expression, expected:sample.expected, actual,
      supported, valid:sample.valid,
      pass:Math.abs(actual - sample.expected) <= 0.02 && supported === sample.valid};
  });
  const results = document.getElementById('results');
  const failed = rows.filter(row => !row.pass);
  // Diagnostic attributes are intentionally capped at 512 characters. Keep
  // each owned result chunk small instead of raising that browser safety limit.
  for (let start = 0; start < rows.length; start += 16) {
    results.setAttribute('data-rows-' + (start / 16), JSON.stringify(rows.slice(start, start + 16)
      .map(row => [row.actual, row.supported ? 1 : 0, row.expected, row.valid ? 1 : 0])));
  }
  results.setAttribute('data-count', String(rows.length));
  results.setAttribute('data-failed', String(failed.length));
  results.setAttribute('data-done', 'true');
  results.textContent = failed.length ? JSON.stringify(failed) : rows.length + ' geometry/type checks passed';
});
