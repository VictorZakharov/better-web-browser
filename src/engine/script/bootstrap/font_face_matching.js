    const matchFontFaces = (set, shorthand, text) => {
        const faces=[], descriptors=[];
        for(const face of fontSet(set).faces) {
            const state=fontState(face),weight=fontWeight(state.descriptors.weight),
                italic=fontStyle(state.descriptors.style);
            if(weight===null||italic===null||!fontHost('fontFaceFamilyValid',state.descriptors.family))continue;
            faces.push(face);descriptors.push([state.descriptors.family,weight,italic,state.descriptors.unicodeRange]);
        }
        const indices=fontHost('fontFaceMatch',shorthand,descriptors,text);
        if(!indices)throw new DOMException('Invalid font shorthand','SyntaxError');
        // Syntax is validated even when there are no characters to draw.
        if(!text.length)return [];
        return indices.map(index=>faces[index]);
    };
