// The info-bubble text of every analysis setting, keyed like `params.ts`. A test checks that
// every parameter has an entry and every entry names a parameter, so a new setting cannot ship
// without an explanation.
import type { Help } from "../../components/InfoTip";

export const ANALYSIS_HELP: Readonly<Record<string, Help>> = {
  // --- Spectrum ---
  channelMode: {
    text: "Which audio channels feed the analysis. Mono mix averages every channel, so stereo music is analysed as one signal; left or right picks a single channel.",
    effect:
      "With a hard-panned instrument, mono mix halves its level; a single channel shows only that side.",
    tip: "Keep mono mix unless you want a left-only or right-only visual.",
  },
  rawBins: {
    text: "Outputs the FFT's own bins, untouched: N/2 + 1 values from 0 Hz to Nyquist on a linear axis. Scale, warp, interpolation, aggregation and output bins are then ignored.",
    effect: "The low end shrinks to a few bars on the left and the highs take most of the width.",
    tip: "Useful to inspect the raw analysis; for a musical display keep it off.",
  },
  scale: {
    text: "How frequencies are spread across the output bins. Logarithmic gives every octave the same width, like a piano; Mel, ERB and Bark follow how the ear groups frequencies; Chroma is a pitch axis; Melog blends Mel and Log.",
    effect: "Changes where the bass, mids and highs sit across the bars.",
    tip: "Logarithmic (default) suits music; Mel or Bark look more even on speech.",
  },
  displayMaxHz: {
    text: "The highest frequency shown, at the right end of the axis. It is clamped to half the sample rate (Nyquist).",
    effect: "Lowering it stretches the lows and mids over more bars and hides the top octave.",
    tip: "16-20 kHz drops the near-empty top of most music.",
  },
  outputBinsMode: {
    text: "Fixed outputs exactly 'Output bins' values; Auto outputs one value per FFT bin (N/2 + 1 of the FFT size).",
    effect:
      "Auto at FFT 32,768 sends 16,385 values per frame: the finest detail, at more CPU and bandwidth.",
    tip: "Fixed with 512-2048 bins is plenty for any display; scenes pool them to their own bar count.",
  },
  outputBins: {
    text: "How many values the analysis outputs per frame, spread along the chosen scale. Each scene then draws its own number of bars from them, keeping the loudest bin under each bar.",
    effect: "More bins give the ring and fine bars more detail; fewer bins look chunkier.",
    tip: "1,024 (default) covers rings of up to 1,024 bins without repeating values.",
  },
  warpBlend: {
    text: "Mixes a linear frequency axis (0) with the chosen scale (1), position by position.",
    effect: "Lower values give the highs more room and squeeze the bass; 1 is the pure scale.",
    tip: "0.93 (default) keeps the scale's shape with a slightly wider top end.",
  },
  warpInterp: {
    text: "How an output bin reads between two FFT bins: linear uses 2 neighbours, cubic (Catmull-Rom) uses 4 for smoother curves.",
    effect:
      "Cubic removes the faceted look of the low end when there are more output bins than FFT bins.",
    tip: "Use cubic with large output bin counts or small FFT sizes.",
  },
  warpAggregation: {
    text: "What an output bin reports when it covers several FFT bins (the coarse top of a log axis): off interpolates, peak takes the loudest, RMS takes the energy average.",
    effect: "Off can make narrow highs flicker or vanish; peak never drops one; RMS is calmer.",
    tip: "Peak (default) for visuals, RMS for measurement-like readings.",
  },
  logFloorHz: {
    text: "The lowest frequency of the logarithmic (and Mel + Log) axis, at the left end.",
    effect: "Raising it removes the sub-bass from the left and spreads the rest wider.",
    tip: "20 Hz shows everything audible; 30-40 Hz suits small speakers.",
  },
  windowLengthMode: {
    text: "Whether the first analysis window is set in samples or in milliseconds (converted with the source's sample rate).",
    tip: "Milliseconds keeps the same timing when the device rate changes (44.1 vs 48 kHz).",
  },
  windowSamples: {
    text: "Length of the first (shortest) FFT window. Each extra Super FFT layer doubles it. Short windows react fast but blur low notes; long windows resolve the bass but react slowly.",
    effect: "Smaller: snappier transients, smeared bass. Larger: crisper bass, softer hits.",
    tip: "1,024 at 48 kHz is 21 ms, a good reactive base for the layered FFT.",
  },
  windowMs: {
    text: "Length of the first FFT window in milliseconds; each extra layer doubles it.",
    tip: "About 21 ms equals 1,024 samples at 48 kHz.",
  },
  layers: {
    text: "The Super FFT: several analyses at once, each with twice the previous window, all at one FFT size and averaged. The short layer keeps it reactive, the long ones make the bass precise.",
    effect:
      "More layers sharpen low notes (two tones 12-18 Hz apart near 60 Hz become separable at 6 layers) at more CPU.",
    tip: "4 (default) is balanced; 6 for maximum bass detail. 1 is a classic single FFT.",
  },
  hopSamples: {
    text: "Samples between two analyses. The frame rate is the sample rate divided by the hop.",
    effect:
      "512 at 48 kHz is 93.75 analyses per second; halving the hop doubles the rate and the CPU.",
    tip: "Keep the analysis rate above your display's frame rate (512 for 60-90 Hz screens).",
  },
  waveformSamples: {
    text: "Length of the raw-audio scope sent with every frame: the newest samples multiplied by a half sine (0, 1, 0) so the trace fades at both ends.",
    effect:
      "Longer shows more of the waveform, shorter zooms in on the newest cycle. 0 turns the scope data off.",
    tip: "800 is one 60 fps frame of audio at 48 kHz.",
  },
  zeroPadding: {
    text: "Pads every window with zeros up to the FFT size before the transform. This interpolates the spectrum (smoother bars, more precise peaks); it does not add real resolution.",
    tip: "Keep it on; the long layers provide the real resolution.",
  },
  fftSize: {
    text: "The FFT length every layer is zero-padded to. It is shared by all layers so their bins line up, and grown automatically when the longest window is larger (the saved value is kept).",
    effect: "Larger sizes give a smoother, finer spectrum at more CPU.",
    tip: "32,768 (default); 65,536 with 6 layers.",
  },
  // --- EQ ---
  "eq.enabled": {
    text: "A pre-analysis equaliser applied to the audio before the FFT. It only colours the visual; the sound you hear is never changed.",
    tip: "On by default with a +24 dB high shelf, because music has far less energy in the highs.",
  },
  "eq.highShelf": {
    text: "Boosts or cuts everything above the high cutoff by 'High boost' dB.",
    effect: "A boost lifts the right side of the spectrum so cymbals and voices show.",
  },
  "eq.highGainDb": {
    text: "Gain of the high shelf in dB.",
    effect:
      "+24 dB (default) balances typical music, whose highs are 20-30 dB quieter than the bass.",
    tip: "Lower it if the highs look too busy; raise it for bass-heavy music.",
  },
  "eq.highCutoffHz": {
    text: "Where the high shelf starts (its midpoint).",
    tip: "1 kHz (default) lifts the presence range and everything above it.",
  },
  "eq.lowShelf": {
    text: "Boosts or cuts everything below the low cutoff by 'Low boost' dB.",
    tip: "A negative low boost tames a dominant kick drum.",
  },
  "eq.lowGainDb": {
    text: "Gain of the low shelf in dB; negative values cut the bass.",
  },
  "eq.lowCutoffHz": {
    text: "Where the low shelf starts (its midpoint).",
  },
  "eq.q": {
    text: "Steepness of both shelves. 0.707 is the smooth Butterworth shape; higher values make a sharper step with a small bump at the corner.",
  },
  "eq.amount": {
    text: "Blend between the dry and equalised signal: 0 bypasses the EQ, 1 applies it fully, above 1 exaggerates it.",
    tip: "A quick way to dial the EQ in or out without touching the gains.",
  },
  // --- Window & Weighting ---
  window: {
    text: "The taper applied to each window before the FFT. It trades sharpness (narrow peaks) against leakage (energy spilling into neighbouring frequencies).",
    effect:
      "Kaiser and Blackman-Harris keep quiet details visible next to loud tones; Hann and Hamming give narrower peaks; Rectangular is sharpest but leaks everywhere.",
    tip: "Kaiser (default) has a continuous leakage control (beta).",
  },
  kaiserBetaMode: {
    text: "Manual uses 'Kaiser beta'; Auto picks the smallest beta whose leakage stays below the dB range floor, which gives the sharpest peaks the display can use.",
  },
  kaiserBeta: {
    text: "Shape of the Kaiser window: higher beta means lower leakage and wider peaks.",
    effect:
      "15 (default) keeps leakage about 110 dB down; 8-10 gives sharper peaks with leakage near 80 dB.",
  },
  weighting: {
    text: "An equal-loudness curve applied per frequency: A and ITU-R 468 follow the ear's sensitivity at normal levels (they cut the bass), C is flatter.",
    effect: "A-weighting lowers 100 Hz by about 19 dB relative to 1 kHz.",
    tip: "Off (default) for visuals; A or 468 to show what is perceived as loud.",
  },
  magnitudeNorm: {
    text: "The level scale of the magnitudes. Coherent gain is the FFT CHOP's classic scale; full scale reads a full-scale sine as 1.0.",
    tip: "Only matters for the dBFS reference and linear loudness; both read correctly in either.",
  },
  // --- Loudness & Ballistics ---
  loudness: {
    text: "How a magnitude becomes a bar height. dB maps the 'dB range' below the reference onto the full height; linear uses the magnitude relative to the reference.",
    effect: "dB shows quiet details; linear shows only the loudest parts and looks spiky.",
    tip: "dB (default) for almost every visual.",
  },
  dbReference: {
    text: "What full height means. Frame peak: the loudest bin of each frame. 0 dBFS: an absolute level (a full-scale sine). Slow AGC: a peak follower that adapts to the volume.",
    effect:
      "Frame peak always fills the height; dBFS follows your volume knob; AGC adapts within seconds.",
    tip: "AGC (default) works at any playback volume.",
  },
  dbRange: {
    text: "How many dB below the reference are shown; anything quieter sits at the bottom.",
    effect:
      "A smaller range makes bars jump higher and hides quiet content; a larger range shows more texture.",
    tip: "60-80 dB for music; 40-50 dB for a punchy look.",
  },
  agcReleaseMs: {
    text: "How fast the automatic gain falls after a loud passage (it rises instantly).",
    effect:
      "Short releases pump the display after each hit; long ones keep it steady across a song.",
  },
  ballistics: {
    text: "Smooths each bar over time with separate rise (attack) and fall (release) speeds.",
    tip: "On (default); off shows every frame raw, which flickers.",
  },
  ballisticsMode: {
    text: "Milliseconds: time constants that behave the same at any analysis rate. Coefficient: the FFT CHOP's per-frame factors.",
  },
  attack: {
    text: "Per-frame rise factor: 0 follows instantly, 0.99 rises very slowly.",
  },
  release: {
    text: "Per-frame fall factor: 0 drops instantly, 0.99 falls very slowly.",
  },
  attackMs: {
    text: "Time for a bar to rise to 63% of a new, higher level.",
    effect: "0-20 ms feels instant; longer softens transients.",
  },
  releaseMs: {
    text: "Time for a bar to fall 63% of the way to a lower level.",
    effect: "Short releases look nervous; 200-400 ms gives the classic smooth decay.",
  },
  // --- Performance ---
  layerThreads: {
    text: "How many threads compute the Super FFT layers, the analysis thread included. 0 = auto: one per layer from 3 layers.",
    effect: "Results are identical at every count; only the time per frame changes.",
    tip: "Auto is fastest; set 1-2 to leave cores for games.",
  },
  layerRefresh: {
    text: "Hop-scaled recomputes layer 4 every 2nd frame, layer 5 every 4th and layer 6 every 8th, reusing their last result in between. Long windows change slowly, so little detail is lost.",
    tip: "Saves about a third of the CPU with 6 layers on few threads.",
  },
  spectralFeatures: {
    text: "Computes descriptors of each frame: centroid (brightness), rolloff, flatness (tonal vs noisy), flux (onsets), RMS and bass/mid/high power. They appear in Diagnostics.",
    tip: "Off unless you need them; they read every FFT bin.",
  },
};
