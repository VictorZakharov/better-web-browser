using System.Text.Json;

namespace ChromiumBaseline;

internal static class BrowserScripts
{
    public const string DocumentProbe = """
        (() => {
          const media = document.querySelector('video, audio');
          return ({
          url: location.href,
          bodyTextLength: (document.body?.innerText || '').trim().length,
          elementCount: document.querySelectorAll('*').length,
          browserErrorSurface: location.protocol === 'chrome-error:' ||
            document.documentURI.startsWith('chrome-error://'),
          documentHeight: Math.max(document.documentElement.scrollHeight, document.body?.scrollHeight || 0),
          fixtureReady: document.documentElement.dataset.fixtureReady === 'true',
          innerWidth: window.innerWidth,
          innerHeight: window.innerHeight,
          media: media ? {
            currentTime: Number(media.currentTime) || 0,
            duration: Number(media.duration) || 0,
            paused: media.paused,
            ended: media.ended,
            readyState: media.readyState,
            videoWidth: Number(media.videoWidth) || 0,
            videoHeight: Number(media.videoHeight) || 0
          } : null
          });
        })()
        """;

    public const string FirstPaint = """
        (() => {
          const paint = performance.getEntriesByName('first-contentful-paint')[0];
          return paint ? paint.startTime : performance.getEntriesByType('navigation')[0]?.domContentLoadedEventEnd || 0;
        })()
        """;

    public static string SteadyScroll(int samples) => $$"""
        (async () => {
          const count = {{samples}};
          const values = [];
          const maximum = Math.max(0, document.documentElement.scrollHeight - innerHeight);
          for (let index = 1; index <= count; index++) {
            const started = performance.now();
            scrollTo(0, Math.round(maximum * index / (count + 1)));
            await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
            values.push(performance.now() - started);
          }
          values.sort((left, right) => left - right);
          const total = values.reduce((sum, value) => sum + value, 0);
          return { samples: values.length, averageMs: total / Math.max(values.length, 1),
            p95Ms: values[Math.max(0, Math.ceil(values.length * .95) - 1)] || 0,
            maximumMs: values[values.length - 1] || 0,
            longFrames: values.filter(value => value > 50).length, finalScrollY: scrollY };
        })()
        """;

    public const string EarlyScroll = """
        (async () => {
          const values = [];
          const maximum = Math.max(0, document.documentElement.scrollHeight - innerHeight);
          let direction = 1;
          for (let index = 0; index < 375; index++) {
            const deadline = performance.now() + 16;
            let target = scrollY + 42 * direction;
            if (target >= maximum) { target = maximum; direction = -1; }
            if (target <= 0) { target = 0; direction = 1; }
            const started = performance.now();
            scrollTo(0, target);
            await new Promise(resolve => requestAnimationFrame(resolve));
            values.push(performance.now() - started);
            const delay = deadline - performance.now();
            if (delay > 0) await new Promise(resolve => setTimeout(resolve, delay));
          }
          values.sort((left, right) => left - right);
          const total = values.reduce((sum, value) => sum + value, 0);
          return { samples: values.length, averageMs: total / values.length,
            p95Ms: values[Math.max(0, Math.ceil(values.length * .95) - 1)] || 0,
            maximumMs: values[values.length - 1] || 0,
            longFrames: values.filter(value => value > 50).length, finalScrollY: scrollY };
        })()
        """;

