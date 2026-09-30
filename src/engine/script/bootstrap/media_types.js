    // Source selection and script queries share the contained decoder's matrix.
    // MIME claims do not imply MSE, recording, or unsupported WebM codecs.
    // Template conversion uses Web IDL's ToString behavior, including Symbol rejection.
    const mediaTypeString = value => `${value}`;
    const mediaElementBrands = new WeakSet();
    const supportedMediaType = type => host('mediaCanPlayType', mediaTypeString(type));
