// Optional "Enhanced" sound (Settings > Audio > Sound). Authored, not original:
// engine sample transitions crossfade, and one shared echo send colours the
// SFX bus in stadiums and enclosed canyons. "Original" bypasses all of it.
export const CROSSFADE_SECONDS = 0.12;
// Two damped feedback delays (left/right), fed once from the SFX bus.
export const ECHO_PRESETS = {
  stadium: { delays: [0.083, 0.127], feedback: 0.32, damping: 2800, send: 0.16 },
  canyon: { delays: [0.21, 0.29], feedback: 0.38, damping: 2200, send: 0.14 },
};
export function createEchoBus(context, input, output, type) {
  const preset = ECHO_PRESETS[type];
  if (!preset) return null;
  const nodes = [];
  const node = n => { nodes.push(n); return n; };
  const send = node(context.createGain()), damp = node(context.createBiquadFilter()), merger = node(context.createChannelMerger(2)), wet = node(context.createGain());
  merger.atvVoice = 'echo'; // lets instrumented tests attribute output
  send.gain.value = 1; damp.type = 'lowpass'; damp.frequency.value = preset.damping; wet.gain.value = preset.send;
  input.connect(send); send.connect(damp);
  preset.delays.forEach((seconds, channel) => {
    const delay = node(context.createDelay(1)), feedback = node(context.createGain());
    delay.delayTime.value = seconds; feedback.gain.value = preset.feedback;
    damp.connect(delay); delay.connect(feedback); feedback.connect(delay);
    delay.connect(merger, 0, channel);
  });
  merger.connect(wet); wet.connect(output);
  return { type, nodes: nodes.length, dispose() { try { input.disconnect(send); } catch { /* already detached */ } for (const n of nodes) n.disconnect(); } };
}
// Fade a playing engine voice out and stop it once silent.
export function fadeOut(context, voice, seconds = CROSSFADE_SECONDS) {
  const now = context.currentTime;
  if (!voice.fade) { voice.source.stop(); voice.source.disconnect(); return; }
  voice.fade.gain.cancelScheduledValues(now);
  voice.fade.gain.setValueAtTime(voice.fade.gain.value, now);
  voice.fade.gain.linearRampToValueAtTime(0, now + seconds);
  voice.source.stop(now + seconds + 0.01);
}
