'use strict';
// A feature page's demo: one recording per window, narrated in each
// language. The window and language buttons swap the clip and keep the
// place; the chapter buttons seek and play.
const demo = document.querySelector('#feature-demo');
if (demo instanceof HTMLVideoElement) {
  const labels = { en: 'English', fr: 'Français' };
  const chosen = { window: demo.dataset.window, lang: 'en' };
  let pending = null;
  const play = async () => {
    try {
      await demo.play();
    } catch (error) {
      if (error.name === 'AbortError') return;
      // Native controls remain available when the browser blocks playback.
      demo.focus();
    }
  };
  // Space plays and pauses the demo anywhere on the page, as on video sites,
  // instead of scrolling it: also while the video, a chapter, or a window or
  // language button has the keyboard. Only typing in a field keeps Space, and
  // Enter still presses a focused button. Listening while capturing runs
  // before the browser's own handling, so the video never toggles twice.
  window.addEventListener('keydown', (event) => {
    if (event.key !== ' ' || event.metaKey || event.ctrlKey || event.altKey) return;
    const target = event.target;
    if (target instanceof HTMLElement
        && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;
    if (pending) pending.playing = !pending.playing;
    else if (demo.paused) play(); else demo.pause();
  }, { capture: true });
  document.querySelectorAll('[data-seek]').forEach((button) => {
    button.addEventListener('click', () => {
      const at = Number(button.dataset.seek);
      if (pending) {
        pending = { at, playing: true };
        return;
      }
      demo.currentTime = at;
      play();
    });
  });
  demo.addEventListener('loadedmetadata', () => {
    if (!pending) return;
    const { at, playing } = pending;
    pending = null;
    demo.currentTime = Math.min(at, demo.duration);
    if (playing) play();
  });
  const show = () => {
    pending ??= { at: demo.currentTime, playing: !demo.paused };
    const name = `${demo.dataset.clips}.${chosen.window}.${chosen.lang}`;
    demo.querySelector('source').src = `${name}.mp4`;
    const track = demo.querySelector('track');
    track.src = `${name}.vtt`;
    track.srclang = chosen.lang;
    track.label = labels[chosen.lang];
    demo.load();
  };
  ['window', 'lang'].forEach((key) => {
    const buttons = document.querySelectorAll(`.demo-switches [data-${key}]`);
    buttons.forEach((button) => {
      button.addEventListener('click', () => {
        if (chosen[key] === button.dataset[key]) return;
        chosen[key] = button.dataset[key];
        buttons.forEach((other) => {
          other.setAttribute('aria-pressed', String(other === button));
        });
        show();
      });
    });
  });
}
