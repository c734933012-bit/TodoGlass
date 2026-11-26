'use strict';

let state = { goal: '', items: [], completionCount: 0 };
const currentLook = { frost: 0, r: 22, rc: 14 };

// frost 0..100 -> blur / saturation / haze / tint; radius -> window + cards (cards at ~64%)
function applyLook(frost, radius) {
  const f = Math.max(0, Math.min(100, frost | 0)), r = Math.max(25, Math.min(80, radius | 0));
  currentLook.frost = f; currentLook.r = r; currentLook.rc = Math.max(4, Math.round(r * 0.64));
  const root = document.documentElement.style, app = document.getElementById('app');
  root.setProperty('--r', r + 'px');
  root.setProperty('--rc', currentLook.rc + 'px');
  // keep content clear of the rounded corners: inset = r * (1 - 1/sqrt2) + base
  const inset = r * (1 - Math.SQRT1_2);
  root.setProperty('--in', inset.toFixed(1) + 'px');
  const pr = Math.round(r * 0.73);
  root.setProperty('--pr', pr + 'px');
  root.setProperty('--pin', (pr * (1 - Math.SQRT1_2)).toFixed(1) + 'px');
  root.setProperty('--cin', (Math.min(currentLook.rc, 19) * 0.3).toFixed(1) + 'px');
  root.setProperty('--fb', (f * 0.36).toFixed(1) + 'px');          // up to 36px blur
  root.setProperty('--fs', (100 + f * 0.8).toFixed(0) + '%');       // up to 180% saturation
  // no live desktop blur available: frost = milky white haze that hides background detail
  root.setProperty('--fh', (f * 0.0030).toFixed(3));                 // up to .30 milky haze
  root.setProperty('--tint', (0.28 - f * 0.0012).toFixed(3));        // keep some dark tint for white text
  document.getElementById('frostLayer').classList.toggle('on', f > 0);
  if (app) app.style.borderRadius = r + 'px';
  document.querySelectorAll('.todo-item.lg').forEach((el) => { el.style.borderRadius = currentLook.rc + 'px'; });
  const s = document.getElementById('settings'); if (s && s.__lg) s.style.borderRadius = pr + 'px';
}

function initFrost() {
  const img = document.getElementById('frostImg');
  window.todoAPI.onFrost((fr) => {
    img.style.left = fr.x + 'px'; img.style.top = fr.y + 'px';
    img.style.width = fr.w + 'px'; img.style.height = fr.h + 'px';
    img.style.backgroundImage = `url(data:image/jpeg;base64,${fr.url})`;
  });
}
let dragId = null;
let animating = new Set();

const $ = (sel) => document.querySelector(sel);

function activeItems() {
  return (state.items || []).filter((it) => !it.deleted && !it.done);
}

function render() {
  paintActivity();
  const list = $('#list');
  const items = activeItems().sort((a, b) => (a.order ?? 0) - (b.order ?? 0));


  const existing = new Map([...list.querySelectorAll('.todo-item')].map((el) => [el.dataset.id, el]));
  const keep = new Set();

  items.forEach((it) => {
    keep.add(it.id);
    let el = existing.get(it.id);
    if (!el) {
      el = document.createElement('li');
      el.className = 'todo-item';
      el.dataset.id = it.id;
      el.draggable = false;

      const circle = document.createElement('button');
      circle.className = 'circle';
      circle.type = 'button';
      circle.title = '完成';
      circle.addEventListener('click', (e) => {
        e.stopPropagation();
        completeTodo(it.id, el);
      });

      const text = document.createElement('div');
      text.className = 'todo-text';
      text.textContent = it.text;

      el.appendChild(circle);
      el.appendChild(text);
      if (window.syncExtras) window.syncExtras(el, it);
      if (window.LiquidGlass) window.LiquidGlass.wrap(el, { cornerRadius: currentLook.rc, warp: false, elasticity: 0, shadow: false });

      el.addEventListener('dragstart', (e) => {
        if (animating.has(it.id)) { e.preventDefault(); return; }
        dragId = it.id;
        el.classList.add('dragging');
        e.dataTransfer.effectAllowed = 'move';
        try { e.dataTransfer.setData('text/plain', it.id); } catch (_) {}
      });
      el.addEventListener('dragend', () => {
        dragId = null;
        el.classList.remove('dragging');
        list.querySelectorAll('.drag-over').forEach((n) => n.classList.remove('drag-over'));
      });
      el.addEventListener('dragover', (e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'move';
        el.classList.add('drag-over');
      });
      el.addEventListener('dragleave', () => el.classList.remove('drag-over'));
      el.addEventListener('drop', async (e) => {
        e.preventDefault();
        el.classList.remove('drag-over');
        const from = dragId || e.dataTransfer.getData('text/plain');
        const to = it.id;
        if (!from || from === to) return;
        await reorderRelative(from, to);
      });

      existing.set(it.id, el);
    } else {
      const text = el.querySelector('.todo-text');
      if (text && !animating.has(it.id) && text.textContent !== it.text) {
        text.textContent = it.text;
      }
      if (window.syncExtras && !animating.has(it.id)) window.syncExtras(el, it);
    }
  });

  // remove stale (not animating)
  for (const [id, el] of existing) {
    if (!keep.has(id) && !animating.has(id)) {
      el.remove();
    }
  }

  // Reconcile in one pass, keeping unchanged cards in place.
  let cursor = list.firstElementChild;
  for (const item of items) {
    const card = existing.get(item.id);
    if (!card || animating.has(item.id)) continue;
    if (card !== cursor) list.insertBefore(card, cursor);
    cursor = card.nextElementSibling;
  }

}

