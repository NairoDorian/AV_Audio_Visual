// Info-bubble text of the look controls (scene, ring components, raw-audio scope, colours).
import type { Help } from "../../components/InfoTip";

export const LOOK_HELP: Readonly<Record<string, Help>> = {
  scene: {
    text: "The visual style and its look: bars, or the concentric ring. A scene never contains audio or device settings, so it can be shared.",
  },
  displayCount: {
    text: "How many bars (or ring bins) are drawn. Each one shows the loudest of the analysis output bins it covers, so no peak is lost when there are fewer bars than bins.",
    effect: "More bars give a finer texture; fewer bars look bolder.",
    tip: "Preview only until scenes can be edited and saved.",
  },
  outerLine: {
    text: "A closed line around the ring at radius R x (1 + level): the spectrum pushing outward.",
  },
  innerLine: {
    text: "A closed line at radius R x (1 - level): the spectrum pulling toward the centre.",
  },
  outerBars: {
    text: "Radial bars from the ring's perimeter outward to the outer line.",
  },
  innerBars: {
    text: "Radial bars from the perimeter inward to the inner line.",
  },
  spanBars: {
    text: "Radial bars from the inner line through the perimeter to the outer line. They take precedence over outer and inner bars.",
  },
  baseCircle: {
    text: "The perimeter itself, drawn faintly as a reference.",
  },
  barScale: {
    text: "Bar length as a fraction of the distance to the lines. Below 1 the bars stop short, so the lines read as an outline around them.",
    tip: "0.9-0.95 leaves a thin, clean gap.",
  },
  ringFloor: {
    text: "Levels at or below the floor draw nothing; the rest is rescaled to the full height. It hides the always-present background energy.",
    effect: "Higher floor: a quieter, more graphic ring. Lower: more texture.",
  },
  ringGain: {
    text: "A fixed multiplier applied after the floor. It is never automatic, so the ring keeps a stable size.",
  },
  scopeEnabled: {
    text: "Draws the raw audio waveform: the newest samples, faded in and out at both ends. Inside the ring for the concentric style, across the view for the bar styles.",
  },
  horizontalMirror: {
    text: "Adds a time-reversed copy of every trace (right to left), superposed on the original.",
  },
  verticalMirror: {
    text: "Adds an upside-down (amplitude-inverted) copy of every trace.",
  },
  copies: {
    text: "Repeats every trace rotated around the centre: copy i is turned by i x 180 / copies degrees. 2 adds vertical traces, 3 gives one every 60 degrees.",
    effect: "Traces drawn = 1 x 2 (horizontal mirror) x 2 (vertical mirror) x copies.",
  },
  scopeGain: {
    text: "Amplitude of the traces: at 1 a full-scale sample reaches half the trace's height.",
    tip: "Raise it for quiet sources; the waveform is never normalized automatically.",
  },
  scopeSize: {
    text: "Length of the traces as a fraction of the inner diameter (ring) or of the width (bars).",
  },
  colors: {
    text: "Every line and bar can take its own colour. Unchecked, it follows the scene palette: a gradient for bars and lines, the first colour for the base circle and the last for the raw audio.",
  },
};
