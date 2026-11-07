// A8 gates for the preview bus: header-only replies notify nobody, the poll rate is capped,
// one request is in flight at a time, and a Studio with no visible preview polls nothing.
import { describe, expect, test } from "bun:test";
import { FrameBus, type BusDeps } from "../src/lib/frame/bus";
import { encodeFrame, Flags, type FrameHeader } from "../src/lib/frame/wire";

const header = (sequence: number, flags = 0): FrameHeader => ({
  sequence,
  timestampLo: sequence * 10_667,
  timestampHi: 0,
  sampleRate: 48_000,
  fftSize: 2048,
  barCount: 3,
  flags,
  generation: 1,
});

/** A fake host: manual rAF queue, manual clock, scripted server. */
function harness(serverSequence: { value: number }) {
  let raf: Array<() => void> = [];
  let clock = 0;
  let hidden = false;
  const requests: Array<number | null> = [];
  const deps: BusDeps = {
    fetchFrame: async (last) => {
      requests.push(last);
      const seq = serverSequence.value;
      if (last === seq) return encodeFrame({ header: header(seq, Flags.HEADER_ONLY), bars: [] });
      return encodeFrame({ header: header(seq), bars: [0.1, 0.2, 0.3] });
    },
    requestFrame: (cb) => raf.push(cb),
    cancelFrame: () => {
      raf = [];
    },
    isHidden: () => hidden,
    now: () => clock,
  };
  return {
    deps,
    requests,
    setHidden: (h: boolean) => (hidden = h),
    pending: () => raf.length,
    /** Advance `ms` of 60 Hz display frames, letting fetches resolve between them. */
    async run(ms: number) {
      for (let t = 0; t < ms; t += 1000 / 60) {
        clock += 1000 / 60;
        const queue = raf;
        raf = [];
        for (const cb of queue) cb();
        await Promise.resolve();
        await Promise.resolve();
      }
    },
  };
}

describe("frame bus", () => {
  test("no subscriber: nothing is scheduled and nothing is fetched", async () => {
    const h = harness({ value: 1 });
    const bus = new FrameBus(h.deps);
    await h.run(1000);
    expect(bus.running).toBe(false);
    expect(h.requests).toHaveLength(0);
    expect(h.pending()).toBe(0);
  });

  test("polls at most 30 Hz and notifies only on new sequences", async () => {
    const server = { value: 1 };
    const h = harness(server);
    const bus = new FrameBus(h.deps);
    const seen: number[] = [];
    bus.subscribe({ frame: (f) => seen.push(f.header.sequence) });
    await h.run(1000);
    expect(h.requests.length).toBeGreaterThanOrEqual(25);
    expect(h.requests.length).toBeLessThanOrEqual(31);
    expect(seen).toEqual([1]);
    expect(bus.stats.headerOnly).toBe(h.requests.length - 1);
    server.value = 2;
    await h.run(100);
    expect(seen).toEqual([1, 2]);
  });

  test("the last unsubscribe stops polling", async () => {
    const h = harness({ value: 1 });
    const bus = new FrameBus(h.deps);
    const off = bus.subscribe({ frame: () => {} });
    await h.run(200);
    const before = h.requests.length;
    off();
    expect(bus.running).toBe(false);
    await h.run(1000);
    expect(h.requests.length).toBe(before);
  });

  test("a hidden document polls nothing until it is visible again", async () => {
    const h = harness({ value: 1 });
    const bus = new FrameBus(h.deps);
    bus.subscribe({ frame: () => {} });
    h.setHidden(true);
    await h.run(1000);
    expect(h.requests).toHaveLength(0);
    h.setHidden(false);
    await h.run(200);
    expect(h.requests.length).toBeGreaterThan(0);
  });

  test("a protocol mismatch is reported and stops the poller", async () => {
    const h = harness({ value: 1 });
    h.deps.fetchFrame = async () => {
      const bytes = encodeFrame({ header: header(1), bars: [0.5, 0.5, 0.5] });
      bytes[27] = 2; // wire version 2
      return bytes;
    };
    const bus = new FrameBus(h.deps);
    const errors: string[] = [];
    bus.subscribe({ frame: () => {}, error: (e) => errors.push(e.message) });
    await h.run(200);
    expect(errors[0]).toContain("UnknownVersion");
    expect(bus.running).toBe(false);
  });

  test("a late subscriber gets the latest frame immediately", async () => {
    const h = harness({ value: 5 });
    const bus = new FrameBus(h.deps);
    bus.subscribe({ frame: () => {} });
    await h.run(100);
    let got = -1;
    bus.subscribe({ frame: (f) => (got = f.header.sequence) });
    expect(got).toBe(5);
  });
});
