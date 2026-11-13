// Shared theme list for the browser frame and the new tab page.
window.HALO_THEMES = [
  { id: 'aurora', name: 'Aurora', dark: true, accent: '#5eead4',
    bg: 'radial-gradient(1100px 650px at 8% 0%, rgba(94,234,212,.38), transparent 60%), radial-gradient(900px 700px at 95% 25%, rgba(129,140,248,.45), transparent 60%), radial-gradient(800px 500px at 50% 110%, rgba(236,72,153,.25), transparent 60%), linear-gradient(135deg,#070b18,#101a33 55%,#0b1222)' },
  { id: 'dynamic', name: 'Dynamic', dark: true, accent: '#c4b5fd', animated: true,
    bg: 'linear-gradient(120deg,#1e1b4b,#4c1d95,#0e7490,#065f46,#1e1b4b)' },
  { id: 'storm', name: 'Storm', dark: true, accent: '#60a5fa',
    bg: 'radial-gradient(900px 600px at 80% 10%, rgba(96,165,250,.35), transparent 60%), linear-gradient(160deg,#0b1120,#1e3a8a 60%,#312e81)' },
  { id: 'sunset', name: 'Sunset', dark: true, accent: '#fdba74',
    bg: 'radial-gradient(900px 500px at 50% 120%, rgba(251,146,60,.7), transparent 65%), linear-gradient(170deg,#1f1335 0%,#5b1f4f 50%,#c2410c 100%)' },
  { id: 'ocean', name: 'Ocean', dark: true, accent: '#38bdf8',
    bg: 'radial-gradient(1000px 600px at 20% 100%, rgba(14,165,233,.45), transparent 60%), linear-gradient(185deg,#020617,#0c4a6e 65%,#0e7490)' },
  { id: 'forest', name: 'Forest', dark: true, accent: '#86efac',
    bg: 'radial-gradient(900px 600px at 85% 0%, rgba(163,230,53,.25), transparent 60%), linear-gradient(150deg,#03140b,#14532d 55%,#365314)' },
  { id: 'midnight', name: 'Midnight', dark: true, accent: '#a5b4fc',
    bg: 'radial-gradient(2px 2px at 20% 30%, #fff8, transparent), radial-gradient(1.5px 1.5px at 70% 20%, #fff7, transparent), radial-gradient(1.5px 1.5px at 40% 70%, #fff6, transparent), radial-gradient(2px 2px at 85% 60%, #fff6, transparent), radial-gradient(1px 1px at 10% 85%, #fff8, transparent), linear-gradient(180deg,#05060c,#0d1020)' },
  { id: 'graphite', name: 'Graphite', dark: true, accent: '#e4e4e7', bg: 'linear-gradient(180deg,#1c1c1f,#141416)' },
  { id: 'rose', name: 'Rose', dark: false, accent: '#e11d48',
    bg: 'radial-gradient(900px 600px at 0% 0%, rgba(251,113,133,.35), transparent 60%), linear-gradient(135deg,#fff1f2,#fecdd3 55%,#fbcfe8)' },
  { id: 'lavender', name: 'Lavender', dark: false, accent: '#7c3aed',
    bg: 'radial-gradient(900px 600px at 100% 0%, rgba(167,139,250,.35), transparent 60%), linear-gradient(135deg,#f5f3ff,#ddd6fe 60%,#e0e7ff)' },
  { id: 'sand', name: 'Sand', dark: false, accent: '#b45309', bg: 'linear-gradient(160deg,#fefce8,#fde68a 70%,#fcd34d)' },
  { id: 'paper', name: 'Paper', dark: false, accent: '#2563eb', bg: 'linear-gradient(180deg,#f7f7f8,#ececef)' }
];

window.haloTheme = function (id) {
  return window.HALO_THEMES.find(t => t.id === id) || window.HALO_THEMES[0];
};

// Paint a root element with the chosen theme and background.
window.haloApplyTheme = function (el, s) {
  const t = window.haloTheme(s.theme);
  const dark = s.custom_bg ? s.custom_dark : t.dark;
  const accent = s.accent || t.accent;
  const st = el.style;
  st.setProperty('--accent', accent);
  st.setProperty('--fg', dark ? '#f4f4f5' : '#18181b');
  st.setProperty('--fg-dim', dark ? 'rgba(244,244,245,.62)' : 'rgba(24,24,27,.6)');
  st.setProperty('--glass', dark ? 'rgba(18,18,24,.42)' : 'rgba(255,255,255,.55)');
  st.setProperty('--glass-strong', dark ? 'rgba(20,20,28,.82)' : 'rgba(255,255,255,.9)');
  st.setProperty('--hover', dark ? 'rgba(255,255,255,.08)' : 'rgba(0,0,0,.06)');
  st.setProperty('--active', dark ? 'rgba(255,255,255,.16)' : 'rgba(255,255,255,.85)');
  st.setProperty('--border', dark ? 'rgba(255,255,255,.10)' : 'rgba(0,0,0,.08)');
  st.setProperty('--dim', String(s.bg_dim || 0));
  el.classList.toggle('dark', !!dark);
  el.classList.toggle('light', !dark);
  el.classList.toggle('animated', !s.custom_bg && !!t.animated);
  if (s.custom_bg) {
    st.setProperty('--bg', 'url(http://halo.localhost/bg?v=' + (s.bg_version || 0) + ') center / cover no-repeat, #111');
  } else {
    st.setProperty('--bg', t.bg);
  }
};