    public static string SelectorDiagnostics(IReadOnlyList<string> selectors)
    {
        var selectorJson = JsonSerializer.Serialize(selectors);
        return $$"""
            (() => {
              const selectors = {{selectorJson}};
              const roots = [document];
              for (let index = 0; index < roots.length; index++) {
                for (const element of roots[index].querySelectorAll('*')) {
                  if (element.shadowRoot) roots.push(element.shadowRoot);
                }
              }
              const properties = [
                'display', 'position', 'visibility', 'opacity', 'box-sizing',
                'width', 'height', 'min-width', 'min-height', 'max-width', 'max-height',
                'margin-top', 'margin-right', 'margin-bottom', 'margin-left',
                'padding-top', 'padding-right', 'padding-bottom', 'padding-left',
                'flex-basis', 'flex-direction', 'flex-grow', 'flex-shrink', 'flex-wrap',
                'align-items', 'align-self', 'justify-content', 'gap',
                'grid-template-columns', 'grid-template-rows',
                'overflow-x', 'overflow-y', 'aspect-ratio', 'object-fit',
                'inset', 'top', 'right', 'bottom', 'left', 'z-index',
                'transform', 'background-color', 'background-image', 'color'
              ];
              const authoredRules = [];
              const collectRules = (root, rules, conditions = [], active = true) => {
                for (const rule of rules) {
                  if (rule.selectorText && rule.style)
                    authoredRules.push({ root, rule, conditions, active });
                  else if (rule.cssRules) {
                    let condition = null;
                    if (rule instanceof CSSMediaRule)
                      condition = { kind: 'media', text: rule.conditionText,
                        matches: matchMedia(rule.conditionText).matches };
                    else if (rule instanceof CSSSupportsRule)
                      condition = { kind: 'supports', text: rule.conditionText,
                        matches: CSS.supports(rule.conditionText) };
                    collectRules(root, rule.cssRules,
                      condition ? [...conditions, condition] : conditions,
                      active && (!condition || condition.matches));
                  }
                }
              };
              for (const root of roots) {
                const sheets = new Set(root === document ? Array.from(document.styleSheets) :
                  Array.from(root.querySelectorAll('style'), element => element.sheet).filter(Boolean));
                for (const sheet of root.adoptedStyleSheets || []) sheets.add(sheet);
                for (const sheet of sheets) {
                  try { collectRules(root, sheet.cssRules); } catch (_) {}
                }
              }
              const matchingRules = element => {
                const root = element.getRootNode();
                const matches = [];
                for (const { root: ruleRoot, rule, conditions, active } of authoredRules) {
                  if (ruleRoot !== root || !active) continue;
                  let matched = false;
                  try { matched = element.matches(rule.selectorText); } catch (_) {}
                  if (!matched) continue;
                  const declarations = Object.fromEntries(properties
                    .map(name => [name, rule.style.getPropertyValue(name),
                      rule.style.getPropertyPriority(name)])
                    .filter(([, value]) => value)
                    .map(([name, value, priority]) => [name,
                      priority ? `${value} !${priority}` : value]));
                  if (Object.keys(declarations).length) {
                    matches.push({ selector: rule.selectorText, conditions, declarations });
                    if (matches.length >= 64) break;
                  }
                }
                return matches;
              };
              const inactiveMatchingRules = element => {
                const root = element.getRootNode();
                const matches = [];
                for (const { root: ruleRoot, rule, conditions, active } of authoredRules) {
                  if (ruleRoot !== root || active) continue;
                  let matched = false;
                  try { matched = element.matches(rule.selectorText); } catch (_) {}
                  if (!matched) continue;
                  const declarations = Object.fromEntries(properties
                    .map(name => [name, rule.style.getPropertyValue(name),
                      rule.style.getPropertyPriority(name)])
                    .filter(([, value]) => value)
                    .map(([name, value, priority]) => [name,
                      priority ? `${value} !${priority}` : value]));
                  if (Object.keys(declarations).length) {
                    matches.push({ selector: rule.selectorText, conditions, declarations });
                    if (matches.length >= 64) break;
                  }
                }
                return matches;
              };
              const customProperties = style => {
                const names = Array.from({ length: style.length }, (_, index) => style.item(index))
                  .filter(name => name.startsWith('--')).sort();
                const retained = names.length <= 64 ? names :
                  [...names.slice(0, 32), ...names.slice(-32)];
                return { count: names.length, truncated: retained.length < names.length,
                  values: Object.fromEntries(retained.map(name =>
                    [name, style.getPropertyValue(name)])) };
              };
              const describe = element => {
                const rect = element.getBoundingClientRect();
                const style = getComputedStyle(element);
                return {
                  tag: element.localName,
                  id: element.id || null,
                  class: typeof element.className === 'string' ? element.className : null,
                  attributes: Object.fromEntries(Array.from(element.attributes).slice(0, 64)
                    .map(attribute => [attribute.name.slice(0, 128),
                      attribute.value.slice(0, 512)])),
                  inline_style: (element.getAttribute('style') || '').slice(0, 2048) || null,
                  text_length: (element.textContent || '').trim().length,
                  child_count: element.children.length,
                  parent: element.parentElement ? {
                    tag: element.parentElement.localName,
                    id: element.parentElement.id || null,
                    class: typeof element.parentElement.className === 'string'
                      ? element.parentElement.className : null
                  } : null,
                  rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
                  style: Object.fromEntries(properties.map(name => [name, style.getPropertyValue(name)])),
                  custom_properties: customProperties(style),
                  matching_rules: matchingRules(element),
                  inactive_matching_rules: inactiveMatchingRules(element),
                  image: element instanceof HTMLImageElement ? {
                    src: element.currentSrc || element.src,
                    complete: element.complete,
                    natural_width: element.naturalWidth,
                    natural_height: element.naturalHeight
                  } : null
                };
              };
              return selectors.map(selector => {
                const matches = [];
                for (const root of roots) {
                  for (const element of root.querySelectorAll(selector)) {
                    if (!matches.includes(element)) matches.push(element);
                    if (matches.length >= 32) break;
                  }
                  if (matches.length >= 32) break;
                }
                return { selector, total_matches: matches.length,
                  matches: matches.map(describe) };
              });
            })()
            """;
    }
}
