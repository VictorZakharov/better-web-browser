// Exercise the exact same ordinary API contract in a dedicated Worker.
importScripts('owned-transfer-contract.js');
self.onmessage = () => {
    try { postMessage({ok:true,result:exerciseTransfers()}); }
    catch (error) { postMessage({ok:false,error:String(error)}); }
};
