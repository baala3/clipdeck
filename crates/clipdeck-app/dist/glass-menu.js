// How the glass menu and its cascades draw their rows, shared by menu.html
// and cascade.html so both look the same.
window.GlassMenu = (() => {
  // A pane's padding, as glass-menu.css sets it: how far its first row sits
  // below its top edge.
  const PADDING = 6;

  const svg = (className, path) =>
    `<svg class="${className}" viewBox="0 0 12 12" aria-hidden="true"><path d="${path}"/></svg>`;
  const CHEVRON = svg('chevron', 'M4.5 2.5 8 6l-3.5 3.5');
  const CHECK = svg('check', 'M2 6.5 4.8 9 10 3');

  function el(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined && text !== null) node.textContent = text;
    return node;
  }

  let nextId = 0;
  function row(build) {
    const node = el('div', 'row');
    node.id = `row-${nextId++}`;
    node.setAttribute('role', 'menuitem');
    build(node);
    return node;
  }

  // A Clip's row: its number, thumbnail, title and number shortcut.
  function fillClip(node, clip) {
    node.append(el('span', 'num', clip.number ?? ''));
    if (clip.thumbnail) {
      const thumb = el('img', 'thumb');
      thumb.src = clip.thumbnail;
      thumb.alt = '';
      node.append(thumb);
    }
    node.append(el('span', 'title', clip.title));
    if (clip.shortcut) node.append(el('span', 'key', clip.shortcut));
  }

  // A row that opens a cascade: a folder, or More.
  function fillSubmenu(node, title) {
    node.setAttribute('aria-haspopup', 'menu');
    node.append(el('span', 'num'), el('span', 'title', title));
    node.insertAdjacentHTML('beforeend', CHEVRON);
  }

  // A row with a check mark column: More's commands, a Clip's actions.
  function fillCommand(node, title, checked) {
    const mark = el('span', 'num');
    if (checked) mark.innerHTML = CHECK;
    node.append(mark, el('span', 'title', title));
  }

  const heading = (title) => el('div', 'heading', title);
  const separator = () => el('div', 'sep');

  // A menu has no use for the browser's own context menu.
  window.addEventListener('contextmenu', (e) => e.preventDefault());

  return { PADDING, el, row, fillClip, fillSubmenu, fillCommand, heading, separator };
})();
