(() => {
    const itemCount = 1500;
    const passes = 30;

    function buildRow(key, index, pass, previous) {
        const state = (index + pass) % 4;
        const label = "Item " + key + " / pass " + pass;
        const classes = ["result", state === 0 ? "selected" : "", index % 2 ? "odd" : "even"]
            .filter(Boolean)
            .join(" ");
        return {
            key,
            label,
            classes,
            score: ((previous ? previous.score : index) * 33 + pass + state) & 0x7fffffff,
            attributes: {
                role: "row",
                state: String(state),
                position: String(index),
            },
            children: [
                { type: "title", text: label },
                { type: "metadata", text: classes + ":" + state },
            ],
        };
    }

    let rows = [];
    let checksum = 0;
    for (let pass = 0; pass < passes; pass++) {
        const previousByKey = new Map();
        for (let index = 0; index < rows.length; index++) {
            previousByKey.set(rows[index].key, rows[index]);
        }

        const nextRows = new Array(itemCount);
        for (let index = 0; index < itemCount; index++) {
            const key = "row-" + ((index * 37 + pass * 17) % itemCount);
            const previous = previousByKey.get(key);
            const row = buildRow(key, index, pass, previous);
            if (
                previous &&
                (previous.label !== row.label ||
                    previous.classes !== row.classes ||
                    previous.attributes.state !== row.attributes.state)
            ) {
                checksum = (checksum + 7) | 0;
            }
            checksum =
                (checksum +
                    row.score +
                    row.children.length +
                    row.label.charCodeAt((index + pass) % row.label.length)) |
                0;
            nextRows[index] = row;
        }
        rows = nextRows;
    }

    return checksum | 0;
})()
