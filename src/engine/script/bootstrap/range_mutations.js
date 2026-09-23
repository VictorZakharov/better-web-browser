    // DOM Standard live-range mutation rules. Iterate weak references so detached
    // Range objects do not keep entire removed subtrees alive indefinitely.
    const forEachLiveRangeBoundary = callback => {
        for (const reference of liveRangeReferences) {
            const range = reference.deref();
            if (!range) { liveRangeReferences.delete(reference); continue; }
            callback(range.__start);
            callback(range.__end);
        }
    };
    const rangeAfterRemovingNode = (node, parent, index) => {
        forEachLiveRangeBoundary(boundary => {
            if (node === boundary.node || node.contains(boundary.node)) {
                boundary.node = parent;
                boundary.offset = index;
            } else if (boundary.node === parent && boundary.offset > index) {
                boundary.offset--;
            }
        });
    };
    const rangeAfterRemovingRecords = records => {
        // Fragment insertion removes several siblings; process old indices descending.
        const removals = records.filter(record => record.oldParent).map(record => ({
            node: record.node, parent: record.oldParent,
            index: record.oldIndex
        }));
        removals.sort((a, b) => b.index - a.index);
        for (const removal of removals)
            rangeAfterRemovingNode(removal.node, removal.parent, removal.index);
    };
    const rangeAfterInsertingNodes = (parent, index, count) => {
        if (!count) return;
        forEachLiveRangeBoundary(boundary => {
            if (boundary.node === parent && boundary.offset > index)
                boundary.offset += count;
        });
    };
    const rangeAfterReplacingData = (node, offset, removedLength, insertedLength) => {
        forEachLiveRangeBoundary(boundary => {
            if (boundary.node !== node) return;
            if (boundary.offset > offset + removedLength)
                boundary.offset += insertedLength - removedLength;
            else if (boundary.offset > offset) boundary.offset = offset;
        });
    };
    const rangeAfterSplittingText = (original, sibling, offset) => {
        forEachLiveRangeBoundary(boundary => {
            if (boundary.node === original && boundary.offset > offset) {
                boundary.node = sibling;
                boundary.offset -= offset;
            }
        });
    };
    const rangeBeforeMergingText = (kept, removed, appendedAt) => {
        const parent = removed.parentNode;
        const index = Array.from(parent.childNodes).indexOf(removed);
        forEachLiveRangeBoundary(boundary => {
            if (boundary.node === removed) {
                boundary.node = kept;
                boundary.offset += appendedAt;
            } else if (boundary.node === parent && boundary.offset === index) {
                boundary.node = kept;
                boundary.offset = appendedAt;
            }
        });
    };
