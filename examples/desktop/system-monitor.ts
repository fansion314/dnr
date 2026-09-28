const supported = Deno.desktop.startSystemMonitoring();
if (typeof supported.lock !== 'boolean' || typeof supported.suspend !== 'boolean' ||
    typeof supported.resume !== 'boolean' || typeof supported.idleTime !== 'boolean') throw new Error('invalid capabilities');
const idle = Deno.desktop.getSystemIdleTime();
if (supported.idleTime && (idle === null || !Number.isFinite(idle) || idle < 0)) throw new Error('invalid idle time');
console.log(JSON.stringify({supported,idleTimeAvailable:idle !== null}));
await new Promise(resolve => setTimeout(resolve,250));
Deno.desktop.stopSystemMonitoring();
Deno.desktop.stopSystemMonitoring();
console.log('DNR_SYSTEM_MONITOR_OK');
