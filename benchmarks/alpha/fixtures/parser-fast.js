parserMark('async-' + (document.querySelector('#tail') === null));
document.querySelector('#fast').className = 'panel pass';
document.querySelector('#fast').textContent = 'Async content ready while parser waits';
