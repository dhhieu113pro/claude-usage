// Shared by the MCP and Settings pages: token, DOM helpers, API calls, theme button, navigation.
const token = new URLSearchParams(location.search).get('t') || '';
const $ = id => document.getElementById(id);

function el(tag, props = {}, ...kids) {
  const n = Object.assign(document.createElement(tag), props);
  for (const k of kids) n.append(k);
  return n;
}

async function api(path, body) {
  const res = await fetch(path, {
    method: body ? 'POST' : 'GET',
    headers: { 'X-Token': token, 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

function banner(msg) {
  const b = $('banner');
  b.textContent = msg || '';
  b.style.display = msg ? 'block' : 'none';
}

const themes = ['auto', 'light', 'dark'];
let theme = 'auto';
try { theme = localStorage.getItem('theme') || 'auto'; } catch {}

// Static, trusted SVG (24x24, stroke icons) for the three theme modes.
const THEME_ICONS = {
  auto: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>',
  light: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>',
  dark: '<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/>',
};
const THEME_LABELS = { auto: 'Auto (follows system)', light: 'Light', dark: 'Dark' };

function applyTheme(t) {
  if (t === 'auto') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
  const b = $('theme');
  b.innerHTML = `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${THEME_ICONS[t]}</svg>`;
  b.title = `Theme: ${THEME_LABELS[t]} (click to change)`;
  b.setAttribute('aria-label', b.title);
}
applyTheme(theme);
$('theme').onclick = () => {
  theme = themes[(themes.indexOf(theme) + 1) % themes.length];
  try { theme === 'auto' ? localStorage.removeItem('theme') : localStorage.setItem('theme', theme); } catch {}
  applyTheme(theme);
};

const pages = [['/profiles', 'Profiles'], ['/mcp', 'MCP servers'], ['/settings', 'Settings']];
const here = location.pathname === '/' ? '/profiles' : location.pathname;
$('nav').replaceChildren(...pages.map(([path, label]) =>
  el('a', { href: `${path}?t=${encodeURIComponent(token)}`, textContent: label, className: here === path ? 'active' : '' })));
