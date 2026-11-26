/* TodoGlass features: recurring tasks and tags */
(function () {
  const $ = (s) => document.querySelector(s);
  const API = () => window.todoAPI;
  const DAY = ['日', '一', '二', '三', '四', '五', '六'];
  const ORDER = [1, 2, 3, 4, 5, 6, 0];

  // ---- per-item extras: tag dot + recurring mark ----
  window.syncExtras = function (el, it) {
    const want = it.tag || '';
    let dot = el.querySelector(':scope > .tag-dot');
    if (want) {
      if (!dot) { dot = document.createElement('i'); el.appendChild(dot); }
      dot.className = 'tag-dot ' + want;
    } else if (dot) dot.remove();
    let mk = el.querySelector(':scope > .rec-mark');
    if (it.recurringId && !mk) {
      mk = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      mk.setAttribute('viewBox', '0 0 24 24'); mk.setAttribute('class', 'rec-mark');
      mk.innerHTML = '<path d="M20 12a8 8 0 1 1-2.34-5.66M20 4v4h-4"/>';
      const t = el.querySelector('.todo-text'); t.after(mk);
    } else if (!it.recurringId && mk) mk.remove();
    if (want && mk) mk.style.marginLeft = 'auto', dot.style.marginLeft = '0';
    else if (dot) dot.style.marginLeft = '';
  };

  document.addEventListener('DOMContentLoaded', () => {
    // ---- context menu ----
    const ctx = $('#ctx'); let ctxId = null;
    const closeCtx = () => { ctx.classList.add('hidden'); ctxId = null; };
    document.addEventListener('contextmenu', (e) => {
      const item = e.target.closest && e.target.closest('.todo-item');
      if (!item || item.classList.contains('completing')) { closeCtx(); return; }
      e.preventDefault();
      ctxId = item.dataset.id;
      const cur = (item.querySelector(':scope > .tag-dot') || {}).className || '';
      ctx.querySelectorAll('.tag-pick').forEach((b) => b.classList.toggle('cur', cur.includes(b.dataset.tag)));
      ctx.classList.remove('hidden');
      const w = ctx.offsetWidth, h = ctx.offsetHeight, m = 8;
      ctx.style.left = Math.max(m, Math.min(e.clientX, innerWidth - w - m)) + 'px';
      ctx.style.top = Math.max(m, Math.min(e.clientY, innerHeight - h - m)) + 'px';
    });
    ctx.addEventListener('click', async (e) => {
      const b = e.target.closest('[data-tag]'); if (!b || !ctxId) return;
      const id = ctxId; closeCtx();
      await API().setTag(id, b.dataset.tag || null);
    });
    document.addEventListener('mousedown', (e) => { if (!ctx.contains(e.target)) closeCtx(); }, true);
    document.addEventListener('keydown', (e) => { if (e.key === 'Escape') { closeCtx(); $('#recPanel').classList.add('hidden'); } });
    window.addEventListener('blur', () => { if (!window.__tgDemo) closeCtx(); });

    // ---- recurring panel ----
    const panel = $('#recPanel'), list = $('#recList'), txt = $('#recText'), time = $('#recTime'), daysEl = $('#recDays');
    const save = $('#recSave'), cancel = $('#recCancel');
    let recs = [], editId = null, days = new Set();
    ORDER.forEach((d) => { const b = document.createElement('button'); b.type = 'button'; b.className = 'rec-day'; b.textContent = DAY[d]; b.dataset.d = d;
      b.title = '仅在选中的星期刷新（都不选＝每天）';
      b.addEventListener('click', () => { days.has(d) ? days.delete(d) : days.add(d); b.classList.toggle('on', days.has(d)); }); daysEl.appendChild(b); });
    const paintDays = () => daysEl.querySelectorAll('.rec-day').forEach((b) => b.classList.toggle('on', days.has(+b.dataset.d)));
    const dayLabel = (ds) => !ds || !ds.length || ds.length === 7 ? '每天'
      : (ds.length === 5 && [1,2,3,4,5].every((d) => ds.includes(d))) ? '工作日'
      : (ds.length === 2 && ds.includes(0) && ds.includes(6)) ? '周末'
      : '周' + ORDER.filter((d) => ds.includes(d)).map((d) => DAY[d]).join('');
    const resetForm = () => { editId = null; txt.value = ''; time.value = '09:00'; days = new Set(); paintDays(); save.textContent = '添加'; cancel.classList.add('hidden'); paintList(); };
    function paintList() {
      list.innerHTML = '';
      if (!recs.length) { const e = document.createElement('li'); e.className = 'rec-empty'; e.textContent = '还没有常驻任务。到点会自动加入待办。'; list.appendChild(e); return; }
      [...recs].sort((a, b) => a.time.localeCompare(b.time)).forEach((r) => {
        const li = document.createElement('li'); li.className = 'rec-item' + (r.id === editId ? ' editing' : '');
        li.innerHTML = '<span class="rt"></span><span class="rx"></span><span class="rd"></span>';
        li.querySelector('.rt').textContent = r.time; li.querySelector('.rx').textContent = r.text; li.querySelector('.rd').textContent = dayLabel(r.days);
        const del = document.createElement('button'); del.className = 'icon-btn'; del.type = 'button'; del.title = '删除';
        del.innerHTML = '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>';
        del.addEventListener('click', async (e) => { e.stopPropagation(); recs = recs.filter((x) => x.id !== r.id); if (editId === r.id) editId = null; await push(); });
        li.appendChild(del);
        li.addEventListener('click', () => { editId = r.id; txt.value = r.text; time.value = r.time; days = new Set(r.days || []); paintDays();
          save.textContent = '保存'; cancel.classList.remove('hidden'); paintList(); txt.focus(); });
        list.appendChild(li);
      });
    }
    async function push() { const r = await API().setRecurring(recs); recs = r.items || []; paintList(); }
    save.addEventListener('click', async () => {
      const t = txt.value.trim(); if (!t) { txt.focus(); return; }
      const entry = { text: t, time: time.value || '09:00', days: [...days].sort() };
      if (editId) { const r = recs.find((x) => x.id === editId); Object.assign(r, entry);
        if (r._origTime !== undefined) delete r._origTime; }
      else recs.push(entry);
      await push(); resetForm();
    });
    txt.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.isComposing && e.keyCode !== 229 && !e.repeat) save.click(); });
    cancel.addEventListener('click', resetForm);
    const open = async () => { $('#settings').classList.add('hidden'); try { recs = (await API().getRecurring()).items || []; } catch (_) { recs = []; } resetForm(); panel.classList.remove('hidden'); };
    $('#openRec').addEventListener('click', () => panel.classList.contains('hidden') ? open() : panel.classList.add('hidden'));
    $('#closeRec').addEventListener('click', () => panel.classList.add('hidden'));
    $('#openSettings').addEventListener('click', () => panel.classList.add('hidden'));
    API().onRecurring((r) => { if (!editId) { recs = r.items || []; if (!panel.classList.contains('hidden')) paintList(); } });
    // keep "+" tabbable only when the add row is open
    new MutationObserver(() => { $('#openRec').tabIndex = $('#addBlock').classList.contains('collapsed') ? -1 : 0; })
      .observe($('#addBlock'), { attributes: true, attributeFilter: ['class'] });
    if (window.LiquidGlass) {
      const s = getComputedStyle(document.documentElement);
      window.LiquidGlass.wrap(panel, { cornerRadius: 22, displacementScale: 64, blurAmount: 0.25, saturation: 140, aberrationIntensity: 2, elasticity: 0, shadow: false });
      panel.style.borderRadius = 'var(--pr, 22px)';
    }
  });
})();
// demo states for verification screenshots (TG_DEMO=add|rec|ctx)
window.addEventListener('load', () => setTimeout(async () => {
  const d = ((await window.todoAPI.getStatus()) || {}).demo || ''; if (!d) return; window.__tgDemo = true;
  const q = (s) => document.querySelector(s);
  if (/add|rec/.test(d) && q('#addBlock').classList.contains('collapsed')) q('#toggleAdd').click();
  if (/rec/.test(d)) setTimeout(() => q('#openRec').click(), 400);
  if (/ctx/.test(d)) { const it = q('.todo-item:nth-child(2)') || q('.todo-item'); if (it) { const r = it.getBoundingClientRect();
    it.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: r.left + r.width * .55, clientY: r.top + r.height * .6 })); } }
}, 600));
/* pointer-based drag reorder (HTML5 DnD is swallowed by the WebView2 host) */
(function () {
  let d = null;
  const list = () => document.getElementById('list');
  const items = () => [...list().children].filter((n) => n.classList.contains('todo-item'));
  function cleanup() {
    if (!d) return null;
    const cur = d; d = null;
    try { const L = list(); if (L.hasPointerCapture(cur.pid)) L.releasePointerCapture(cur.pid); } catch (_) {}
    cur.el.classList.remove('dragging'); document.documentElement.classList.remove('reordering');
    cur.el.style.transition = 'transform .16s ease-out'; cur.el.style.transform = '';
    setTimeout(() => { cur.el.style.transition = ''; }, 180);
    return cur;
  }
  async function finish() {
    const cur = cleanup(); if (!cur || !cur.started) return;
    const stop = (ev) => { ev.stopPropagation(); ev.preventDefault(); };
    window.addEventListener('click', stop, true); setTimeout(() => window.removeEventListener('click', stop, true), 60);
    const ids = items().map((n) => n.dataset.id);
    if (ids.join() !== cur.before) await window.reorderIds(ids);
  }
  document.addEventListener('pointerdown', (e) => {
    if (d) finish();                           // stale state from a lost release: clear it first
    if (e.button !== 0 || e.pointerType !== 'mouse' && e.pointerType !== 'pen' && e.pointerType !== 'touch') return;
    const el = e.target.closest && e.target.closest('.todo-item');
    if (!el || e.target.closest('.circle,button,input,textarea') || el.classList.contains('completing')) return;
    if (window.isAnimating && window.isAnimating(el.dataset.id)) return;
    e.preventDefault();
    d = { el, pid: e.pointerId, y0: e.clientY, started: false, before: items().map((n) => n.dataset.id).join() };
    // capture on the (never-moving) list: capturing on the card itself is lost as soon as the card is re-inserted
    try { list().setPointerCapture(e.pointerId); } catch (_) {}
  });
  document.addEventListener('pointermove', (e) => {
    if (!d || e.pointerId !== d.pid) return;
    if (e.buttons === 0) { finish(); return; } // button was released somewhere we didn't hear about
    const dy = e.clientY - d.y0;
    if (!d.started) {
      if (Math.abs(dy) < 4) return;
      d.started = true; d.el.classList.add('dragging'); document.documentElement.classList.add('reordering');
      d.base = d.el.offsetTop;
    }
    const L = list(), el = d.el;
    const sibs = items().filter((n) => n !== el && !n.classList.contains('shattering'));
    let target = null;
    for (const n of sibs) { const r = n.getBoundingClientRect(); if (e.clientY < r.top + r.height / 2) { target = n; break; } }
    if (target !== el.nextElementSibling && target !== el) {
      const before = new Map(sibs.map((n) => [n, n.getBoundingClientRect().top]));
      if (target) L.insertBefore(el, target); else L.appendChild(el);
      sibs.forEach((n) => { const dt = before.get(n) - n.getBoundingClientRect().top; if (dt) {
        n.style.transition = 'none'; n.style.transform = `translateY(${dt}px)`;
        requestAnimationFrame(() => { n.style.transition = 'transform .18s cubic-bezier(.3,1,.4,1)'; n.style.transform = ''; }); } });
    }
    el.style.transform = `translateY(${dy - (el.offsetTop - d.base)}px)`;
  });
  document.addEventListener('pointerup', (e) => { if (d && e.pointerId === d.pid) finish(); });
  document.addEventListener('pointercancel', () => finish());
  document.addEventListener('lostpointercapture', (e) => { if (d && e.pointerId === d.pid) finish(); }, true);
  window.addEventListener('blur', () => finish());
  document.addEventListener('visibilitychange', () => { if (document.hidden) finish(); });
})();
window.addEventListener('load', () => setTimeout(async () => {
  const d = ((await window.todoAPI.getStatus()) || {}).demo || '';
  if (/setbottom/.test(d)) { document.querySelector('#settings').classList.remove('hidden');
    setTimeout(() => { const b = document.querySelector('#settings .panel-body'); b.scrollTop = b.scrollHeight; }, 400); }
  if (/recbottom/.test(d)) setTimeout(() => { const b = document.querySelector('#recPanel .panel-body'); b.scrollTop = b.scrollHeight; }, 1500);
}, 700));
