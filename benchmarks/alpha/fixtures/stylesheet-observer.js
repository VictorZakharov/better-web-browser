mark('ASYNC');
document.getElementById('blocking').addEventListener('load', () => mark('LINK_LOAD'));
document.getElementById('blocking').addEventListener('error', () => mark('LINK_ERROR'));
