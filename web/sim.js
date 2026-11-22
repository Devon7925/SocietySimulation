// Thin wrapper around the simulation's WebAssembly module. Works in the
// browser and in Node. The module speaks JSON: sim.call({ cmd: 'step', slots: 4 }).

const enc = new TextEncoder();
const dec = new TextDecoder();

export class Sim {
  constructor(instance) {
    this.x = instance.exports;
  }

  /** Load from a URL (browser) or from bytes (Node). */
  static async load(source) {
    let instance;
    if (typeof source === 'string' || source instanceof URL) {
      const res = await fetch(source);
      if (!res.ok) throw new Error(`could not fetch the simulation (${res.status})`);
      ({ instance } = await WebAssembly.instantiate(await res.arrayBuffer(), {}));
    } else {
      ({ instance } = await WebAssembly.instantiate(source, {}));
    }
    return new Sim(instance);
  }

  /** Send one command; returns the parsed reply. Throws if the simulation reports an error. */
  call(command) {
    const reply = this.raw(command);
    if (!reply.ok) throw new Error(reply.error || 'simulation error');
    return reply;
  }

  /** Like call(), but returns { ok: false, error } instead of throwing. */
  raw(command) {
    const bytes = enc.encode(JSON.stringify(command));
    const ptr = this.x.alloc(bytes.length);
    new Uint8Array(this.x.memory.buffer, ptr, bytes.length).set(bytes);
    this.x.call(ptr, bytes.length);
    this.x.dealloc(ptr, bytes.length);
    const out = new Uint8Array(this.x.memory.buffer, this.x.out_ptr(), this.x.out_len());
    return JSON.parse(dec.decode(out));
  }
}
