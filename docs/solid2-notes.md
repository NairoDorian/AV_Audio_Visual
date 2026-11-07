# Solid 2 notes (confirmed against solid-js / @solidjs/web 2.0.0-rc.21)

Recorded during Track A step A1 so Studio code is not written from Solid 1 memory.

| Topic | Solid 2 rc.21 | Solid 1 habit that breaks |
| :-- | :-- | :-- |
| Mount | `import { render } from "@solidjs/web"` | `solid-js/web` |
| Vite plugin | `@solidjs/vite-plugin` 3.0.0-next.x; default export returns `Plugin[]`, so `plugins: [...solid()]` | `vite-plugin-solid` (old package name) |
| JSX types | `jsx: "preserve"`, `jsxImportSource: "@solidjs/web"`; TS 7 `tsc -b` handles it | `jsxImportSource: "solid-js"` |
| Classes | `class` takes a string, an object or an array: `class={["nav-item", { active }]}` | `classList` (removed) |
| `<For>` (default, keyed) | child is `(item: T, index: Accessor<number>)`: the **item is a plain value** | `item()` |
| `<For keyed={false}>` | child is `(item: Accessor<T>, index: number)` | |
| `<Show>` with a callback | `{(v) => v().field}`: the narrowed value is an accessor | |
| After mount | `onSettled(fn)`; `fn` may return a cleanup that runs on owner disposal | `onMount` + `onCleanup` inside it |
| Flow components | `For`, `Show`, `Switch`, `Match`, `Repeat`, `Errored`, `Loading` from `solid-js` | `ErrorBoundary`, `Suspense`, `Index` |

`solid-js` and `@solidjs/web` must be the identical version (lockstep group in the updater).
