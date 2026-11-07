// The latest-frame bus (blueprint 5.3, ADR-015). One poller per Studio window pulls raw wire
// bytes with `get_frame(lastSequence)` on requestAnimationFrame, capped at 30 Hz, one request
// in flight, and only while at least one subscriber exists and the document is visible. A
// header-only reply means "nothing new" and notifies nobody.
//
// The bus lives outside the reactive graph: canvases subscribe imperatively and draw in rAF.
// No signal or store is written at frame rate.

import { decodeFrame, WireError, type DecodedFrame } from "./wire";

export interface BusDeps {
  fetchFrame(lastSequence: number | null, wantBins: boolean): Promise<ArrayBuffer | Uint8Array>;
  requestFrame(cb: () => void): number;
  cancelFrame(id: number): void;
  isHidden(): boolean;
  now(): number;
}

export interface Subscriber {
  frame(frame: DecodedFrame): void;
  /** A wire error (e.g. protocol mismatch) or a transport failure. */
  error?(error: Error): void;
  wantBins?: boolean;
}

export interface BusStats {
  fetches: number;
  frames: number;
  headerOnly: number;
  errors: number;
}

export const MAX_POLL_HZ = 30;

export class FrameBus {
  private subs = new Set<Subscriber>();
  private rafId: number | null = null;
  private inFlight = false;
  private lastFetch = -Infinity;
  private lastSequence: number | null = null;
  private latestFrame: DecodedFrame | null = null;
  readonly stats: BusStats = { fetches: 0, frames: 0, headerOnly: 0, errors: 0 };

  constructor(
    private readonly deps: BusDeps,
    private readonly maxHz = MAX_POLL_HZ,
  ) {}

  get latest(): DecodedFrame | null {
    return this.latestFrame;
  }

  get running(): boolean {
    return this.rafId !== null;
  }

  subscribe(sub: Subscriber): () => void {
    this.subs.add(sub);
    if (this.latestFrame) sub.frame(this.latestFrame);
    this.schedule();
    return () => {
      this.subs.delete(sub);
      if (this.subs.size === 0) this.halt();
    };
  }

  private schedule(): void {
    if (this.rafId === null && this.subs.size > 0) {
      this.rafId = this.deps.requestFrame(() => this.tick());
    }
  }

  private halt(): void {
    if (this.rafId !== null) this.deps.cancelFrame(this.rafId);
    this.rafId = null;
  }

  private tick(): void {
    this.rafId = null;
    if (this.subs.size === 0) return;
    const now = this.deps.now();
    const due = now - this.lastFetch >= 1000 / this.maxHz - 1;
    if (!this.inFlight && due && !this.deps.isHidden()) {
      this.lastFetch = now;
      void this.fetch();
    }
    this.schedule();
  }

  private async fetch(): Promise<void> {
    this.inFlight = true;
    this.stats.fetches++;
    const wantBins = [...this.subs].some((s) => s.wantBins);
    try {
      const bytes = await this.deps.fetchFrame(this.lastSequence, wantBins);
      const frame = decodeFrame(bytes);
      if (frame.headerOnly) {
        this.stats.headerOnly++;
        return;
      }
      this.lastSequence = frame.header.sequence;
      this.latestFrame = frame;
      this.stats.frames++;
      for (const s of this.subs) s.frame(frame);
    } catch (e) {
      this.stats.errors++;
      const err = e instanceof Error ? e : new Error(String(e));
      for (const s of this.subs) s.error?.(err);
      if (e instanceof WireError && e.kind === "UnknownVersion") this.halt();
    } finally {
      this.inFlight = false;
    }
  }
}
