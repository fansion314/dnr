  // Stable logical windows own replaceable native instances. Native op fast-call
  // upgrades touch only the native prototype, never these lifecycle wrappers.
  const nativeConstructor = BrowserWindow;
  const privateDesktopClose = Symbol.for("Deno_privateDesktopClose");
  const windows = new Map(); // native instance id -> logical object
  const windowStates = new WeakMap();
  const stateKeys = new Set();
  let defaultStateKeyTaken = false;

  function policy(value = { mode: "delayed", delayMs: 300_000 }) {
    if (value === null || typeof value !== "object" || !["immediate", "keep", "delayed"].includes(value.mode)) {
      throw new TypeError("hiddenWindowPolicy.mode must be immediate, keep or delayed");
    }
    if (value.mode !== "delayed") return { mode: value.mode };
    const delayMs = value.delayMs ?? 300_000;
    if (!Number.isSafeInteger(delayMs) || delayMs < 0 || delayMs > 2_147_483_647) {
      throw new RangeError("delayMs must be an integer from 0 through 2147483647");
    }
    return { mode: "delayed", delayMs };
  }
  function closeBehavior(value = "hide") {
    if (value !== "hide" && value !== "destroy") throw new TypeError('closeBehavior must be "destroy" or "hide"');
    return value;
  }
  function stateOf(win) {
    const state = windowStates.get(win);
    if (!state) throw new TypeError("invalid BrowserWindow receiver");
    return state;
  }
  function active(win) {
    const state = stateOf(win);
    if (state.destroyed || !state.native || state.releasing) throw new Error("Window is released or destroyed; call show() before using its page");
    return state.native;
  }
  function cancelTimer(state) {
    if (state.timer !== undefined) clearTimeout(state.timer);
    state.timer = undefined;
  }
  function captureSize(win) {
    const state = stateOf(win);
    if (!state.native || state.releasing || state.destroyed) return;
    const [width, height, , , normal, exists] = op_desktop_window_metrics(state.native.windowId);
    if (!exists || !normal || width <= 0 || height <= 0) return;
    state.options.width = width;
    state.options.height = height;
    if (state.remember && state.key) op_desktop_record_window_size(state.key, width, height);
  }
  function createNative(win, state) {
    const native = new nativeConstructor(state.options);
    if (state.options.documentPolicy && !op_desktop_set_document_policy(native.windowId, state.options.documentPolicy.allowedOrigins)) {
      native[privateDesktopClose]();
      const reap = () => { if (op_desktop_window_metrics(native.windowId)[5]) setTimeout(reap, 10); };
      reap();
      throw new Error("The selected backend cannot enforce documentPolicy");
    }
    state.native = native;
    state.epoch++;
    windows.set(native.windowId, win);
    const [, , availableWidth, availableHeight] = op_desktop_window_metrics(native.windowId);
    const [width, height] = native.getSize();
    const w = availableWidth > 0 ? Math.min(width, availableWidth) : width;
    const h = availableHeight > 0 ? Math.min(height, availableHeight) : height;
    if (w !== width || h !== height) native.setSize(w, h);
    if (state.menu) native.setApplicationMenu(state.menu);
    // Install native bindings before navigating; callback registry uses logical ids.
    const callbacks = windowBindCallbacks.get(state.id);
    if (callbacks) for (const name of callbacks.keys()) native[privateDesktopBind](name);
    if (state.url !== undefined) native.navigate(state.url);
    if (state.wantVisible) native.show();
    if (state.wantFocus) native.focus();
  }
  function release(win, permanent = false) {
    const state = stateOf(win);
    cancelTimer(state);
    if (state.destroyed) return;
    if (state.releasing || !state.native) {
      if (permanent) {
        state.destroyed = true;
        state.wantVisible = false;
        stateKeys.delete(state.key);
        windowBindCallbacks.delete(state.id);
        contextualBindings.delete(state.id);
      }
      return;
    }
    captureSize(win);
    const native = state.native;
    const [x, y] = native.getPosition();
    state.options.x = x; state.options.y = y;
    native[privateDesktopClose]();
    if (!native.isClosed()) return; // WebGPU surface protection: native close only hid it.
    state.releasing = true;
    state.destroyed = permanent;
    state.wantVisible = false;
    state.wantFocus = false;
    windows.delete(native.windowId); // Drop late input/load/binding events from this generation.
    invalidateDocument(state.id);
    for (const reject of state.pending) reject(new Error("Window page was released"));
    state.pending.clear();
    if (permanent) {
      stateKeys.delete(state.key);
      windowBindCallbacks.delete(state.id);
      contextualBindings.delete(state.id);
    }
    const complete = () => {
      // CEF close is asynchronous. A referenced timer keeps the runtime alive
      // until the backend confirms teardown; show during closing is queued.
      const metrics = op_desktop_window_metrics(native.windowId);
      if (metrics[5]) { setTimeout(complete, 10); return; }
      state.native = null;
      state.releasing = false;
      if (!state.destroyed && state.wantVisible) {
        try { createNative(win, state); }
        catch (error) { state.wantVisible = false; console.error("dnr: window recreation failed", error); }
      }
    };
    complete();
  }
  function scheduleRelease(win) {
    const state = stateOf(win);
    cancelTimer(state);
    if (state.destroyed || !state.native || state.releasing || state.hiddenAt === null || state.policy.mode === "keep") return;
    if (state.policy.mode === "immediate") { release(win); return; }
    const remaining = Math.max(0, state.policy.delayMs - (performance.now() - state.hiddenAt));
    state.timer = setTimeout(() => { state.timer = undefined; release(win); }, remaining);
  }
  class OrigBW extends EventTarget {
    constructor(options = {}) {
      super();
      if (options === null || typeof options !== "object") throw new TypeError("window options must be an object");
      const hiddenPolicy = policy(options.hiddenWindowPolicy);
      let documentPolicy;
      if (options.documentPolicy !== undefined) {
        const candidate = options.documentPolicy;
        if (!candidate || typeof candidate !== "object" || Object.keys(candidate).some(k => k !== "allowedOrigins") ||
            !Array.isArray(candidate.allowedOrigins) || candidate.allowedOrigins.length < 1 || candidate.allowedOrigins.length > 32) {
          throw new TypeError("documentPolicy requires 1 through 32 allowedOrigins");
        }
        const origins = candidate.allowedOrigins.map(value => {
          if (typeof value !== "string" || /[\\\s]/.test(value)) throw new TypeError("Invalid document origin");
          const url = new URL(value);
          if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
            throw new TypeError("documentPolicy accepts only HTTP(S) origins");
          }
          return url.origin;
        });
        documentPolicy = Object.freeze({ allowedOrigins: Object.freeze([...new Set(origins)]) });
      }
      const behavior = closeBehavior(options.closeBehavior);
      if (options.rememberSize !== undefined && typeof options.rememberSize !== "boolean") throw new TypeError("rememberSize must be a boolean");
      let key = options.stateKey;
      if (key !== undefined && (typeof key !== "string" || key.length === 0 || key.length > 256)) throw new TypeError("stateKey must be a nonempty string of at most 256 characters");
      const ordinary = !options.noActivate;
      const takeDefault = ordinary && key === undefined && !defaultStateKeyTaken;
      if (takeDefault) key = "main";
      if (key !== undefined && stateKeys.has(key)) throw new TypeError(`Window stateKey already in use: ${key}`);
      const remember = options.rememberSize !== false && key !== undefined;
      const restored = remember ? op_desktop_saved_window_size(key) : [];
      const state = { options: { ...options, documentPolicy }, key, remember, policy: hiddenPolicy, behavior,
        native: null, id: 0, epoch: 0, hiddenAt: null, timer: undefined, releasing: false,
        destroyed: false, closing: false, wantVisible: true, wantFocus: false, pending: new Set(),
        url: undefined, menu: undefined };
      if (restored.length === 2) [state.options.width, state.options.height] = restored;
      windowStates.set(this, state);
      createNative(this, state);
      state.id = state.native.windowId;
      if (ordinary) defaultStateKeyTaken = true;
      if (key !== undefined) stateKeys.add(key);
    }
    get windowId() { return stateOf(this).id; }
    isClosed() { return stateOf(this).destroyed; }
    isVisible() {
      const s = stateOf(this);
      return !s.destroyed && !s.releasing && s.native !== null && s.native.isVisible();
    }
    getCloseBehavior() { return stateOf(this).behavior; }
    setCloseBehavior(value) { stateOf(this).behavior = closeBehavior(value); }
    getHiddenWindowPolicy() { return { ...stateOf(this).policy }; }
    setHiddenWindowPolicy(value) { stateOf(this).policy = policy(value); scheduleRelease(this); }
    close() {
      const s = stateOf(this);
      if (s.destroyed || s.closing) return;
      s.closing = true;
      try {
        if (!this.dispatchEvent(new Event("close", { cancelable: true })) || s.destroyed) return;
        if (s.behavior === "hide") this.hide(); else this.destroy();
      } finally { s.closing = false; }
    }
    destroy() { release(this, true); }
    hide() {
      const s = stateOf(this);
      if (s.destroyed) return;
      captureSize(this);
      s.wantVisible = false; s.wantFocus = false;
      if (s.hiddenAt === null) s.hiddenAt = performance.now();
      if (s.native && !s.releasing) s.native.hide();
      scheduleRelease(this);
    }
    show() {
      const s = stateOf(this);
      if (s.destroyed) return;
      cancelTimer(s); s.hiddenAt = null; s.wantVisible = true;
      if (s.releasing) return;
      if (!s.native) createNative(this, s); else s.native.show();
    }
    focus() {
      const s = stateOf(this);
      if (s.destroyed) return;
      s.wantFocus = true;
      this.show();
      if (s.native && !s.releasing) s.native.focus();
    }
    getDocumentPolicy() { return structuredClone(stateOf(this).options.documentPolicy ?? null); }
    navigate(url) {
      const s = stateOf(this);
      if (s.destroyed) throw new Error("Window is destroyed");
      if (typeof url !== "string") throw new TypeError("url must be a string");
      if (s.options.documentPolicy) {
        const parsed = new URL(url);
        if (parsed.username || parsed.password || !s.options.documentPolicy.allowedOrigins.includes(parsed.origin)) throw new TypeError("Navigation is outside documentPolicy");
        const previous = s.url;
        s.url = parsed.href;
        if (previous !== undefined && s.native && !s.releasing) {
          const visible = s.wantVisible, focus = s.wantFocus;
          release(this);
          if (s.native && !s.releasing) throw new Error("Protected document could not be released");
          s.wantVisible = visible; s.wantFocus = focus;
          if (!s.native && visible) createNative(this, s);
          return;
        }
      } else { s.url = url; }
      if (s.native && !s.releasing) s.native.navigate(s.url);
    }
    executeJs(script) {
      const s = stateOf(this);
      let native;
      try { native = active(this); } catch (error) { return Promise.reject(error); }
      return new Promise((resolve, reject) => {
        s.pending.add(reject);
        Promise.resolve(native.executeJs(script)).then(resolve, reject).finally(() => s.pending.delete(reject));
      });
    }
    reload() {
      const s = stateOf(this);
      if (s.options.documentPolicy) { if (s.url !== undefined) this.navigate(s.url); }
      else active(this).reload();
    }
    openDevtools(options) { return active(this).openDevtools(options); }
    getNativeWindow() { return active(this).getNativeWindow(); }
    showContextMenu(...args) { return active(this).showContextMenu(...args); }
    setApplicationMenu(menu) {
      const s = stateOf(this); s.menu = structuredClone(menu);
      if (s.native && !s.releasing && !s.destroyed) s.native.setApplicationMenu(menu);
    }
    [privateDesktopBind](name) {
      const s = stateOf(this);
      if (s.destroyed) throw new Error("Window is destroyed");
      if (s.native && !s.releasing) s.native[privateDesktopBind](name);
    }
    [privateDesktopUnbind](name) {
      const s = stateOf(this);
      if (s.native && !s.releasing && !s.destroyed) s.native[privateDesktopUnbind](name);
    }
  }
  const BrowserWindowPrototype = OrigBW.prototype;
  for (const [method, names] of [
    ["setTitle", ["title"]], ["setSize", ["width", "height"]],
    ["setPosition", ["x", "y"]], ["setResizable", ["resizable"]],
    ["setAlwaysOnTop", ["alwaysOnTop"]], ["setOpacity", ["opacity"]],
  ]) {
    BrowserWindowPrototype[method] = function(...values) {
      const s = stateOf(this);
      if (s.destroyed) throw new Error("Window is destroyed");
      if (method === "setSize" && (values.length !== 2 || values.some(v => !Number.isInteger(v) || v <= 0 || v > 2_147_483_647))) throw new RangeError("window dimensions must be positive 32-bit integers");
      if (method === "setPosition" && (values.length !== 2 || values.some(v => !Number.isInteger(v) || v < -2_147_483_648 || v > 2_147_483_647))) throw new RangeError("window coordinates must be 32-bit integers");
      if ((method === "setResizable" || method === "setAlwaysOnTop") && typeof values[0] !== "boolean") throw new TypeError("window flag must be a boolean");
      if (method === "setTitle" && typeof values[0] !== "string") throw new TypeError("window title must be a string");
      if (method === "setOpacity" && (!Number.isFinite(values[0]) || values[0] < 0 || values[0] > 1)) throw new RangeError("window opacity must be in [0, 1]");
      if (s.native && !s.releasing) s.native[method](...values);
      names.forEach((name, i) => { s.options[name] = values[i]; });
      if (method === "setSize" && s.remember && s.key) op_desktop_record_window_size(s.key, values[0], values[1]);
    };
  }
  for (const [method, names, defaults] of [
    ["getSize", ["width", "height"], [800, 600]],
    ["getPosition", ["x", "y"], [0, 0]],
    ["isResizable", ["resizable"], [true]],
    ["isAlwaysOnTop", ["alwaysOnTop"], [false]],
    ["getOpacity", ["opacity"], [1]],
  ]) {
    BrowserWindowPrototype[method] = function() {
      const s = stateOf(this);
      if (s.native && !s.releasing && !s.destroyed) return s.native[method]();
      const values = names.map((name, i) => s.options[name] ?? defaults[i]);
      return values.length === 1 ? values[0] : values;
    };
  }
  Deno.BrowserWindow = OrigBW;
