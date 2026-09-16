    windowObject.URL.createObjectURL = value => createObjectUrl(value);
    windowObject.URL.revokeObjectURL = value => revokeObjectUrl(value);
