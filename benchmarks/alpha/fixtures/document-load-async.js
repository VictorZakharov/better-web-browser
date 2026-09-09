record('async');
const image = document.createElement('img');
image.src = 'assets/landscape.svg?delay_ms=1500&late=1';
image.onload = () => record('late');
document.body.appendChild(image);
