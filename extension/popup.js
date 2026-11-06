// Lists the video found on the current page.

import { mediaKey, mediaLabel } from './rules.js';

async function currentTabId() {
  const asked = Number(new URLSearchParams(location.search).get('tab'));
  if (asked) return asked;

  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab?.id;
}

async function show() {
  const tabId = await currentTabId();
  const key = mediaKey(tabId);
  const { [key]: items = [] } = await chrome.storage.session.get(key);

  document.getElementById('nothing').hidden = items.length > 0;

  const list = document.getElementById('items');
  items.forEach((item, index) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = mediaLabel(item);
    button.title = item.url;
    button.addEventListener('click', () => {
      void chrome.runtime.sendMessage({ type: 'save-media', tabId, index });
    });

    const row = document.createElement('li');
    row.append(button);
    list.append(row);
  });
}

void show();
