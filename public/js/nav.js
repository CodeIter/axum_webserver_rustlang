// Mobile navigation toggle (inline scripts are blocked by the CSP).
(() => {
  const toggle = document.getElementById('nav-toggle');
  const menu = document.getElementById('nav-mobile');
  if (!toggle || !menu) return;
  toggle.addEventListener('click', () => {
    const open = menu.classList.toggle('w3-show');
    toggle.setAttribute('aria-expanded', String(open));
  });
})();
