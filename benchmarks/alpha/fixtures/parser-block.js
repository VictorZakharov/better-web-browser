parserMark('block-' + (document.querySelector('#tail') === null) + '-' + document.readyState);
Promise.resolve().then(() => parserMark('micro'));
document.currentScript.onload = () => parserMark('block-load');
