(function(){
  const T = window.__TAURI__, inv = async (name, args) => { try { return await T.core.invoke(name, args); } catch (e) { window.showTodoError(String(e)); throw e; } }, on = (n, cb) => { let un; T.event.listen(n, e => cb(e.payload)).then(u => un = u); return () => un && un(); };
  window.todoAPI = {
    listenState: (cb) => T.event.listen('state', e => cb(e.payload)),
    getStatus: () => inv('get_status'), getSettings: () => inv('get_settings'),
    setSettings: (patch) => inv('set_settings', { patch }), relaunch: () => {},
    onStatus: (cb) => on('status', cb), onFrost: () => () => {}, onState: (cb) => on('state', cb),
    getState: () => inv('get_state'), setGoal: (goal) => inv('set_goal', { goal }),
    addItem: (text) => inv('add_item', { text }), completeItem: (id) => inv('complete_item', { id }),
    removeItem: (id) => inv('remove_item', { id }), reorder: (ids) => inv('reorder', { ids }),
    updateText: (id, text) => inv('update_text', { id, text }),
    setTag: (id, tag) => inv('set_tag', { id, tag }), getRecurring: () => inv('get_recurring'),
    setRecurring: (items) => inv('set_recurring', { items }), reconnect: () => inv('reconnect'), onRecurring: (cb) => on('recurring', cb)
  };
  document.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    const tb = e.target.closest && e.target.closest('.titlebar');
    if (!tb || e.target.closest('button,input,textarea,.icon-btn')) return;
    // also ignore presses within 4px of a header button (elastic hover can shift it under the cursor)
    for (const btn of tb.querySelectorAll('.icon-btn')) { const r = btn.getBoundingClientRect();
      if (e.clientX > r.left - 4 && e.clientX < r.right + 4 && e.clientY > r.top - 4 && e.clientY < r.bottom + 4) return; }
    e.preventDefault();
    const x0 = e.clientX, y0 = e.clientY;
    const mv = (m) => { if (Math.abs(m.clientX - x0) + Math.abs(m.clientY - y0) > 3) { off(); T.window.getCurrentWindow().startDragging(); } };
    const off = () => { removeEventListener('mousemove', mv, true); removeEventListener('mouseup', off, true); };
    addEventListener('mousemove', mv, true); addEventListener('mouseup', off, true);
  });
  document.addEventListener('contextmenu', (e) => { if (!e.target.closest('input,textarea')) e.preventDefault(); });
})();
(function(){
  const E = 6, dirs = { n:'North', s:'South', e:'East', w:'West', ne:'NorthEast', nw:'NorthWest', se:'SouthEast', sw:'SouthWest' };
  const st = document.createElement('style');
  st.textContent = '.rz{position:fixed;z-index:99999}.rz.n,.rz.s{left:'+E*3+'px;right:'+E*3+'px;height:'+E+'px;cursor:ns-resize}.rz.n{top:0}.rz.s{bottom:0}.rz.e,.rz.w{top:'+E*3+'px;bottom:'+E*3+'px;width:'+E+'px;cursor:ew-resize}.rz.e{right:0}.rz.w{left:0}.rz.ne,.rz.nw,.rz.se,.rz.sw{width:'+E*3+'px;height:'+E*3+'px}.rz.ne{top:0;right:0;cursor:nesw-resize}.rz.sw{bottom:0;left:0;cursor:nesw-resize}.rz.nw{top:0;left:0;cursor:nwse-resize}.rz.se{bottom:0;right:0;cursor:nwse-resize}';
  document.head.appendChild(st);
  addEventListener('DOMContentLoaded', () => { for (const k in dirs) { const d = document.createElement('div'); d.className = 'rz ' + k;
    d.addEventListener('mousedown', (e) => { if (e.button) return; e.preventDefault(); e.stopPropagation(); window.__TAURI__.window.getCurrentWindow().startResizeDragging(dirs[k]); });
    document.body.appendChild(d); } });
})();
(function(){
  let on = true, t = 0;
  window.__TAURI__.core.invoke('get_settings').then(s => { on = s.rzFast !== false; });
  addEventListener('resize', () => {
    if (!on) return;
    document.documentElement.classList.add('resizing');
    clearTimeout(t); t = setTimeout(() => document.documentElement.classList.remove('resizing'), 150);
  }, true);
})();

window.showTodoError = function(message) {
  const banner = document.getElementById('saveError');
  banner.textContent = message;
  banner.classList.remove('hidden');
};
document.addEventListener('DOMContentLoaded', () => {
  const el = id => document.getElementById(id);
  const invoke = window.__TAURI__.core.invoke;
  window.__TAURI__.event.listen('storage-warning', e => window.showTodoError(String(e.payload)));
  el('saveError').onclick = () => el('saveError').classList.add('hidden');
  const hideKey = () => { el('pairKey').value = ''; el('pairKey').classList.add('hidden'); };
  el('advanced').addEventListener('toggle', () => { if (!el('advanced').open) hideKey(); });
  el('closeSettings').addEventListener('click', hideKey);
  el('pair').onclick = async () => {
    if (el('pairKey').value) { hideKey(); return; }
    try { el('pairKey').value = await invoke('sync_key'); el('pairKey').classList.remove('hidden'); el('pairKey').select(); }
    catch (e) { window.showTodoError(String(e)); }
  };
  el('restore').onclick = async () => {
    const select = el('backupFile');
    if (!confirm('還原「' + select.selectedOptions[0].text + '」的上一版備份？目前版本也會另外保留。')) return;
    try { await invoke('restore_backup', {name: select.value}); location.reload(); }
    catch (e) { window.showTodoError(String(e)); }
  };
});
