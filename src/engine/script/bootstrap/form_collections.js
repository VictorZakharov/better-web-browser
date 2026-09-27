    // The form collections are live views of the same DOM/control state used by
    // submission and rendering. See HTML's common DOM interfaces, §2.6.4.
    // https://html.spec.whatwg.org/multipage/common-dom-interfaces.html#collections
    class NodeList {
        constructor(token) {
            if (token !== htmlCollectionConstructionToken) throw new TypeError('Illegal constructor');
        }
        get length() { return collectionItems(this).length; }
        item(index) { return collectionItems(this)[Number(index) >>> 0] || null; }
        get [Symbol.toStringTag]() { return 'NodeList'; }
    }
    installIndexedIterator(NodeList.prototype, true);

    class RadioNodeList extends NodeList {
        get value() {
            const checked = collectionItems(this).find(element =>
                element instanceof HTMLInputElement && element.type === 'radio' && element.checked);
            return checked?.value ?? '';
        }
        set value(value) {
            value = String(value);
            const match = collectionItems(this).find(element =>
                element instanceof HTMLInputElement && element.type === 'radio' &&
                (value === 'on'
                    ? element.getAttribute('value') === null || element.getAttribute('value') === 'on'
                    : element.getAttribute('value') === value));
            if (match) match.checked = true;
        }
        get [Symbol.toStringTag]() { return 'RadioNodeList'; }
    }

    const matchesControlName = (element, name) =>
        element.id === name || element.getAttribute('name') === name;
    class HTMLFormControlsCollection extends HTMLCollection {
        namedItem(name) {
            name = String(name);
            if (!name) return null;
            const resolve = () => collectionItems(this).filter(element =>
                matchesControlName(element, name));
            const matches = resolve();
            if (matches.length < 2) return matches[0] || null;
            return liveNodeList(resolve, RadioNodeList);
        }
        get [Symbol.toStringTag]() { return 'HTMLFormControlsCollection'; }
    }
    const liveFormControlsCollection = resolve =>
        liveHtmlCollection(resolve, HTMLFormControlsCollection);

    const optionCollectionLimit = 100000;
    class HTMLOptionsCollection extends HTMLCollection {
        get length() { return super.length; }
        set length(value) {
            const requested = (+value) >>> 0;
            if (requested > optionCollectionLimit) return;
            const options = collectionItems(this);
            if (requested < options.length) {
                for (let index = options.length - 1; index >= requested; index--)
                    options[index].remove();
            } else if (requested > options.length) {
                const select = optionCollectionOwners.get(this);
                const fragment = select.ownerDocument.createDocumentFragment();
                for (let index = options.length; index < requested; index++)
                    fragment.appendChild(select.ownerDocument.createElement('option'));
                select.appendChild(fragment);
            }
        }
        get selectedIndex() { return optionCollectionOwners.get(this).selectedIndex; }
        set selectedIndex(value) {
            optionCollectionOwners.get(this).selectedIndex = (+value) | 0;
        }
        add(element, before = null) {
            const select = optionCollectionOwners.get(this);
            if (!(element instanceof HTMLOptionElement) &&
                !(element instanceof HTMLElement && element.localName === 'optgroup'))
                throw new TypeError('Options collection accepts an option or optgroup');
            if (element.contains(select))
                throw new DOMException('The option contains the select element', 'HierarchyRequestError');
            if (before === element) return;
            let reference = null;
            if (before instanceof HTMLElement) {
                if (before === select || !select.contains(before))
                    throw new DOMException('Reference element is not in the select', 'NotFoundError');
                reference = before;
            } else if (before !== null && before !== undefined) {
                const index = (+before) | 0;
                reference = collectionItems(this)[index] || null;
            }
            (reference?.parentNode || select).insertBefore(element, reference);
        }
        remove(index) {
            if (arguments.length === 0) throw new TypeError('An option index is required');
            collectionItems(this)[(+index) | 0]?.remove();
        }
        get [Symbol.toStringTag]() { return 'HTMLOptionsCollection'; }
    }
    const optionCollectionOwners = new WeakMap();
    const liveOptionsCollection = (select, resolve) => {
        const setIndex = (index, option) => {
            if (option !== null && !(option instanceof HTMLOptionElement))
                throw new TypeError('Options collection indexed value must be an option or null');
            const options = resolve();
            if (option === null) {
                options[index]?.remove();
                return;
            }
            if (option.contains(select))
                throw new DOMException('The option contains the select element', 'HierarchyRequestError');
            if (index < options.length) {
                options[index].parentNode.replaceChild(option, options[index]);
            } else if (index < optionCollectionLimit) {
                const fragment = select.ownerDocument.createDocumentFragment();
                for (let blank = options.length; blank < index; blank++)
                    fragment.appendChild(select.ownerDocument.createElement('option'));
                fragment.appendChild(option);
                select.appendChild(fragment);
            }
        };
        const collection = liveHtmlCollection(resolve, HTMLOptionsCollection, setIndex);
        optionCollectionOwners.set(collection, select);
        return collection;
    };
