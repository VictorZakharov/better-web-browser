    const fontEventFaces = new WeakMap();
    const fontEventFreeze = Object.freeze;
    class FontFaceSetLoadEvent extends Event {
        constructor(type, init = {}) {
            if (!arguments.length) throw new TypeError('FontFaceSetLoadEvent requires a type');
            type = fontString(type);
            if (init !== null && typeof init !== 'object' && typeof init !== 'function')
                throw new TypeError('Expected an event dictionary');
            // Inherited EventInit members precede fontfaces in lexical order.
            // Snapshot once before invoking the parent event constructor.
            const flags = fontCreate(null);
            flags.bubbles = !!init?.bubbles;
            flags.cancelable = !!init?.cancelable;
            flags.composed = !!init?.composed;
            const source = init?.fontfaces;
            const faces = source === undefined ? [] : idlSequence(source, face => {
                fontState(face);
                return face;
            });
            super(type, flags);
            fontWeakSet(fontEventFaces, this, fontEventFreeze(faces));
        }
        get fontfaces() {
            const faces = fontWeakGet(fontEventFaces, this);
            if (!faces) throw new TypeError('Illegal invocation');
            return faces;
        }
    }
    Object.defineProperty(FontFaceSetLoadEvent.prototype, 'fontfaces', {
        ...Object.getOwnPropertyDescriptor(FontFaceSetLoadEvent.prototype, 'fontfaces'),
        enumerable:true
    });
