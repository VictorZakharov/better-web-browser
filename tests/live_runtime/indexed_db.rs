//! An origin-owned IndexedDB request must survive renderer/broker/worker round trips.
use super::*;

#[test]
fn indexed_db_secondary_indexes_and_upgrade_work_in_hidden_browser() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind page fixture");
    let address = listener.local_addr().unwrap();
    let html = r#"<!doctype html><title>index pending</title>
        <style>body { background: rgb(220,20,20); } #state { height: 600px; }</style>
        <div id="state">pending</div><script>
        const fail = message => {
            document.title = 'index failure ' + (message?.name || message);
            document.getElementById('state').textContent = document.title;
        };
        const done = () => {
            document.title = 'index complete';
            document.getElementById('state').textContent = document.title;
            document.body.style.backgroundColor = 'rgb(17,170,34)';
        };
        const opened = indexedDB.open('index-runtime', 1);
        opened.onupgradeneeded = () => {
            const store = opened.result.createObjectStore('items');
            store.createIndex('by_group', 'group');
            store.createIndex('by_slug', 'slug', { unique: true });
            store.createIndex('by_tags', 'tags', { multiEntry: true });
            store.put({ group: 'b', slug: 'beta', tags: ['shared', 'shared'] }, 2);
            store.put({ group: 'a', slug: 'alpha', tags: ['shared', 'other'] }, 1);
        };
        opened.onerror = () => fail(opened.error);
        opened.onsuccess = () => {
            const db = opened.result;
            const tx = db.transaction('items');
            const store = tx.objectStore('items');
            if (!store.indexNames.contains('by_group') || store.indexNames.length !== 3)
                return fail('index names');
            const group = store.index('by_group');
            const first = group.get('a');
            const key = group.getKey('b');
            const keys = group.getAllKeys();
            const count = store.index('by_tags').count('shared');
            const cursor = group.openCursor();
            const tagCursor = store.index('by_tags').openCursor(IDBKeyRange.only('shared'));
            const seen = [];
            const jumped = [];
            cursor.onsuccess = () => {
                if (!cursor.result) return;
                seen.push([cursor.result.key, cursor.result.primaryKey,
                    cursor.result.value.slug]);
                cursor.result.continue();
            };
            tagCursor.onsuccess = () => {
                if (!tagCursor.result) return;
                jumped.push(tagCursor.result.primaryKey);
                if (jumped.length === 1)
                    tagCursor.result.continuePrimaryKey('shared', 2);
                else tagCursor.result.continue();
            };
            tx.oncomplete = () => {
                if (first.result?.slug !== 'alpha' || key.result !== 2 ||
                    String(keys.result) !== '1,2' || count.result !== 2 ||
                    String(jumped) !== '1,2' ||
                    JSON.stringify(seen) !== JSON.stringify([
                        ['a', 1, 'alpha'], ['b', 2, 'beta']]))
                    return fail('ordered index queries');
                db.close();
                const upgraded = indexedDB.open('index-runtime', 2);
                upgraded.onupgradeneeded = () => upgraded.transaction.objectStore('items')
                    .createIndex('by_pair', ['group', 'slug']);
                upgraded.onerror = () => fail(upgraded.error);
                upgraded.onsuccess = () => {
                    const next = upgraded.result.transaction('items');
                    const pair = next.objectStore('items').index('by_pair')
                        .getKey(['b', 'beta']);
                    next.oncomplete = () => pair.result === 2 ? done() : fail('upgrade backfill');
                };
            };
        };
        </script>"#
        .to_string();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 1, move |_| FixtureResponse::html(html.clone()))
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/page");
    let mut child = hidden_benchmark_with_fresh_profile_args(&url, &artifacts, 3500, &[]);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(
        report.contains("index complete"),
        "index API did not finish: {report}"
    );
    assert_green_capture(&artifacts, "index API did not repaint the page");
}

