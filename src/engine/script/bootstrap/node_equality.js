    // Web IDL nullable Node conversion uses private brands, including same-origin
    // foreign realms. Equality must not invoke overridden DOM accessors.
    const comparisonNode = value => {
        const id = host('nodeHandle', value);
        if (id) return {id};
        const attribute = host('attributeComparisonData', value);
        if (attribute) return {attribute};
        throw new TypeError('The value is not a Node');
    };
    const compareNodes = (left, right, identity) => {
        const a = comparisonNode(left);
        if (right == null) return false;
        const b = comparisonNode(right);
        if (identity) return left === right;
        if (a.id && b.id) return host('nodesEqual', a.id, b.id);
        if (!a.attribute || !b.attribute) return false;
        return a.attribute[0] === b.attribute[0] && a.attribute[1] === b.attribute[1]
            && a.attribute[2] === b.attribute[2];
    };
