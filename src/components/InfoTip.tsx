// An info bubble: a small (i) button that opens an explanation in the browser's top layer
// (the Popover API), so no panel, scroll container or overflow can clip it. Opens on hover after a
// short delay, on keyboard focus and on click (click pins it open until Escape or a click
// elsewhere). Positioned next to its button and kept inside the viewport.
import { createSignal, For, onSettled, Show } from "solid-js";

export interface Help {
  /** One or two sentences: what the setting is. */
  text: string;
  /** What changes on screen when you move it. */
  effect?: string;
  /** A practical recommendation. */
  tip?: string;
}

let nextId = 0;
const HOVER_DELAY_MS = 250;
const GAP = 8;

export default function InfoTip(props: {
  title: string;
  help: Help;
  /** Extra facts shown under the text, e.g. "Default: 0.93" or "Range: 0 - 1". */
  facts?: ReadonlyArray<string>;
}) {
  const id = `info-${++nextId}`;
  let button: HTMLButtonElement | undefined;
  let bubble: HTMLDivElement | undefined;
  const [pinned, setPinned] = createSignal(false);
  let hoverTimer = 0;

  const place = () => {
    if (!button || !bubble) return;
    const b = button.getBoundingClientRect();
    const w = bubble.offsetWidth;
    const h = bubble.offsetHeight;
    const vw = document.documentElement.clientWidth;
    const vh = document.documentElement.clientHeight;
    let left = b.left + b.width / 2 - w / 2;
    left = Math.max(GAP, Math.min(left, vw - w - GAP));
    // Below the button, or above it when there is no room below.
    let top = b.bottom + GAP;
    if (top + h > vh - GAP) top = Math.max(GAP, b.top - h - GAP);
    bubble.style.left = `${Math.round(left)}px`;
    bubble.style.top = `${Math.round(top)}px`;
  };

  const show = () => {
    if (!bubble || bubble.matches(":popover-open")) return;
    bubble.showPopover();
    place();
  };
  const hide = () => {
    clearTimeout(hoverTimer);
    if (!pinned() && bubble?.matches(":popover-open")) bubble.hidePopover();
  };

  const onToggle = (e: Event) => {
    if ((e as ToggleEvent).newState === "closed") setPinned(false);
  };

  onSettled(() => {
    bubble?.addEventListener("toggle", onToggle);
    window.addEventListener("resize", place);
    return () => {
      clearTimeout(hoverTimer);
      bubble?.removeEventListener("toggle", onToggle);
      window.removeEventListener("resize", place);
    };
  });

  return (
    <>
      <button
        ref={(e) => (button = e)}
        type="button"
        class={["info", { pinned: pinned() }]}
        aria-label={`About ${props.title}`}
        aria-describedby={id}
        aria-expanded={pinned() ? "true" : "false"}
        onPointerEnter={() => {
          clearTimeout(hoverTimer);
          hoverTimer = window.setTimeout(show, HOVER_DELAY_MS);
        }}
        onPointerLeave={hide}
        onFocus={show}
        onBlur={hide}
        onClick={(e) => {
          e.preventDefault();
          if (pinned()) {
            setPinned(false);
            bubble?.hidePopover();
          } else {
            show();
            setPinned(true);
          }
        }}
      >
        i
      </button>
      <div ref={(e) => (bubble = e)} id={id} class="bubble" popover="auto" role="tooltip">
        <strong>{props.title}</strong>
        <p>{props.help.text}</p>
        <Show when={props.help.effect}>
          <p>
            <span class="bubble-label">On screen</span> {props.help.effect}
          </p>
        </Show>
        <Show when={props.help.tip}>
          <p>
            <span class="bubble-label">Tip</span> {props.help.tip}
          </p>
        </Show>
        <Show when={props.facts?.length}>
          <ul class="bubble-facts">
            <For each={props.facts ?? []}>{(f) => <li>{f}</li>}</For>
          </ul>
        </Show>
      </div>
    </>
  );
}
