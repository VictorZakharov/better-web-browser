(() => {
  const rows = document.getElementById('results');
  let count = 0, failures = 0;
  const check = (name, condition) => {
    count++;
    if (!condition) failures++;
    const row = document.createElement('li');
    row.dataset.pass = String(condition);
    row.textContent = `${condition ? 'PASS' : 'FAIL'}: ${name}`;
    rows.append(row);
  };
  const sample = (id, time) => {
    const node = document.getElementById(id);
    const animation = node.getAnimations()[0];
    check(`${id}: stylesheet creates a CSSAnimation`,
      typeof CSSAnimation === 'function' && animation instanceof CSSAnimation);
    if (animation) animation.currentTime = time;
    return [node, animation];
  };
  try {
    const [fade, animation] = sample('fade', 500);
    check('Native opacity sampled at half time', getComputedStyle(fade).opacity === '0.5');
    check('No authored opacity attribute', fade.style.opacity === '');
    const [color] = sample('color', 500);
    check('Native color interpolation', getComputedStyle(color).color === 'rgb(100, 50, 20)');
    const [edges] = sample('edges', 500);
    check('Shorthand expands into native edges', getComputedStyle(edges).marginTop === '8px' &&
      getComputedStyle(edges).marginRight === '4px');
    const [implicit] = sample('implicit', 500);
    check('Implicit endpoint reads underlying style', getComputedStyle(implicit).opacity === '0.6');
    implicit.style.opacity = '.4';
    check('Underlying endpoint updates without restarting', getComputedStyle(implicit).opacity === '0.7');
    const [layer] = sample('layer', 500);
    check('Layer rank outranks source order', getComputedStyle(layer).opacity === '0.2');

    const rule = [...document.styleSheets[0].cssRules].find(rule => rule instanceof CSSKeyframesRule && rule.name === 'fade');
    check('Real CSSKeyframesRule interface', !!rule && rule.type === CSSRule.KEYFRAMES_RULE);
    if (rule) {
      const list = rule.cssRules;
      check('Keyframe CSSOM parent links', list[0].parentRule === rule);
      rule.findRule('to').style.opacity = '.8';
      check('CSSOM edit updates running sample', getComputedStyle(fade).opacity === '0.4');
      check('CSSOM edit preserves identity and time', fade.getAnimations()[0] === animation && animation.currentTime === 500);
      rule.findRule('to').style.opacity = '1';
      rule.appendRule('50%{opacity:.9}');
      check('Appending a keyframe updates effect', getComputedStyle(fade).opacity === '0.9');
      rule.deleteRule('50%');
      check('Live CSSRuleList reflects deletion', list.length === 2);
      check('Deleting a keyframe restores path', getComputedStyle(fade).opacity === '0.5');
    }

    const host = document.getElementById('shadow-host');
    const root = host.attachShadow({mode:'open'});
    root.innerHTML = '<style>@keyframes fade{from{opacity:0}to{opacity:.6}} :host{animation:fade 1s linear both paused}</style><span>Shadow-scoped host</span>';
    const shadowAnimation = host.getAnimations()[0];
    check('Host animation discovers its own tree-scoped name', !!shadowAnimation);
    if (shadowAnimation) {
      shadowAnimation.currentTime = 500;
      check('Host uses shadow keyframes', getComputedStyle(host).opacity === '0.3');
    }
    fade.setAttribute('data-unrelated', 'changed');
    check('Unrelated mutation preserves animation time', fade.getAnimations()[0] === animation && animation.currentTime === 500);
    check('Computed animation settings remain author values', getComputedStyle(fade).animationDuration === '1s');
  } catch (error) {
    check('Fixture completed without exceptions: '+error.name+': '+error.message, false);
  }
  document.documentElement.dataset.animationChecks = String(count);
  document.documentElement.dataset.animationFailures = String(failures);
  document.documentElement.dataset.fixtureReady = 'true';
  document.getElementById('fixture-status').textContent = `${count-failures}/${count} behavior checks passed`;
})();