window.reorderIds = async (ids) => { state = await window.todoAPI.reorder(ids); render(); };
window.isAnimating = (id) => animating.has(id);
async function reorderRelative(fromId, toId) {
  const items = activeItems().sort((a, b) => (a.order ?? 0) - (b.order ?? 0));
  const ids = items.map((it) => it.id);
  const from = ids.indexOf(fromId);
  const to = ids.indexOf(toId);
  if (from < 0 || to < 0) return;
  ids.splice(from, 1);
  ids.splice(to, 0, fromId);
  state = await window.todoAPI.reorder(ids);
  render();
}

async function completeTodo(id, el) {
  if (animating.has(id)) return;
  animating.add(id);
  el.classList.add('completing');
  const circle = el.querySelector('.circle');
  const text = el.querySelector('.todo-text');
  circle.classList.add('checked');
  text.classList.add('strike');

  const rect = el.getBoundingClientRect();
  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 2;

  // small firework
  if (window.FX) window.FX.burst(cx, cy, { count: 26, speed: 3.4, life: 48 });

  let result;
  try { result = await window.todoAPI.completeItem(id); } catch { animating.delete(id); el.classList.remove('completing'); circle.classList.remove('checked'); text.classList.remove('strike'); return; }
  state = result.state;

  // wait for strike animation
  await sleep(380);

  // shatter into pixels
  // powder lands on the next card (or the window bottom if this is the last one)
  let next = el.nextElementSibling;
  while (next && (next.classList.contains('completing') || !next.classList.contains('todo-item'))) next = next.nextElementSibling;
  if (window.FX) {
    window.FX.shatterElement(el, async () => {
      el.classList.add('shattering');
      await sleep(280);
      animating.delete(id);
      el.remove();
      render();
    }, next || null);
  } else {
    animating.delete(id);
    el.remove();
    render();
  }

  if (result.firedBig && window.FX) {
    setTimeout(() => {
      window.FX.bigBurst(window.innerWidth / 2, window.innerHeight / 3);
    }, 200);
  }
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function init() {
  if (window.FX) window.FX.resize();
  let received = 0;
  await window.todoAPI.listenState((s) => {
    received++;
    // don't clobber mid-animation heavily; still update
    state = s;
    render();
  });

  const refresh = async () => { const before = received; const snapshot = await window.todoAPI.getState(); if (received === before) { state = snapshot; render(); } };
  await refresh();
  $('#addInput').addEventListener('keydown', async (e) => {
    if (e.key === 'Enter' && !e.isComposing && e.keyCode !== 229 && !e.repeat) {
      e.preventDefault();
      const v = e.target.value.trim();
      if (!v) return;
      if (e.target.dataset.saving) return;
      e.target.dataset.saving = '1';
      try { await window.todoAPI.addItem(v); if (e.target.value.trim() === v) e.target.value = ''; }
      finally { delete e.target.dataset.saving; }
    }
  });

  // Direct JSON imports still become visible when returning to the window.
  window.addEventListener('focus', async () => {
    if (!animating.size) await refresh();
  });

}

// ---------- header, settings, status ----------
let activityTimer;
function paintActivity() {
  const dot = $('#activityDot');
  const stamp = state.lastActivityAt || state.activityStartedAt;
  clearTimeout(activityTimer);
  if (!stamp) { dot.hidden = true; return; }
  const hour = 3600000;
  const elapsed = Math.max(0, Date.now() - stamp);
  const color = elapsed >= 24 * hour ? 'red' : elapsed >= 12 * hour ? 'yellow' : 'green';
  const range = {green: '未滿 12 小時', yellow: '已滿 12 小時', red: '已滿 24 小時'}[color];
  const label = state.lastActivityAt ? '上次新增或完成待辦' : '開始記錄（尚無新增或完成）';
  dot.className = 'activity-dot ' + color;
  dot.title = label + '：' + new Date(stamp).toLocaleString() + ' · ' + range;
  dot.setAttribute('aria-label', dot.title);
  dot.hidden = false;
  if (color !== 'red') {
    const next = stamp + (color === 'green' ? 12 : 24) * hour;
    activityTimer = setTimeout(paintActivity, Math.min(24 * hour, Math.max(1, next - Date.now())));
  }
}

function setTitle(t) { $('#appTitle').textContent = (t && t.trim()) || 'TodoGlass'; document.title = $('#appTitle').textContent; }

async function initChrome() {
  const settings = await window.todoAPI.getSettings();
  if (settings.readWarning) window.showTodoError(settings.readWarning);
  setTitle(settings.title);
  initFrost();
  const statusInfo = await window.todoAPI.getStatus();
  if (window.LiquidGlass) {
    const LG = window.LiquidGlass;
    // window panel: border light only — no warp/aberration at the outer edge
    LG.wrap($('#app'), { cornerRadius: settings.radius || 25, warp: false, shadow: false, elasticity: 0, trackOutside: true });
    $('#app').style.borderRadius = 'var(--r)';
    LG.wrap($('#addWrap'), { cornerRadius: 999, warp: false, elasticity: 0, shadow: false });
    document.querySelectorAll('.icon-btn').forEach((b) => LG.wrap(b, { cornerRadius: 999, warp: false, elasticity: 0.2, shadow: false, activationZone: 60 }));
    LG.wrap($('#settings'), { cornerRadius: Math.round(Math.max(25, Math.min(80, settings.radius || 25)) * 0.73), displacementScale: 64, blurAmount: 0.25, saturation: 140, aberrationIntensity: 2, elasticity: 0, shadow: false });
  }
  const paintSync = (s) => {
    $('#syncInfo').textContent = !s.apiOk ? '同步介面無法連線' : s.lastOriAt
      ? '上次同步：' + new Date(s.lastOriAt).toLocaleString() : '同步介面可用，尚無配對客戶端讀寫';
  };
  paintSync(statusInfo);
  window.todoAPI.onStatus(paintSync);
  $('#reconnect').addEventListener('click', async () => { await window.todoAPI.reconnect(); paintSync(await window.todoAPI.getStatus()); });
  document.addEventListener('visibilitychange', () => { if (!document.hidden) paintActivity(); });
  window.addEventListener('focus', paintActivity);

  applyLook(settings.frost || 0, settings.radius || 25);

  // chevron toggles the add input (collapsed by default)
  const chev = $('#toggleAdd'), block = $('#addBlock'), input = $('#addInput');
  chev.addEventListener('click', () => {
    const open = block.classList.toggle('collapsed') === false;
    chev.classList.toggle('open', open);
    input.tabIndex = open ? 0 : -1;
    if (open) setTimeout(() => input.focus(), 120); else input.blur();
  });

  // settings panel
  const panel = $('#settings'), titleInput = $('#titleInput');
  titleInput.value = settings.title || 'TodoGlass';
  const fr = $('#frostRange'), rr = $('#radiusRange');
  fr.value = settings.frost || 0; rr.value = settings.radius || 25;
  $('#frostVal').textContent = fr.value; $('#radiusVal').textContent = rr.value;
  let lt = null;
  const onLook = () => {
    $('#frostVal').textContent = fr.value; $('#radiusVal').textContent = rr.value;
    applyLook(+fr.value, +rr.value);
    clearTimeout(lt);
    lt = setTimeout(() => window.todoAPI.setSettings({ frost: +fr.value, radius: +rr.value }), 250);
  };
  fr.addEventListener('input', onLook); rr.addEventListener('input', onLook);
  $('#openSettings').addEventListener('click', () => {
    panel.classList.toggle('hidden');
    if (!panel.classList.contains('hidden')) setTimeout(() => titleInput.focus(), 80);
  });
  $('#closeSettings').addEventListener('click', () => panel.classList.add('hidden'));
  if (statusInfo.openSettings) panel.classList.remove('hidden');
  let tt = null;
  titleInput.addEventListener('input', () => {
    setTitle(titleInput.value);
    clearTimeout(tt);
    tt = setTimeout(() => window.todoAPI.setSettings({ title: titleInput.value.trim() || 'TodoGlass' }), 300);
  });
  titleInput.addEventListener('keydown', (e) => { if (!e.isComposing && e.keyCode !== 229 && (e.key === 'Enter' || e.key === 'Escape')) panel.classList.add('hidden'); });

  // sheen follows the pointer (subtle liquid highlight)
  const app = $('#app');
  app.addEventListener('mousemove', (e) => {
    const r = app.getBoundingClientRect();
    app.style.setProperty('--mx', ((e.clientX - r.left) / r.width * 100).toFixed(1) + '%');
    app.style.setProperty('--my', ((e.clientY - r.top) / r.height * 100).toFixed(1) + '%');
  });
  app.addEventListener('mouseleave', () => { app.style.setProperty('--mx', '0%'); app.style.setProperty('--my', '0%'); });
}

document.addEventListener('DOMContentLoaded', () => { initChrome(); init(); });