#[test]
fn indexed_db_upgrade_write_and_read_complete_in_hidden_browser() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind page fixture");
    let address = listener.local_addr().unwrap();
    let html = r#"<!doctype html><title>indexed db pending</title>
        <style>body { background: rgb(220,20,20); } #state { height: 600px; }</style>
        <div id="state">pending</div><script>
        const fail = error => {
            document.title = 'indexed db failure ' + (error?.name || error);
            document.getElementById('state').textContent = document.title;
        };
        const request = indexedDB.open('hidden-runtime-test', 1);
        request.onupgradeneeded = event => {
            if (!event.isTrusted || event.oldVersion !== 0 || event.newVersion !== 1)
                throw new Error('versionchange event contract');
            const store = request.result.createObjectStore('items');
            store.add({ answer: 42, bytes: new Uint8Array([0, 255]).buffer }, 'entry');
            const generated = request.result.createObjectStore('generated',
                { keyPath: 'meta.id', autoIncrement: true });
            generated.add({ title: 'nested key' });
        };
        request.onerror = () => fail(request.error);
        request.onsuccess = () => {
            const db = request.result;
            if (db.name !== 'hidden-runtime-test' || db.version !== 1 ||
                !db.objectStoreNames.contains('items')) return fail('database metadata');
            const transaction = db.transaction(['items', 'generated']);
            const store = transaction.objectStore('items');
            const value = store.get('entry');
            const values = store.getAll(IDBKeyRange.bound('a', 'z'));
            const keys = store.getAllKeys(undefined, 1);
            const count = store.count(IDBKeyRange.only('entry'));
            const firstKey = store.getKey(IDBKeyRange.lowerBound('entry'));
            const generated = transaction.objectStore('generated').get(1);
            value.onerror = () => fail(value.error);
            value.onsuccess = event => {
                if (!event.isTrusted || value.result?.answer !== 42 ||
                    !(value.result.bytes instanceof ArrayBuffer) ||
                    String(new Uint8Array(value.result.bytes)) !== '0,255')
                    return fail('structured clone round trip');
            };
            transaction.oncomplete = () => {
                if (values.result?.length !== 1 || values.result[0]?.answer !== 42 ||
                    keys.result?.length !== 1 || keys.result[0] !== 'entry' ||
                    count.result !== 1 || firstKey.result !== 'entry' ||
                    generated.result?.meta?.id !== 1 ||
                    !IDBKeyRange.only('entry').includes('entry'))
                    return fail('range query contract');
                const cursorTx = db.transaction('items');
                const cursor = cursorTx.objectStore('items').openCursor();
                let seen = 0;
                cursor.onsuccess = () => {
                    if (cursor.result) {
                        if (cursor.result.key !== 'entry' ||
                            cursor.result.value.answer !== 42) return fail('cursor record');
                        seen++;
                        cursor.result.continue();
                    } else if (seen !== 1) fail('cursor end');
                };
                cursorTx.oncomplete = () => {
                    if (seen !== 1 || cursor.result !== null) return fail('cursor completion');
                    indexedDB.databases().then(databases => {
                        if (databases.length !== 1 || databases[0].name !== 'hidden-runtime-test' ||
                            databases[0].version !== 1) return fail('database listing');
                        document.getElementById('state').textContent = 'indexed db complete';
                        document.body.style.backgroundColor = 'rgb(17,170,34)';
                        document.title = 'indexed db complete';
                    }, fail);
                };
            };
        };
        </script>"#
        .to_string();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 1, move |_| FixtureResponse::html(html.clone()))
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/page");
    let mut child = hidden_benchmark_with_fresh_profile_args(&url, &artifacts, 2500, &[]);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(
        report.contains("indexed db complete"),
        "IndexedDB did not finish: {report}"
    );
    assert_green_capture(&artifacts, "IndexedDB did not repaint the page");
}

#[test]
fn indexed_db_persists_after_hidden_browser_restarts_with_the_same_profile() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind page fixture");
    let address = listener.local_addr().unwrap();
    let write = r#"<!doctype html><title>write pending</title><script>
        const request = indexedDB.open('restart-test', 1);
        request.onupgradeneeded = () => request.result.createObjectStore('items');
        request.onsuccess = () => {
            const tx = request.result.transaction('items', 'readwrite');
            tx.objectStore('items').put({ value: 123 }, 'persisted');
            tx.oncomplete = () => { document.title = 'write complete'; };
        };
        </script>"#
        .to_string();
    let read = r#"<!doctype html><title>read pending</title><script>
        const request = indexedDB.open('restart-test');
        request.onupgradeneeded = () => { document.title = 'unexpected upgrade'; };
        request.onsuccess = () => {
            const tx = request.result.transaction('items');
            const record = tx.objectStore('items').get('persisted');
            record.onsuccess = () => {
                document.title = record.result?.value === 123 ?
                    'persisted across restart' : 'record lost';
            };
        };
        </script>"#
        .to_string();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, move |request| {
            FixtureResponse::html(if request.contains("GET /write") {
                write.clone()
            } else {
                read.clone()
            })
        })
    });
    let artifacts = TestArtifacts::new();
    let write_url = format!("http://{address}/write");
    let mut first = hidden_benchmark_with_reused_profile(&write_url, &artifacts, 1500);
    assert!(wait_for_child(&mut first, Duration::from_secs(20)).success());
    let first_report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(first_report.contains("write complete"), "{first_report}");
    let read_url = format!("http://{address}/read");
    let mut second = hidden_benchmark_with_reused_profile(&read_url, &artifacts, 1500);
    assert!(wait_for_child(&mut second, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let second_report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(
        second_report.contains("\"javascript_errors\": []"),
        "{second_report}"
    );
    assert!(
        second_report.contains("persisted across restart"),
        "{second_report}"
    );
}
