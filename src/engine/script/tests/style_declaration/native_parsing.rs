use super::super::css_animations::assert_async_script;

fn assert_style(script: &str) {
    assert_async_script("", &format!("{script}\nfinish();"));
}

#[test]
fn invalid_or_unknown_set_property_does_not_mutate_the_attribute() {
    assert_style(
        r#"
        target.style.width = '20px';
        const previous = target.getAttribute('style');
        target.style.width = 'not-a-length';
        target.style.setProperty('unknown-property','anything');
        target.style.setProperty('width','30px','urgent');
        check('all invalid writes ignored', target.getAttribute('style') === previous);
        check('valid value retained', target.style.width === '20px');
    "#,
    );
}

#[test]
fn quoted_semicolons_and_comments_do_not_split_css_values() {
    assert_style(
        r#"
        target.style.cssText = '--message:"a;b:c";/*comment;colon:*/width:2em;color:red';
        check('quoted tokens intact', target.style.getPropertyValue('--message') === '"a;b:c"');
        check('relative value authored', target.style.width === '2em');
        check('comments ignored', target.style.color === 'red' && target.style.length === 3);
    "#,
    );
}

#[test]
fn important_duplicates_win_but_set_property_can_replace_them() {
    assert_style(
        r#"
        target.style.cssText = 'opacity:.2!important;opacity:.8;color:red;color:blue!important';
        check('priority winner', target.style.opacity === '.2' && target.style.getPropertyPriority('opacity') === 'important');
        check('native priority', getComputedStyle(target).opacity === '0.2');
        target.style.setProperty('opacity','.5');
        check('setter replaces priority', target.style.opacity === '.5' && target.style.getPropertyPriority('opacity') === '');
        check('native setter sample', getComputedStyle(target).opacity === '0.5');
    "#,
    );
}

#[test]
fn empty_set_property_removes_even_with_an_invalid_priority() {
    assert_style(
        r#"
        target.style.color = 'red';
        target.style.setProperty('color','','not-important');
        check('empty removes', target.style.color === '' && target.style.length === 0);
    "#,
    );
}

#[test]
fn priority_is_separate_from_property_value_and_case_insensitive() {
    assert_style(
        r#"
        target.style.setProperty('COLOR','red','IMPORTANT');
        check('canonical priority', target.style.getPropertyPriority('color') === 'important');
        check('value excludes annotation', target.style.color === 'red');
        check('cssText includes priority', target.style.cssText === 'color: red !important;');
        check('remove returns value', target.style.removeProperty('color') === 'red');
    "#,
    );
}

#[test]
fn indexed_style_getters_and_item_follow_the_live_declaration_order() {
    assert_style(
        r#"
        target.style.cssText = 'color:red;width:20px';
        const style = target.style;
        check('indexed names', style[0] === 'color' && style[1] === 'width' && style[2] === undefined);
        check('supported indices', 0 in style && !(2 in style));
        check('item and length', style.item(1) === 'width' && style.item(99) === '' && style.length === 2);
        style.removeProperty('color');
        check('live index shifted', style[0] === 'width' && style.length === 1);
        check('inline parentRule', style.parentRule === null);
    "#,
    );
}

#[test]
fn animation_names_are_canonicalized_without_lowercasing_them() {
    assert_style(
        r#"
        target.style.animationName = '"two words", "none", "INITIAL", Fade';
        check('canonical names', target.style.animationName === 'two\\ words, "none", "INITIAL", Fade');
        const previous = target.style.animationName;
        target.style.animationName = 'one, initial';
        check('invalid list atomic', target.style.animationName === previous);
        target.style.animationName = '""';
        check('empty string rejected', target.style.animationName === previous);
    "#,
    );
}

#[test]
fn authored_animation_times_remain_uncomputed_cssom_values() {
    assert_style(
        r#"
        target.style.animationDuration = '250ms, 2s';
        target.style.animationDelay = '-500ms';
        check('time normalization', target.style.animationDuration === '0.25s, 2s');
        check('negative delay normalization', target.style.animationDelay === '-0.5s');
        target.style.animationDuration = 'var(--duration, 1s)';
        check('variables not substituted', target.style.animationDuration === 'var(--duration, 1s)');
    "#,
    );
}

#[test]
fn raw_attribute_changes_invalidate_the_cssom_read_cache() {
    assert_style(
        r#"
        const style = target.style;
        target.setAttribute('style','width:10px');
        check('first source', style.width === '10px');
        target.setAttribute('style','width:30px;opacity:.4');
        check('new source', style.width === '30px' && style.opacity === '.4');
        target.removeAttribute('style');
        check('removed source', style.width === '' && style.length === 0);
    "#,
    );
}

#[test]
fn equal_or_invalid_writes_do_not_queue_spurious_mutation_records() {
    assert_async_script(
        "",
        r#"
        target.style.width = '20px';
        const records = [];
        new MutationObserver(items => records.push(...items)).observe(target,{attributes:true});
        target.style.width = '20px'; target.style.width = 'invalid';
        target.style.removeProperty('missing');
        queueMicrotask(() => { check('no records', records.length === 0); finish(); });
    "#,
    );
}

#[test]
fn native_cssom_parsing_cannot_call_author_parsers_or_attribute_overrides() {
    assert_style(
        r#"
        target.setAttribute = () => { throw Error('author setter'); };
        target.getAttribute = () => { throw Error('author getter'); };
        CSS.supports = () => { throw Error('author supports'); };
        target.style.width = '20px';
        target.style.animationName = '"two words"';
        check('native parsing independent', target.style.width === '20px' && target.style.animationName === 'two\\ words');
    "#,
    );
}

#[test]
fn rule_style_uses_the_same_native_validation_and_preserves_unknown_expandos() {
    assert_style(
        r#"
        document.styleSheets[0].insertRule('#target{width:20px;opacity:.2}',0);
        const style = document.styleSheets[0].cssRules[0].style;
        style.opacity = 'not-opacity'; style.setProperty('unknown-property','value');
        check('rule invalid ignored', style.opacity === '.2' && style.getPropertyValue('unknown-property') === '');
        style.breezeUnknownProperty = 42;
        check('ordinary expando', style.breezeUnknownProperty === 42);
        check('native rule sample', getComputedStyle(target).opacity === '0.2');
    "#,
    );
}

#[test]
fn editing_one_rule_preserves_other_native_declarations_without_claiming_support() {
    assert_style(
        r#"
        const sheet = new CSSStyleSheet();
        sheet.replaceSync('#target{font:16px Arial;background:#dbeafe;border:2px solid #275caa;gap:20px;opacity:.2}');
        document.adoptedStyleSheets = [sheet];
        const style = sheet.cssRules[0].style;
        const supportedBefore = CSS.supports('background','#dbeafe');
        style.opacity = '.5';
        check('background retained', style.getPropertyValue('background') === '#dbeafe');
        check('border retained', style.getPropertyValue('border') === '2px solid #275caa');
        check('font retained', style.getPropertyValue('font') === '16px Arial');
        check('capability unchanged', CSS.supports('background','#dbeafe') === supportedBefore);
        check('native background', getComputedStyle(target).backgroundColor === 'rgb(219, 234, 254)');
        check('native border', getComputedStyle(target).borderTopWidth === '2px');
        check('valid edit applied', getComputedStyle(target).opacity === '0.5');
    "#,
    );
}
