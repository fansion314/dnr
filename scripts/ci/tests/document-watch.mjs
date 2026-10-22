// Exercise the actual desktop bootstrap fragment from the pinned Deno patch.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const patch = readFileSync('integration/deno.patch', 'utf8');
const start = patch.indexOf('+  const contextualBindings = new Map();');
const end = patch.indexOf('   // Binding-call correlation:', start);
assert(start >= 0 && end > start, 'document context fragment is present');
const source = patch.slice(start, end).split('\n')
  .filter(line => line.startsWith('+'))
  .map(line => line.slice(1))
  .join('\n');

let current = true;
let unrefed = false;
const Deno = { unrefTimer(timer) { timer.unref(); unrefed = true; } };
const { bindingContext, watchDocuments, bindingInFlight, invalidateDocument } =
  new Function('Deno', 'op_desktop_is_document_current', 'op_desktop_reject_bind_call',
    `${source}\nreturn {bindingContext, watchDocuments, bindingInFlight, invalidateDocument};`)(
      Deno, () => current, () => {},
    );
const metadata = { protected: true, mainFrame: true, windowId: 1,
  documentId: 'first', url: 'http://127.0.0.1/' };
const saved = bindingContext(1, metadata);
bindingInFlight.set(1, { windowId: 1 });
watchDocuments();
bindingInFlight.delete(1);
await new Promise(resolve => setTimeout(resolve, 150));
assert(unrefed, 'document observation must not keep the process alive');
assert.equal(saved.signal.aborted, false);
current = false;
await new Promise(resolve => setTimeout(resolve, 150));
assert.equal(saved.currentDocument, false);
assert.equal(saved.signal.aborted, true,
  'a saved context must be aborted after its completed binding is released');

current = true;
const replacement = bindingContext(1, { ...metadata, documentId: 'second' });
watchDocuments();
assert.equal(replacement.signal.aborted, false);
invalidateDocument(1);
assert.equal(replacement.signal.aborted, true);
console.log('DNR_DOCUMENT_WATCH_OK');
