use super::*;

fn check(code: &str) {
    let (_, outcome) = execute_html(&format!("<script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn component_patterns_match_normalized_urls_and_return_named_groups() {
    check(
        r#"
        const assert = (condition, message) => { if (!condition) throw Error(message); };
        const pattern = new URLPattern({ hostname: '*.Example.COM', pathname: '/books/:id' });
        assert(pattern.hostname === '*.example.com', 'normalized hostname pattern');
        assert(pattern.pathname === '/books/:id', 'pathname pattern');
        assert(pattern.protocol === '*' && pattern.port === '*', 'missing components wildcard');
        const result = pattern.exec('https://store.example.com/books/42?sort=up#top');
        assert(result?.hostname.input === 'store.example.com', 'hostname input');
        assert(result.hostname.groups[0] === 'store', 'unnamed hostname capture');
        assert(result.pathname.groups.id === '42', 'named pathname capture');
        assert(result.search.input === 'sort=up' && result.hash.input === 'top', 'URL components');
        assert(result.inputs.length === 1 && result.inputs[0] ===
            'https://store.example.com/books/42?sort=up#top', 'result input');
        const url = new URL('https://store.example.com/books/99');
        assert(pattern.exec(url).inputs[0] === url.href, 'URL object input serialization');
        assert(pattern.test('http://other.example.com/books/title'), 'independent scheme');
        assert(!pattern.test('https://example.net/books/42'), 'hostname mismatch');
        assert(!pattern.test('https://store.example.com/books/42/chapter'), 'segment boundary');
        assert(pattern.hasRegExpGroups === false, 'no regexp groups in supported subset');
        assert(Object.prototype.toString.call(pattern) === '[object URLPattern]', 'interface tag');
    "#,
    );
}

#[test]
fn constructor_strings_and_base_urls_inherit_only_less_specific_components() {
    check(
        r#"
        const assert = (condition, message) => { if (!condition) throw Error(message); };
        const origin = new URLPattern('https://example.com');
        assert(origin.protocol === 'https' && origin.hostname === 'example.com' &&
            origin.port === '' && origin.pathname === '*', 'origin shorthand');
        assert(origin.test('https://example.com:443/a?q#h'), 'default port and wildcard tail');
        assert(!origin.test('https://example.com:8443/a'), 'explicit port mismatch');
        const relative = new URLPattern('products/:id', 'https://example.com/shop/index');
        assert(relative.pathname === '/shop/products/:id', 'relative pathname resolution');
        const result = relative.exec('products/123', 'https://example.com/shop/index');
        assert(result?.pathname.groups.id === '123', 'relative input URL');
        assert(result.inputs[0] === 'products/123' &&
            result.inputs[1] === 'https://example.com/shop/index', 'base is reported');
        assert(!relative.test('products/123', 'https://other.example/shop/index'), 'base origin');
        const query = new URLPattern('https://example.com/search?q=:term#top');
        assert(query.search === 'q=:term' && query.hash === 'top', 'query and hash patterns');
        assert(query.exec('https://example.com/search?q=blue#top').search.groups.term === 'blue',
            'query capture');
        assert(!query.test('https://example.com/search?q=blue#other'), 'hash mismatch');
        const inherited = new URLPattern({baseURL: 'https://example.com/a?x#fragment'});
        assert(inherited.pathname === '/a' && inherited.search === 'x' &&
            inherited.hash === 'fragment', 'dictionary base inheritance');
        const partial = new URLPattern({hostname: 'other.example',
            baseURL: 'https://example.com/a?x#fragment'});
        assert(partial.protocol === 'https' && partial.port === '*' &&
            partial.pathname === '*' && partial.search === '*', 'specificity cuts inheritance');
        const dotted = new URLPattern('../:id', 'https://example.com/shop/books/index');
        assert(dotted.pathname === '/shop/:id' && dotted.test('https://example.com/shop/5'),
            'relative dot segments');
        const literalBase = new URLPattern({baseURL: 'https://example.com/a*b'});
        assert(literalBase.test('https://example.com/a*b') &&
            !literalBase.test('https://example.com/axxxb'), 'base URL punctuation is literal');
    "#,
    );
}

#[test]
fn escaped_literals_optional_segments_and_case_options_are_real_matches() {
    check(
        r#"
        const assert = (condition, message) => { if (!condition) throw Error(message); };
        const literal = new URLPattern({pathname: '/users/\\:id', search: 'q=\\*'});
        assert(literal.test({pathname: '/users/:id', search: 'q=*'}), 'escaped punctuation');
        assert(!literal.test({pathname: '/users/42', search: 'q=*'}), 'literal colon');
        assert(literal.pathname === '/users/\\:id', 'escaped getter');
        const optional = new URLPattern('/products/:id?', 'https://example.com/');
        assert(optional.test('https://example.com/products'), 'optional segment absent');
        assert(optional.exec('https://example.com/products').pathname.groups.id === undefined,
            'unmatched optional capture');
        assert(optional.exec('https://example.com/products/7').pathname.groups.id === '7',
            'optional segment present');
        assert(!optional.test('https://example.com/products/'), 'dangling slash');
        const folded = new URLPattern({pathname: '/Post/:id', search: 'Key=:value'},
            {ignoreCase: true});
        assert(folded.test({pathname: '/post/ABC', search: 'key=one'}), 'ignoreCase matching');
        const strict = new URLPattern({pathname: '/Post/:id'});
        assert(!strict.test({pathname: '/post/ABC'}), 'default case sensitivity');
        const object = {pathname: '/post/ABC', search: 'key=one',
            baseURL: 'https://example.com/a'};
        assert(folded.exec(object).inputs[0] === object, 'object input identity');
        const marker = 'BREEZEURLPATTERNESC0TOKEN';
        const masked = new URLPattern({pathname: '/marker/' + marker + '/\\:id'});
        assert(masked.test({pathname: '/marker/' + marker + '/:id'}),
            'escape masking never collides with author literal text');
    "#,
    );
}

#[test]
fn named_segment_repetition_preserves_prefix_and_capture_value() {
    check(
        r#"
        const assert = (condition, message) => { if (!condition) throw Error(message); };
        const required = new URLPattern({pathname: '/books/:chapters+'});
        assert(required.pathname === '/books/:chapters+', 'required repetition serialization');
        assert(required.exec({pathname: '/books/one/two/three'})?.pathname.groups.chapters ===
            'one/two/three', 'required capture includes delimiters');
        assert(required.exec({pathname: '/books/one'})?.pathname.groups.chapters === 'one',
            'one segment satisfies plus');
        assert(!required.test({pathname: '/books'}), 'plus requires a segment');
        assert(!required.test({pathname: '/books/'}), 'plus rejects an empty segment');

        const optional = new URLPattern({pathname: '/books/:chapters*'});
        assert(optional.pathname === '/books/:chapters*', 'zero-or-more serialization');
        assert(optional.exec({pathname: '/books'})?.pathname.groups.chapters === undefined,
            'omitted automatic slash prefix and capture');
        assert(optional.exec({pathname: '/books/one/two'})?.pathname.groups.chapters ===
            'one/two', 'zero-or-more capture includes all repeated segments');
        assert(!optional.test({pathname: '/books/'}), 'zero-or-more does not leave a slash');

        const hostname = new URLPattern({hostname: ':label+.example.com'});
        assert(hostname.exec('https://abc.example.com/')?.hostname.groups.label === 'abc',
            'hostname repetition remains inside one label');
        assert(!hostname.test('https://a.b.example.com/'),
            'hostname dot is not an automatic repeated prefix');
        const optionalHost = new URLPattern({hostname: 'api.:label?'});
        assert(optionalHost.exec({hostname: 'api.'})?.hostname.groups.label === undefined,
            'hostname optional group preserves preceding dot');
        assert(optionalHost.exec({hostname: 'api.east'})?.hostname.groups.label === 'east',
            'hostname optional group captures a label');
    "#,
    );
}

#[test]
fn single_named_capture_can_have_literal_prefix_and_suffix_in_one_segment() {
    check(
        r#"
        const assert = (condition, message) => { if (!condition) throw Error(message); };
        const pathname = new URLPattern({pathname: '/reports/file-:slug.html'});
        assert(pathname.pathname === '/reports/file-:slug.html', 'affixed getter');
        assert(pathname.exec({pathname: '/reports/file-annual.html'})?.pathname.groups.slug ===
            'annual', 'capture between literal affixes');
        assert(!pathname.test({pathname: '/reports/file-.html'}), 'capture is nonempty');
        assert(!pathname.test({pathname: '/reports/file-annual.csv'}), 'suffix is literal');
        assert(!pathname.test({pathname: '/reports/file-a/b.html'}), 'segment boundary');
        const hostname = new URLPattern({hostname: 'edge-:region.example.com'});
        assert(hostname.exec('https://edge-us.example.com/')?.hostname.groups.region === 'us',
            'literal hostname label prefix');
        assert(!hostname.test('https://edge-.example.com/'), 'hostname capture nonempty');

        for (const value of ['/x/:left:right', '/x/:id*tail', '/x/:id+tail',
            '/x/file-:id+', '/x/:id(\\d+)']) {
            let error; try { new URLPattern({pathname: value}); } catch (caught) { error = caught; }
            assert(error instanceof TypeError, 'ambiguous or unsupported syntax rejects: ' + value);
        }
    "#,
    );
}

#[test]
fn unsupported_pattern_syntax_and_invalid_inputs_fail_closed() {
    check(
        r#"
        const throws = (operation, message) => {
            let error; try { operation(); } catch (caught) { error = caught; }
            if (!(error instanceof TypeError)) throw Error(message);
        };
        throws(() => new URLPattern('/relative'), 'relative shorthand requires base');
        throws(() => new URLPattern(7), 'primitive constructor input is a relative string');
        throws(() => new URLPattern('/x', 'not a base'), 'invalid base');
        throws(() => new URLPattern('https://example.com/', 7), 'primitive base conversion');
        throws(() => new URLPattern({pathname: '/x', baseURL: 'bad base'}), 'invalid dictionary base');
        throws(() => new URLPattern({pathname: '/:id/:id'}), 'duplicate names');
        throws(() => new URLPattern({pathname: '/:id([0-9]+)'}), 'regexp syntax not claimed');
        throws(() => new URLPattern({pathname: '/{foo}'}), 'braced groups not claimed');
        throws(() => new URLPattern({pathname: '/abc\\'}), 'incomplete escape');
        throws(() => new URLPattern({pathname: '/:left:right'}), 'adjacent captures rejected');
        throws(() => new URLPattern({pathname: '/a/*/tail'}), 'middle wildcard rejected');
        throws(() => new URLPattern({search: '*a*'}), 'multiple wildcards rejected');
        throws(() => new URLPattern({pathname: '/x'}, 'https://example.com/'),
            'dictionary with separate base');
        const pattern = new URLPattern({pathname: '/x'});
        throws(() => pattern.test({pathname: '/x'}, 'https://example.com/'),
            'object input with separate base');
        throws(() => URLPattern.prototype.test.call({}, 'https://example.com/x'),
            'receiver check');
        if (pattern.test('not a URL') || pattern.exec('not a URL') !== null ||
            pattern.exec({pathname: '/x', baseURL: 'not a URL'}) !== null ||
            pattern.test(7))
            throw Error('invalid match input returns no match');
        const bounded = new URLPattern({pathname: '/*'});
        if (bounded.test({pathname: '/' + 'x'.repeat(8193)}))
            throw Error('adversarially long input exceeded component bound');
    "#,
    );
}

#[test]
fn serialized_components_and_idn_hosts_use_the_url_parser() {
    check(
        r#"
        const assert = (condition, message) => { if (!condition) throw Error(message); };
        const pattern = new URLPattern({hostname: 'bücher.example',
            pathname: '/white space', search: 'q=hello world', hash: 'some place'});
        assert(pattern.hostname === 'xn--bcher-kva.example', 'IDNA host pattern');
        assert(pattern.pathname === '/white%20space', 'serialized pathname');
        assert(pattern.search === 'q=hello%20world' && pattern.hash === 'some%20place',
            'serialized query and fragment');
        assert(pattern.test('https://bücher.example/white%20space?q=hello%20world#some%20place'),
            'serialized URL input');
        assert(pattern.test({hostname: 'BÜCHER.EXAMPLE', pathname: '/white space',
            search: '?q=hello world', hash: '#some place'}), 'dictionary URL canonicalization');
        const defaultPort = new URLPattern({protocol: 'HTTPS', port: '00443'});
        assert(defaultPort.protocol === 'https' && defaultPort.port === '' &&
            defaultPort.test('https://other.example/path'), 'default port normalization');
        for (const input of [{hostname: 'bad host'}, {port: '65536'},
            {hostname: '*.bücher.example'}]) {
            let error; try { new URLPattern(input); } catch (caught) { error = caught; }
            assert(error instanceof TypeError, 'invalid or unsupported canonicalization');
        }
    "#,
    );
}
