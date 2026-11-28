import init, { Guest } from "./guest.js";

const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const report = state => postMessage({state});
const MAX_RESPONSE = 256 * 1024;

// TCP chunks arriving as WebSocket messages can split or combine Noise frames.
class Socket {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.ws.binaryType = "arraybuffer";
    this.queue = [];
    this.size = 0;
    this.bytes = new Uint8Array();
    this.ws.onmessage = ({data}) => {
      const size = typeof data === "string" ? data.length : data.byteLength;
      if (!size || (typeof data === "string" && size > 8192) ||
          this.size + size > 2 * MAX_RESPONSE || this.queue.length >= 256) {
        this.close();
        return;
      }
      this.size += size;
      this.queue.push(data);
      this.wake?.();
    };
    this.ws.onclose = this.ws.onerror = () => this.close();
  }

  close() {
    this.closed = true;
    this.queue = [];
    this.ws.close();
    this.wake?.();
  }

  async next(deadline) {
    const remaining = deadline - performance.now();
    if (remaining <= 0) throw Error("timeout");
    if (!this.queue.length && !this.closed) {
      await new Promise(resolve => {
        const timer = setTimeout(resolve, remaining);
        this.wake = () => { clearTimeout(timer); resolve(); };
      });
      this.wake = null;
    }
    if (this.closed || !this.queue.length) throw Error("offline");
    const chunk = this.queue.shift();
    this.size -= typeof chunk === "string" ? chunk.length : chunk.byteLength;
    return chunk;
  }

  async record(deadline) {
    while (true) {
      if (this.bytes.length >= 2) {
        const n = new DataView(this.bytes.buffer, this.bytes.byteOffset).getUint16(0);
        if (!n) throw Error("invalid");
        if (this.bytes.length >= n + 2) {
          const record = this.bytes.subarray(2, n + 2);
          this.bytes = this.bytes.subarray(n + 2);
          return record;
        }
      }
      const part = await this.next(deadline);
      if (!(part instanceof ArrayBuffer)) throw Error("invalid");
      const joined = new Uint8Array(this.bytes.length + part.byteLength);
      joined.set(this.bytes);
      joined.set(new Uint8Array(part), this.bytes.length);
      this.bytes = joined;
    }
  }

  async response(guest, deadline, limit) {
    // The u32 prefix itself may span encrypted records.
    const header = new Uint8Array(4);
    let position = 0, body;
    while (!body || position < body.length) {
      let part = guest.open(await this.record(deadline));
      if (!body) {
        const take = Math.min(4 - position, part.length);
        header.set(part.subarray(0, take), position);
        position += take;
        if (position < 4) continue;
        const length = new DataView(header.buffer).getUint32(0);
        if (!length || length > limit) throw Error("invalid");
        body = new Uint8Array(length);
        part = part.subarray(take);
        position = 0;
      }
      body.set(part, position);
      position += part.length;
    }
    return body;
  }

  async admit(guest, enrollment) {
    const deadline = performance.now() + 5000;
    const challenge = await this.next(deadline);
    if (typeof challenge !== "string") throw Error("invalid");
    this.ws.send(guest.hello(challenge, enrollment));
    const status = JSON.parse(await this.next(deadline));
    if (status.t !== "status" || status.status !== "ok") {
      throw Error(status.status === "offline" ? "unavailable" : "offline");
    }
    this.ws.send(guest.begin());
    this.ws.send(guest.admit(await this.record(deadline), enrollment));
    const reply = JSON.parse(new TextDecoder().decode(await this.response(guest, deadline, 128)));
    if (reply.version !== 0) throw Error("version");
    if (reply.type !== (enrollment ? "registered" : "ready")) throw Error("invalid");
  }
}

// Wait for the main thread before sending another transcript to a busy page.
function present(transcript) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      self.onmessage = null;
      reject(Error("timeout"));
    }, 5000);
    self.onmessage = ({data}) => {
      if (data !== "rendered") return;
      clearTimeout(timer);
      self.onmessage = null;
      resolve();
    };
    postMessage({state:"live", transcript});
  });
}

self.onmessage = async ({data: {landing, fragment}}) => {
  self.onmessage = null;
  let previouslyLive = false;
  try {
    await init();
    for (let attempt = 0; attempt < 3; attempt++) {
      const guest = new Guest(landing, fragment);
      const expiry = Number(guest.expiry());
      let socket;
      try {
        if (Date.now() >= expiry) { report("expired"); return; }
        postMessage({expiry});
        if (attempt) { report("reconnecting"); await pause(500 * attempt); }
        for (const enrollment of [true, false]) {
          socket = new Socket(guest.url());
          await socket.admit(guest, enrollment);
          if (enrollment) socket.close();
        }
        while (Date.now() < expiry) {
          socket.ws.send(guest.read());
          const update = guest.update(await socket.response(guest, performance.now() + 5000, MAX_RESPONSE));
          if (update) {
            const transcript = JSON.parse(update);
            if (["exited", "failed"].includes(transcript.status.state)) { report("ended"); return; }
            previouslyLive = true;
            await present(transcript);
          }
          await pause(500);
        }
        report("expired");
        return;
      } catch (err) {
        if (["invalid", "version"].includes(err.message)) throw err;
        if (attempt === 2) {
          report(previouslyLive && err.message === "unavailable" ? "withdrawn" : "offline");
        } else report("reconnecting");
      } finally {
        socket?.close();
        guest.free();
      }
    }
  } catch (err) {
    report(err.message === "version" ? "version" : "invalid");
  }
};
