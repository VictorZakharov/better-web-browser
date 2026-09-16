trace.push('external');
document.write('<span id=external-child>Written external scripts keep the insertion point.</span>');
trace.push(document.getElementById('external-child') ? 'external child' : 'missing external child');
