// "Load more" for post lists. The server renders the cards (same partial as
// the page), so this script only inserts HTML it received from our own API.
(() => {
  const button = document.getElementById('load-more');
  if (!button) return;
  const target = document.getElementById(button.dataset.target);
  const status = document.getElementById('load-more-status');
  if (!target) return;

  const endpoint = button.dataset.endpoint;
  const limit = Number(button.dataset.limit) || 10;
  let offset = Number(button.dataset.offset) || 0;
  const label = button.textContent;
  let loading = false;

  button.addEventListener('click', async () => {
    if (loading) return;
    loading = true;
    button.disabled = true;
    button.textContent = 'Loading\u2026';
    status.textContent = '';
    try {
      const response = await fetch(`${endpoint}?offset=${offset}&limit=${limit}`, {
        headers: { Accept: 'application/json' },
      });
      if (response.status === 429) throw new Error('Too many requests, please wait a moment.');
      if (!response.ok) throw new Error(`Request failed (HTTP ${response.status}).`);
      const data = await response.json();
      target.insertAdjacentHTML('beforeend', data.html);
      offset = data.next_offset;
      status.textContent = `Loaded ${data.posts.length} more post${data.posts.length === 1 ? '' : 's'}.`;
      if (!data.has_more) button.classList.add('w3-hide');
    } catch (err) {
      status.textContent = err.message || 'Could not load more posts.';
    } finally {
      loading = false;
      button.disabled = false;
      button.textContent = label;
    }
  });
})();
