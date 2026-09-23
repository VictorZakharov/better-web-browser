// Canvas helpers share a lexical scope in both Window and Worker. The DOM canvas
// class is private here: workers expose OffscreenCanvas, never HTMLElement.
(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    class HTMLElement {}
