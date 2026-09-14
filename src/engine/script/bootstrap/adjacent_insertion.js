    // DOM insert-adjacent shares pre-insertion, adoption, mutation and CE reactions.
    // https://dom.spec.whatwg.org/#insert-adjacent
    const adjacentString = value => {
        if (typeof value === 'symbol') throw new TypeError('Cannot convert a Symbol to DOMString');
        return String(value);
    };
    const insertAdjacentNode = (element, position, node) => {
        // Only ASCII case folding is permitted; whitespace is not trimmed.
        position = position.replace(/[A-Z]/g, c => c.toLowerCase());
        switch (position) {
            case 'beforebegin':
                return element.parentNode === null ? null :
                    element.parentNode.insertBefore(node, element);
            case 'afterbegin':
                return element.insertBefore(node, element.firstChild);
            case 'beforeend':
                return element.insertBefore(node, null);
            case 'afterend':
                return element.parentNode === null ? null :
                    element.parentNode.insertBefore(node, element.nextSibling);
            default:
                throw new DOMException('Invalid adjacent insertion position', 'SyntaxError');
        }
    };
    Object.defineProperties(Element.prototype, {
        insertAdjacentElement: {
            configurable: true, enumerable: true, writable: true,
            value: function insertAdjacentElement(position, element) {
                if (!(this instanceof Element) || arguments.length < 2)
                    throw new TypeError('insertAdjacentElement requires an Element and two arguments');
                position = adjacentString(position);
                if (!(element instanceof Element)) throw new TypeError('Expected an Element');
                return insertAdjacentNode(this, position, element);
            }
        },
        insertAdjacentText: {
            configurable: true, enumerable: true, writable: true,
            value: function insertAdjacentText(position, data) {
                if (!(this instanceof Element) || arguments.length < 2)
                    throw new TypeError('insertAdjacentText requires an Element and two arguments');
                position = adjacentString(position);
                data = adjacentString(data);
                insertAdjacentNode(this, position, this.ownerDocument.createTextNode(data));
            }
        }
    });
