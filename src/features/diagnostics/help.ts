// Info-bubble text of the Audio and Diagnostics readouts.
import type { Help } from "../../components/InfoTip";

export const DIAG_HELP: Readonly<Record<string, Help>> = {
  Source: {
    text: "Where the audio comes from: system audio (what the computer plays), the microphone, a file or the demo track.",
  },
  Format: { text: "The source's sample rate and channel count. The analysis runs at this rate." },
  State: {
    text: "Silent: digital silence. Quiet: playing but below -60 dBFS. Audible: a normal level. Device lost: the device disappeared; pick the source again.",
  },
  "Input peak": {
    text: "The loudest raw sample over the last status second, in dB below full scale (0 dBFS).",
    tip: "Around -20 to -3 dBFS is typical for music; below -60 the display has little to show.",
  },
  Name: { text: "The application name and build." },
  Version: { text: "The application version." },
  Platform: {
    text: "Operating system and CPU architecture, and whether this is a debug or release build.",
  },
  "FFT / hop": {
    text: "The FFT size every layer runs at (grown to the longest window when needed) and the samples between analyses.",
  },
  "Layers / first window": {
    text: "Super FFT layers in use, the first layer's window in samples, and how many threads compute them.",
  },
  "Output bins": { text: "Values per frame sent to the displays (the analysis output bins)." },
  Peak: { text: "Frequency of the loudest output bin in the newest frame." },
  "Centroid / rolloff": {
    text: "Centroid: the power-weighted mean frequency (brightness). Rolloff: the frequency below which 85% of the power lies.",
  },
  "Flatness / flux": {
    text: "Flatness: 0 for a pure tone, 1 for white noise. Flux: how much the spectrum rose since the previous frame (onsets).",
  },
  "RMS / bass / mid / high": {
    text: "The window's RMS level in dBFS, and the power below 250 Hz, from 250 Hz to 4 kHz, and above 4 kHz.",
  },
  Generation: {
    text: "Increments every time a setting rebuilds the analysis. The audio stream is never restarted for it.",
  },
  Frames: { text: "Frames analysed since the source started." },
  "DSP avg / max": {
    text: "Time to analyse one hop, average and worst over the last second, in microseconds.",
    tip: "Keep the worst under the hop duration (10.7 ms at 512 samples, 48 kHz); under 2 ms is the budget.",
  },
  "Overruns / dropped": {
    text: "Times the analysis fell behind the capture and samples that were dropped as a result.",
    tip: "Should stay 0; if it grows, lower the layers or the FFT size.",
  },
  "Starved hops": {
    text: "Hops of silence published because the source delivered nothing (a paused device or no playback).",
  },
  Clock: {
    text: "How frame timestamps are made: from the sample count, or from the device's own clock.",
  },
};
