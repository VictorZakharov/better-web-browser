import { answer } from './module-shared.js?delay_ms=120';
await new Promise(resolve => setTimeout(resolve, 30));
export { answer };
export const settled = true;
