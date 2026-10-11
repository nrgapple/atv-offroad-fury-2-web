// Keep the two-link entry screen usable without putting down a controller.
const links = [...document.querySelectorAll('nav a')];
let held = new Set(), selected = 0;
links.forEach((link, index) => link.addEventListener('focus', () => { selected = index; }));
function poll() {
  const next = new Set();
  if (!document.hidden && document.hasFocus()) {
    let pads = []; try { pads = navigator.getGamepads?.() ?? []; } catch { /* keyboard and touch remain available */ }
    for (const pad of pads) if (pad?.connected) {
      if (pad.buttons[12]?.pressed || pad.buttons[14]?.pressed || pad.axes[0] < -.55 || pad.axes[1] < -.55) next.add('previous');
      if (pad.buttons[13]?.pressed || pad.buttons[15]?.pressed || pad.axes[0] > .55 || pad.axes[1] > .55) next.add('next');
      if (pad.buttons[0]?.pressed || pad.buttons[9]?.pressed) next.add('accept');
    }
    for (const action of next) if (!held.has(action)) {
      if (action === 'accept') links[selected].click();
      else { selected = (selected + 1) % links.length; links[selected].focus({ preventScroll: true }); }
    }
  }
  held = next; requestAnimationFrame(poll);
}
requestAnimationFrame(poll);
