// Runs in web pages. Only notes which keys are held on a click, and lists links on request.

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

chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.type !== 'collect') return undefined;

  const links = message.mode === 'selection' ? linksInSelection() : [...document.links];
  respond(links.map((link) => link.href));
  return undefined;
});
