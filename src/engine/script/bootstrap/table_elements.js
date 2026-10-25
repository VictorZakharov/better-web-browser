    // HTML tables expose their structure through live [SameObject] collections over
    // direct children; table.rows orders thead rows first and tfoot rows last.
    // https://html.spec.whatwg.org/multipage/tables.html#the-table-element
    const tableCollections = new WeakMap();
    const tableCollection = (owner, name, resolve) => {
        let collections = tableCollections.get(owner);
        if (!collections) tableCollections.set(owner, collections = new Map());
        if (!collections.has(name)) collections.set(name, liveHtmlCollection(resolve));
        return collections.get(name);
    };
    const isTablePart = (node, ...names) => !!node && node.nodeType === 1 &&
        node.namespaceURI === htmlNamespace && names.includes(node.localName);
    const tableChildren = (owner, ...names) =>
        Array.from(owner.childNodes).filter(node => isTablePart(node, ...names));
    const firstTableChild = (owner, name) => tableChildren(owner, name)[0] || null;
    const tableRows = table => {
        const sectionRows = name => tableChildren(table, name)
            .flatMap(section => tableChildren(section, 'tr'));
        const body = tableChildren(table, 'tbody', 'tr')
            .flatMap(node => node.localName === 'tr' ? [node] : tableChildren(node, 'tr'));
        return [...sectionRows('thead'), ...body, ...sectionRows('tfoot')];
    };
    const tableIndexError = () => new DOMException('The index is out of range', 'IndexSizeError');
    const createTablePart = (owner, localName) =>
        owner.ownerDocument.createElementNS(htmlNamespace, localName);
    // Section and row insertion share one contract: -1 or the length appends, and any
    // other index outside [0, length] throws IndexSizeError.
    const insertTablePart = (owner, items, index, localName) => {
        index = Number(index) | 0;
        if (index < -1 || index > items.length) throw tableIndexError();
        const part = createTablePart(owner, localName);
        owner.insertBefore(part, index === -1 || index === items.length ? null : items[index]);
        return part;
    };
    const deleteTablePart = (items, index) => {
        index = Number(index) | 0;
        if (index === -1) { items[items.length - 1]?.remove(); return; }
        if (index < 0 || index >= items.length) throw tableIndexError();
        items[index].remove();
    };
    const checkTablePart = (value, localName) => {
        if (value !== null && !isTablePart(value, localName))
            throw new DOMException(`The value must be a ${localName} element`, 'HierarchyRequestError');
    };
    const firstNonHeaderChild = table => Array.from(table.childNodes)
        .find(node => !isTablePart(node, 'caption', 'colgroup')) || null;

    class HTMLTableElement extends HTMLElement {
        get caption() { return firstTableChild(this, 'caption'); }
        set caption(value) {
            checkTablePart(value, 'caption');
            this.deleteCaption();
            if (value) this.insertBefore(value, this.firstChild);
        }
        createCaption() {
            return this.caption || this.insertBefore(createTablePart(this, 'caption'), this.firstChild);
        }
        deleteCaption() { this.caption?.remove(); }
        get tHead() { return firstTableChild(this, 'thead'); }
        set tHead(value) {
            checkTablePart(value, 'thead');
            this.deleteTHead();
            if (value) this.insertBefore(value, firstNonHeaderChild(this));
        }
        createTHead() {
            return this.tHead ||
                this.insertBefore(createTablePart(this, 'thead'), firstNonHeaderChild(this));
        }
        deleteTHead() { this.tHead?.remove(); }
        get tFoot() { return firstTableChild(this, 'tfoot'); }
        set tFoot(value) {
            checkTablePart(value, 'tfoot');
            this.deleteTFoot();
            if (value) this.appendChild(value);
        }
        createTFoot() { return this.tFoot || this.appendChild(createTablePart(this, 'tfoot')); }
        deleteTFoot() { this.tFoot?.remove(); }
        get tBodies() {
            return tableCollection(this, 'tBodies', () => tableChildren(this, 'tbody'));
        }
        createTBody() {
            const bodies = tableChildren(this, 'tbody');
            const last = bodies[bodies.length - 1];
            return this.insertBefore(createTablePart(this, 'tbody'), last ? last.nextSibling : null);
        }
        get rows() { return tableCollection(this, 'rows', () => tableRows(this)); }
        insertRow(index = -1) {
            const rows = tableRows(this);
            index = Number(index) | 0;
            if (index < -1 || index > rows.length) throw tableIndexError();
            const row = createTablePart(this, 'tr');
            if (rows.length === 0) {
                const bodies = tableChildren(this, 'tbody');
                const body = bodies[bodies.length - 1] || this.appendChild(createTablePart(this, 'tbody'));
                return body.appendChild(row);
            }
            if (index === -1 || index === rows.length)
                return rows[rows.length - 1].parentNode.appendChild(row);
            return rows[index].parentNode.insertBefore(row, rows[index]);
        }
        deleteRow(index) { deleteTablePart(tableRows(this), index); }
    }
    class HTMLTableSectionElement extends HTMLElement {
        get rows() { return tableCollection(this, 'rows', () => tableChildren(this, 'tr')); }
        insertRow(index = -1) { return insertTablePart(this, tableChildren(this, 'tr'), index, 'tr'); }
        deleteRow(index) { deleteTablePart(tableChildren(this, 'tr'), index); }
    }
    class HTMLTableRowElement extends HTMLElement {
        get rowIndex() {
            const parent = this.parentNode;
            const table = isTablePart(parent, 'table') ? parent :
                isTablePart(parent, 'thead', 'tbody', 'tfoot') &&
                    isTablePart(parent.parentNode, 'table') ? parent.parentNode : null;
            return table ? tableRows(table).indexOf(this) : -1;
        }
        get sectionRowIndex() {
            const parent = this.parentNode;
            if (isTablePart(parent, 'table')) return tableRows(parent).indexOf(this);
            if (isTablePart(parent, 'thead', 'tbody', 'tfoot'))
                return tableChildren(parent, 'tr').indexOf(this);
            return -1;
        }
        get cells() { return tableCollection(this, 'cells', () => tableChildren(this, 'td', 'th')); }
        insertCell(index = -1) {
            return insertTablePart(this, tableChildren(this, 'td', 'th'), index, 'td');
        }
        deleteCell(index) { deleteTablePart(tableChildren(this, 'td', 'th'), index); }
    }
    class HTMLTableCellElement extends HTMLElement {
        get cellIndex() {
            return isTablePart(this.parentNode, 'tr')
                ? tableChildren(this.parentNode, 'td', 'th').indexOf(this) : -1;
        }
    }
    class HTMLTableCaptionElement extends HTMLElement {}
    const tableElementConstructor = localName => {
        if (localName === 'table') return HTMLTableElement;
        if (localName === 'thead' || localName === 'tbody' || localName === 'tfoot')
            return HTMLTableSectionElement;
        if (localName === 'tr') return HTMLTableRowElement;
        if (localName === 'td' || localName === 'th') return HTMLTableCellElement;
        if (localName === 'caption') return HTMLTableCaptionElement;
        return null;
    };
