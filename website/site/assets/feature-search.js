'use strict';
// The features index: a search over each guide's title, description, steps
// and narration in every language, plus category chips. The query lives in
// the address (?q=) so a search can be shared; "/" focuses the box.
(() => {
  const input = document.querySelector('#feature-query');
  const data = document.querySelector('#feature-index');
  if (!(input instanceof HTMLInputElement) || !data) return;
  // Case and accents never matter: "thème" finds "theme" and back.
  const fold = (text) => text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  const entries = JSON.parse(data.textContent).map((entry) => ({ ...entry, text: fold(entry.text) }));
  const cards = new Map([...document.querySelectorAll('.feature-card')].map((card) => [card.dataset.id, card]));
  const chips = [...document.querySelectorAll('.feature-filters [data-category]')];
  const count = document.querySelector('#feature-count');
  const empty = document.querySelector('#feature-empty');
  let category = '';

  const apply = () => {
    const words = fold(input.value).split(/\s+/).filter(Boolean);
    let shown = 0;
    for (const entry of entries) {
      const match = (!category || entry.category === category)
        && words.every((word) => entry.text.includes(word));
      cards.get(entry.id).hidden = !match;
      if (match) shown += 1;
    }
    count.textContent = String(shown);
    empty.hidden = shown > 0;
    const url = new URL(window.location.href);
    if (input.value.trim()) url.searchParams.set('q', input.value.trim());
    else url.searchParams.delete('q');
    window.history.replaceState(null, '', url);
  };

  input.value = new URL(window.location.href).searchParams.get('q') || '';
  input.addEventListener('input', apply);
  chips.forEach((chip) => {
    chip.addEventListener('click', () => {
      category = chip.dataset.category;
      chips.forEach((other) => other.setAttribute('aria-pressed', String(other === chip)));
      apply();
    });
  });
  document.addEventListener('keydown', (event) => {
    if (event.key === '/' && document.activeElement !== input) {
      event.preventDefault();
      input.focus();
    } else if (event.key === 'Escape' && document.activeElement === input) {
      input.value = '';
      apply();
    }
  });
  apply();
})();
