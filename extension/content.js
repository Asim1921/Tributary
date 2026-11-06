// Runs in web pages. Notes which keys are held on a click, lists links on request, and
// shows the panel over video players.

// Tells the extension which document this tab now holds, so it can drop finds from the
// page before it.
if (window === window.top) {
  chrome.runtime.sendMessage({ type: 'page-start' }).catch(() => {});
}

addEventListener(
  'mousedown',
  (event) => {
    chrome.runtime
      .sendMessage({
        type: 'click',
        ctrl: event.ctrlKey,
        alt: event.altKey,
        shift: event.shiftKey,
      })
      .catch(() => {});
  },
  { capture: true, passive: true },
);

function linksInSelection() {
  const selection = getSelection();
  if (!selection || selection.rangeCount === 0) return [];

  const range = selection.getRangeAt(0);
  return [...document.links].filter((link) => range.intersectsNode(link));
}

const PANEL_STYLE = `
  .panel {
    display: flex; flex-direction: column; gap: 2px; min-width: 150px; max-width: 240px;
    padding: 4px; font: 13px/1.3 system-ui, sans-serif; color: #eef3f2;
    background: #15201fee; border: 1px solid #26b5a4; border-radius: 7px;
  }
  button {
    padding: 5px 8px; overflow: hidden; font: inherit; color: inherit; text-align: left;
    text-overflow: ellipsis; white-space: nowrap; cursor: pointer;
    background: transparent; border: 0; border-radius: 4px;
  }
  button:hover { background: #26b5a433; }
  .hide { align-self: flex-end; padding: 0 6px; opacity: 0.7; }
`;

/** The most room a player can have, so ads and previews don't get the panel. */
function largestVideo() {
  let best = null;
  let bestArea = 0;

  for (const video of document.querySelectorAll('video')) {
    const box = video.getBoundingClientRect();
    const area = box.width * box.height;
    if (area > bestArea) {
      best = video;
      bestArea = area;
    }
  }
  return best;
}

let panel = null;
let panelHidden = false;

function placePanel() {
  if (!panel) return;

  const box = largestVideo()?.getBoundingClientRect();
  const margin = 8;
  const top = Math.max(margin, (box?.top ?? 0) + margin);
  const right = Math.max(margin, innerWidth - (box?.right ?? innerWidth) + margin);
  panel.style.top = `${top}px`;
  panel.style.right = `${right}px`;
}

function showPanel(labels) {
  if (panelHidden) return;
  // A frame only gets a panel if its player is actually in it.
  if (window !== window.top && !largestVideo()) return;

  panel?.remove();
  panel = document.createElement('div');
  panel.setAttribute('data-tributary-panel', '');
  panel.style.cssText = 'all: initial; position: fixed; z-index: 2147483647;';

  // Closed, so the page can't reach in and press anything.
  const shadow = panel.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = PANEL_STYLE;

  const box = document.createElement('div');
  box.className = 'panel';

  labels.forEach((label, index) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = `Save: ${label}`;
    button.addEventListener('click', (event) => {
      if (event.isTrusted) void chrome.runtime.sendMessage({ type: 'save-media', index });
    });
    box.append(button);
  });

  const hide = document.createElement('button');
  hide.type = 'button';
  hide.className = 'hide';
  hide.textContent = 'Hide';
  hide.addEventListener('click', (event) => {
    if (!event.isTrusted) return;
    panelHidden = true;
    panel?.remove();
    panel = null;
  });
  box.append(hide);

  shadow.append(style, box);
  document.documentElement.append(panel);
  placePanel();
}

addEventListener('scroll', placePanel, { capture: true, passive: true });
addEventListener('resize', placePanel, { passive: true });

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.type === 'media') {
    showPanel(message.labels);
    return undefined;
  }
  if (message?.type !== 'collect') return undefined;

  const links = message.mode === 'selection' ? linksInSelection() : [...document.links];
  respond(links.map((link) => link.href));
  return undefined;
});
