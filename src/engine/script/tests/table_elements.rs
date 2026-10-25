use super::*;

fn status_after_script(body: &str) -> String {
    let html = format!(r#"<body>{body}<output id="status"></output></body>"#);
    let (dom, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("output").next().unwrap().text_content()
}

#[test]
fn table_creates_and_deletes_caption_head_and_foot_in_content_model_order() {
    let status = status_after_script(
        r#"<script>
            const table = document.createElement('table');
            const initial = [table.caption, table.tHead, table.tFoot, table.tBodies.length, table.rows.length];
            table.appendChild(document.createElement('colgroup'));
            const foot = table.createTFoot(), head = table.createTHead(), caption = table.createCaption();
            const same = table.createTHead() === head && table.createCaption() === caption && table.tBodies === table.tBodies;
            const order = Array.from(table.childNodes, node => node.localName).join('/');
            table.tHead = head;
            let hierarchy = '';
            try { table.tHead = document.createElement('tbody'); } catch (error) { hierarchy = error.name; }
            table.deleteCaption(); table.deleteTHead(); table.deleteTFoot();
            document.getElementById('status').textContent = [initial.join(','), same, order,
                hierarchy, table.childNodes.length, table instanceof HTMLTableElement].join(':');
        </script>"#,
    );
    assert_eq!(
        status,
        ",,,0,0:true:caption/colgroup/thead/tfoot:HierarchyRequestError:1:true"
    );
}

#[test]
fn table_rows_list_head_rows_then_body_rows_then_foot_rows() {
    let status = status_after_script(
        r#"<table id="parsed"><tr><td><p></tbody> </table><script>
            const table = document.createElement('table');
            const first = table.insertRow(0);
            const bodyCreated = table.tBodies.length === 1 && first.parentNode === table.tBodies[0];
            const foot = table.createTFoot().insertRow();
            const direct = table.appendChild(document.createElement('tr'));
            const head = table.createTHead().insertRow(0);
            const before = table.insertRow(1);
            const ids = new Map([[first, 'body'], [foot, 'foot'], [direct, 'direct'], [head, 'head'], [before, 'before']]);
            const order = Array.from(table.rows, row => ids.get(row)).join('/');
            const indexes = [first.rowIndex, first.sectionRowIndex, direct.rowIndex,
                direct.sectionRowIndex, document.createElement('tr').rowIndex].join(',');
            let rangeError = '';
            try { table.insertRow(9); } catch (error) { rangeError = error.name; }
            table.deleteRow(-1);
            const cell = first.insertCell(0);
            first.insertCell(-1);
            const clone = document.getElementById('parsed').cloneNode(true);
            document.getElementById('status').textContent = [bodyCreated, order, indexes, rangeError,
                table.rows.length, cell.cellIndex, first.cells.length,
                clone.tBodies[0].rows[0].cells[0].firstChild.localName, clone.lastChild.data === ' ', clone.childNodes.length].join(':');
        </script>"#,
    );
    assert_eq!(
        status,
        "true:head/before/body/direct/foot:2,1,3,3,-1:IndexSizeError:4:0:2:p:true:2"
    );
}

#[test]
fn meta_and_object_reflect_their_content_attributes() {
    let status = status_after_script(
        r#"<script>
            const meta = document.createElement('meta');
            meta.httpEquiv = 'refresh';
            meta.content = '5';
            const object = document.createElement('object');
            object.setAttribute('data', 'test.html');
            document.getElementById('status').textContent = [meta.getAttribute('http-equiv'),
                meta.hasAttribute('httpEquiv'), meta.content, 'http-equiv' in meta,
                meta instanceof HTMLMetaElement, object.data === new URL('test.html', document.baseURI).href,
                document.createElement('object').data === ''].join(':');
        </script>"#,
    );
    assert_eq!(status, "refresh:false:5:false:true:true:true");
}
