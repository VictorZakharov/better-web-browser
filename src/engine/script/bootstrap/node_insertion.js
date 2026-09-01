    // DOM's pre-insert validity algorithm is shared by Node mutation methods and
    // ParentNode.replaceChildren(). Keep it ahead of all host mutations so a rejected
    // operation cannot partially detach or adopt nodes.
    // https://dom.spec.whatwg.org/#concept-node-ensure-pre-insertion-validity
    const isHostIncludingInclusiveAncestor = (ancestor, node) => {
        for (let current = node; current;) {
            if (current === ancestor) return true;
            if (current.parentNode) current = current.parentNode;
            else if (current.nodeType === 11 && current.host) current = current.host;
            else current = null;
        }
        return false;
    };
    const ensurePreInsertionValidity = (node, parent, child = null, childrenToExclude = []) => {
        const hierarchyError = message => {
            throw new DOMException(message, 'HierarchyRequestError');
        };
        if (![1, 9, 11].includes(parent.nodeType))
            hierarchyError('This node type cannot have children');
        if (isHostIncludingInclusiveAncestor(node, parent))
            hierarchyError('The inserted node contains its parent');
        if (child !== null && child.parentNode !== parent)
            throw new DOMException('The reference node is not a child', 'NotFoundError');

        const type = node.nodeType;
        if (![1, 3, 4, 7, 8, 10, 11].includes(type))
            hierarchyError('This node type cannot be inserted');
        if (parent.nodeType !== 9) {
            if (type === 10) hierarchyError('A doctype can only be inserted into a document');
            return;
        }
        if (type === 3 || type === 4)
            hierarchyError('Text cannot be inserted directly into a document');
        if (type === 7 || type === 8) return;

        const excluded = new Set(childrenToExclude);
        const parentChildren = [...parent.childNodes];
        const remaining = parentChildren.filter(candidate => !excluded.has(candidate));
        if (type === 11) {
            const fragmentChildren = [...node.childNodes];
            if (fragmentChildren.filter(candidate => candidate.nodeType === 1).length > 1 ||
                fragmentChildren.some(candidate => candidate.nodeType === 3 || candidate.nodeType === 4))
                hierarchyError('A document fragment cannot create an invalid document tree');
            if (!fragmentChildren.some(candidate => candidate.nodeType === 1)) return;
        }

        if (type === 1 || type === 11) {
            const childIndex = child === null ? -1 : parentChildren.indexOf(child);
            const doctypeFollowsChild = childIndex >= 0 && parentChildren.some((candidate, index) =>
                index > childIndex && candidate.nodeType === 10 && !excluded.has(candidate));
            if (remaining.some(candidate => candidate.nodeType === 1) || doctypeFollowsChild ||
                (child?.nodeType === 10 && !excluded.has(child)))
                hierarchyError('A document can only contain one element after its doctype');
            return;
        }

        const childIndex = child === null ? -1 : parentChildren.indexOf(child);
        const elementPrecedesChild = childIndex >= 0 && parentChildren.some((candidate, index) =>
            index < childIndex && candidate.nodeType === 1 && !excluded.has(candidate));
        if (remaining.some(candidate => candidate.nodeType === 10) || elementPrecedesChild ||
            (child === null && remaining.some(candidate => candidate.nodeType === 1)))
            hierarchyError('A document can only contain one doctype before its element');
    };
