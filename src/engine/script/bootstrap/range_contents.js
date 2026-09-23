    // DOM §5: a Range can partially contain its edge descendants while fully
    // containing the siblings between them. Keep those three classes separate.
    // https://dom.spec.whatwg.org/#concept-range-extract
    const rangeInclusiveAncestor = (ancestor, node) => {
        for (let current = node; current; current = current.parentNode)
            if (current === ancestor) return true;
        return false;
    };
    const rangeContained = (node, start, end) => {
        const parent = node.parentNode;
        if (!parent) return false;
        const index = nodeIndex(node);
        return compareBoundaries(start.node, start.offset, parent, index) <= 0 &&
            compareBoundaries(parent, index + 1, end.node, end.offset) <= 0;
    };
    const rangeIntersects = (node, start, end) => {
        const parent = node.parentNode, index = nodeIndex(node);
        return parent && compareBoundaries(start.node, start.offset, parent, index + 1) < 0 &&
            compareBoundaries(parent, index, end.node, end.offset) < 0;
    };
    const rangeCommonAncestor = (start, end) => {
        let node = start.node;
        while (node && !rangeInclusiveAncestor(node, end.node)) node = node.parentNode;
        return node;
    };
    const rangeEdgeChild = (common, boundaryNode) => {
        let child = boundaryNode;
        while (child.parentNode !== common) child = child.parentNode;
        return child;
    };
    const rangeDirectParts = (start, end) => {
        const common = rangeCommonAncestor(start, end);
        const first = rangeInclusiveAncestor(start.node, end.node) ? null :
            rangeEdgeChild(common, start.node);
        const last = rangeInclusiveAncestor(end.node, start.node) ? null :
            rangeEdgeChild(common, end.node);
        const contained = Array.from(common.childNodes)
            .filter(node => rangeContained(node, start, end));
        if (contained.some(node => node instanceof DocumentType))
            throw new DOMException('A DocumentType cannot be copied into a fragment', 'HierarchyRequestError');
        return { first, last, contained };
    };
    const rangeCopyPart = (fragment, child, start, end, extract) => {
        if (!child) return;
        if (child instanceof CharacterData) {
            const clone = child.cloneNode(false);
            const from = start.node === child ? start.offset : 0;
            const to = end.node === child ? end.offset : child.length;
            clone.data = child.data.slice(from, to);
            fragment.appendChild(clone);
            if (extract) child.replaceData(from, to - from, '');
            return;
        }
        const clone = child.cloneNode(false);
        fragment.appendChild(clone);
        clone.appendChild(rangeCopyFragment(start, end, extract));
    };
    const rangeCopyFragment = (start, end, extract) => {
        const fragment = document.createDocumentFragment();
        if (start.node === end.node && start.offset === end.offset) return fragment;
        if (start.node === end.node && start.node instanceof CharacterData) {
            rangeCopyPart(fragment, start.node, start, end, extract);
            return fragment;
        }
        const { first, last, contained } = rangeDirectParts(start, end);
        if (first) rangeCopyPart(fragment, first, start,
            { node: first, offset: rangeNodeLength(first) }, extract);
        for (const child of contained) fragment.appendChild(extract ? child : child.cloneNode(true));
        if (last) rangeCopyPart(fragment, last, { node: last, offset: 0 }, end, extract);
        return fragment;
    };
    const rangeCollapseAfterDelete = (start, end) => {
        if (rangeInclusiveAncestor(start.node, end.node)) return { ...start };
        let reference = start.node;
        while (!rangeInclusiveAncestor(reference.parentNode, end.node))
            reference = reference.parentNode;
        return { node: reference.parentNode, offset: nodeIndex(reference) + 1 };
    };
    const rangeCopyContents = (range, extract) => {
        const start = { ...range.__start }, end = { ...range.__end };
        if (!extract) return rangeCopyFragment(start, end, false);
        if (range.collapsed) return document.createDocumentFragment();
        const collapse = rangeCollapseAfterDelete(start, end);
        // The standard collapses the live range before the selected nodes move.
        range.__start = range.__end = collapse;
        return rangeCopyFragment(start, end, true);
    };
    const rangeTopContained = (parent, start, end, result) => {
        for (const child of Array.from(parent.childNodes)) {
            if (rangeContained(child, start, end)) result.push(child);
            else if (child.childNodes.length && rangeIntersects(child, start, end))
                rangeTopContained(child, start, end, result);
        }
    };
    const rangeDeleteContents = range => {
        if (range.collapsed) return;
        const start = { ...range.__start }, end = { ...range.__end };
        if (start.node === end.node && start.node instanceof CharacterData) {
            start.node.replaceData(start.offset, end.offset - start.offset, '');
            range.__end = range.__start;
            return;
        }
        const contained = [];
        rangeTopContained(rangeCommonAncestor(start, end), start, end, contained);
        const collapse = rangeCollapseAfterDelete(start, end);
        range.__start = range.__end = collapse;
        if (start.node instanceof CharacterData)
            start.node.replaceData(start.offset, start.node.length - start.offset, '');
        for (const node of contained) node.remove();
        if (end.node instanceof CharacterData)
            end.node.replaceData(0, end.offset, '');
    };
