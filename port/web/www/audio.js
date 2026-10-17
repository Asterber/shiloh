// AudioWorklet sink: plays interleaved stereo Float32Array chunks mixed by the worker (xf_pump), in arrival order.
// The worker sends its chunks through a MessagePort handed over as {port}. Underflow = silence; more than 1.5 s
// queued = the oldest chunks are dropped (a safety cap: the worker keeps the queue at its target, H1791; a hidden
// running tab, H1731, keeps 1 s).
// H1791–H1793: the sink reports back on the same port {c: frames taken from the queue so far (dropped ones included),
// a: seconds from now until the first frame not yet taken reaches the graph's output, b: seconds taken at once (0 =
// continuously)} — the worker tops the queue up from it and delays the picture by the latency (front::avsync).
class Queue {
  constructor(rate) { this.q = []; this.off = 0; this.queued = 0; this.max = rate * 1.5; this.consumed = 0; }
  push(b) {
    this.q.push(b);
    this.queued += b.length / 2;
    while (this.queued > this.max && this.q.length > 1) {
      const d = this.q.shift();
      const n = (d.length - this.off) / 2;
      this.queued -= n;
      this.consumed += n;
      this.off = 0;
    }
  }
  pull(L, R) {
    for (let i = 0; i < L.length; i++) {
      if (!this.q.length) { L[i] = 0; R[i] = 0; continue; }
      const b = this.q[0];
      L[i] = b[this.off]; R[i] = b[this.off + 1];
      this.off += 2; this.queued--; this.consumed++;
      if (this.off >= b.length) { this.q.shift(); this.off = 0; }
    }
  }
}

if (typeof AudioWorkletProcessor !== 'undefined') {
  class Sink extends AudioWorkletProcessor {
    constructor() {
      super();
      this.queue = new Queue(sampleRate);
      this.out = null;
      this.n = 0;
      this.port.onmessage = (e) => {
        if (e.data && e.data.port) {
          this.out = e.data.port;
          this.out.onmessage = (m) => this.queue.push(m.data);
        }
      };
    }
    process(_inputs, outputs) {
      const out = outputs[0];
      this.queue.pull(out[0], out[1] || out[0]);
      // every 8 render quanta (~21 ms): the quantum just taken plays now, the next frame after it
      if (this.out && ++this.n % 8 === 0) this.out.postMessage({ c: this.queue.consumed, a: out[0].length / sampleRate, b: 0 });
      return true;
    }
  }
  registerProcessor('xf-sink', Sink);
} else {
  // loaded as a classic script on the page: the ScriptProcessor fallback uses the same queue
  self.XfAudioQueue = Queue;
}
