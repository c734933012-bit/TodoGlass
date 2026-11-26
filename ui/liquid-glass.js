'use strict';
/*
 * Vanilla port of rdev/liquid-glass-react v1.1.1 (MIT) — same filter graph,
 * displacement map, blur/saturate formula, border layers and elastic mouse math.
 */
(function () {
  const NS = 'http://www.w3.org/2000/svg';
  const registry = new Set();
  let uid = 0;
  let mouse = { x: 0, y: 0 };

  function svgEl(tag, attrs, parent) {
    const e = document.createElementNS(NS, tag);
    for (const k in attrs) e.setAttribute(k, attrs[k]);
    if (parent) parent.appendChild(e);
    return e;
  }

  // GlassFilter (standard mode)
  function makeFilter(id, displacementScale, aberrationIntensity) {
    const svg = svgEl('svg', { 'aria-hidden': 'true', width: '0', height: '0' });
    svg.style.position = 'absolute';
    const defs = svgEl('defs', {}, svg);
    const f = svgEl('filter', { id, x: '-35%', y: '-35%', width: '170%', height: '170%', 'color-interpolation-filters': 'sRGB' }, defs);
    svgEl('feImage', { x: '0', y: '0', width: '100%', height: '100%', result: 'DISPLACEMENT_MAP', href: window.LG_DISPLACEMENT_MAP, preserveAspectRatio: 'xMidYMid slice' }, f);
    svgEl('feColorMatrix', { in: 'DISPLACEMENT_MAP', type: 'matrix', values: '0.3 0.3 0.3 0 0 0.3 0.3 0.3 0 0 0.3 0.3 0.3 0 0 0 0 0 1 0', result: 'EDGE_INTENSITY' }, f);
    const ct = svgEl('feComponentTransfer', { in: 'EDGE_INTENSITY', result: 'EDGE_MASK' }, f);
    svgEl('feFuncA', { type: 'discrete', tableValues: `0 ${aberrationIntensity * 0.05} 1` }, ct);
    svgEl('feOffset', { in: 'SourceGraphic', dx: '0', dy: '0', result: 'CENTER_ORIGINAL' }, f);
    const ch = [
      ['RED', -1, '1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0'],
      ['GREEN', -1 - aberrationIntensity * 0.05, '0 0 0 0 0 0 1 0 0 0 0 0 0 0 0 0 0 0 1 0'],
      ['BLUE', -1 - aberrationIntensity * 0.1, '0 0 0 0 0 0 0 0 0 0 0 0 1 0 0 0 0 0 1 0']
    ];
    for (const [name, mul, m] of ch) {
      svgEl('feDisplacementMap', { in: 'SourceGraphic', in2: 'DISPLACEMENT_MAP', scale: String(displacementScale * mul), xChannelSelector: 'R', yChannelSelector: 'B', result: name + '_DISPLACED' }, f);
      svgEl('feColorMatrix', { in: name + '_DISPLACED', type: 'matrix', values: m, result: name + '_CHANNEL' }, f);
    }
    svgEl('feBlend', { in: 'GREEN_CHANNEL', in2: 'BLUE_CHANNEL', mode: 'screen', result: 'GB_COMBINED' }, f);
    svgEl('feBlend', { in: 'RED_CHANNEL', in2: 'GB_COMBINED', mode: 'screen', result: 'RGB_COMBINED' }, f);
    svgEl('feGaussianBlur', { in: 'RGB_COMBINED', stdDeviation: String(Math.max(0.1, 0.5 - aberrationIntensity * 0.1)), result: 'ABERRATED_BLURRED' }, f);
    svgEl('feComposite', { in: 'ABERRATED_BLURRED', in2: 'EDGE_MASK', operator: 'in', result: 'EDGE_ABERRATION' }, f);
    const inv = svgEl('feComponentTransfer', { in: 'EDGE_MASK', result: 'INVERTED_MASK' }, f);
    svgEl('feFuncA', { type: 'table', tableValues: '1 0' }, inv);
    svgEl('feComposite', { in: 'CENTER_ORIGINAL', in2: 'INVERTED_MASK', operator: 'in', result: 'CENTER_CLEAN' }, f);
    svgEl('feComposite', { in: 'EDGE_ABERRATION', in2: 'CENTER_CLEAN', operator: 'over' }, f);
    return svg;
  }

  const BORDER_SHADOW = '0 0 0 0.5px rgba(255, 255, 255, 0.5) inset, 0 1px 3px rgba(255, 255, 255, 0.25) inset, 0 1px 4px rgba(0, 0, 0, 0.35)';

  function borderBg(o, a1, a2) {
    return `linear-gradient(${135 + o.x * 1.2}deg, rgba(255,255,255,0) 0%, rgba(255,255,255,${a1 + Math.abs(o.x) * 0.008}) ${Math.max(10, 33 + o.y * 0.3)}%, rgba(255,255,255,${a2 + Math.abs(o.x) * 0.012}) ${Math.min(90, 66 + o.y * 0.4)}%, rgba(255,255,255,0) 100%)`;
  }

  /**
   * Turn an element into a liquid-glass surface.
   * opts: displacementScale(70) blurAmount(0.0625) saturation(140) aberrationIntensity(2)
   *       elasticity(0.15) cornerRadius(999) overLight(false) warp(true) shadow(true)
   */
  function wrap(el, opts) {
    const o = Object.assign({ displacementScale: 70, blurAmount: 0.0625, saturation: 140, aberrationIntensity: 2,
      elasticity: 0.15, cornerRadius: 999, overLight: false, warp: true, shadow: true }, opts || {});
    if (el.__lg) return el.__lg;
    el.classList.add('lg');
    el.style.borderRadius = o.cornerRadius + 'px';
    if (o.shadow) el.style.boxShadow = o.overLight ? '0px 16px 70px rgba(0,0,0,0.75)' : '0px 12px 40px rgba(0,0,0,0.25)';
    const layers = [];
    if (o.warp) {
      const id = 'lgf' + (++uid);
      const svg = makeFilter(id, o.overLight ? o.displacementScale * 0.5 : o.displacementScale, o.aberrationIntensity);
      svg.classList.add('lg-svg');
      const warp = document.createElement('span');
      warp.className = 'lg-warp';
      warp.style.filter = `url(#${id})`;
      warp.style.backdropFilter = `blur(${(o.overLight ? 12 : 4) + o.blurAmount * 32}px) saturate(${o.saturation}%)`;
      el.prepend(warp);
      el.prepend(svg);
      layers.push(svg, warp);
    }
    const b1 = document.createElement('span');
    const b2 = document.createElement('span');
    b1.className = 'lg-border lg-border-screen';
    b2.className = 'lg-border lg-border-overlay';
    for (const b of [b1, b2]) { b.style.boxShadow = BORDER_SHADOW; el.appendChild(b); }
    const state = { el, o, b1, b2, offset: { x: 0, y: 0 }, active: false, first: true, wasNear: false };
    el.__lg = state;
    if (o.elasticity > 0) {
      el.addEventListener('mousedown', () => { state.active = true; update(state); });
      window.addEventListener('mouseup', () => { if (state.active) { state.active = false; update(state); } });
    }
    if (el.classList.contains('todo-item')) {
      // Cards only respond to the pointer over them. No global per-card layout reads.
      b1.style.background = borderBg(state.offset, 0.12, 0.4);
      b2.style.background = borderBg(state.offset, 0.32, 0.6);
    } else { registry.add(state); update(state); }
    return state;
  }

  function update(s) {
    const { el, o } = s;
    if (!el.isConnected) { registry.delete(s); return; }
    const r = el.getBoundingClientRect();
    const zone0 = o.elasticity > 0 ? (o.activationZone || 200) : 0;
    const near = mouse.x >= r.left - zone0 && mouse.x <= r.right + zone0 && mouse.y >= r.top - zone0 && mouse.y <= r.bottom + zone0;
    if (!near && !s.wasNear && !o.trackOutside && !s.first) return;
    s.wasNear = near; s.first = false;
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const inside = mouse.x >= r.left && mouse.x <= r.right && mouse.y >= r.top && mouse.y <= r.bottom;
    // mouseOffset (percent from centre) — the library only tracks it while hovering the element/container
    if (inside || o.trackOutside) {
      s.offset = { x: (mouse.x - cx) / r.width * 100, y: (mouse.y - cy) / r.height * 100 };
    }
    const bg1 = borderBg(s.offset, 0.12, 0.4);
    if (bg1 !== s.lastBg) { s.lastBg = bg1; s.b1.style.background = bg1; s.b2.style.background = borderBg(s.offset, 0.32, 0.6); }

    if (o.elasticity > 0 && !el.classList.contains('dragging')) {
      const dx = mouse.x - cx, dy = mouse.y - cy;
      const ex = Math.max(0, Math.abs(dx) - r.width / 2), ey = Math.max(0, Math.abs(dy) - r.height / 2);
      const edge = Math.sqrt(ex * ex + ey * ey);
      const zone = o.activationZone || 200;
      const fade = !mouse.x && !mouse.y ? 0 : (edge > zone ? 0 : 1 - edge / zone);
      const tx = dx * o.elasticity * 0.1 * fade, ty = dy * o.elasticity * 0.1 * fade;
      let scale = 'scale(1)';
      const cd = Math.sqrt(dx * dx + dy * dy);
      if (s.active) scale = 'scale(0.96)';
      else if (fade > 0 && cd > 0) {
        const nx = dx / cd, ny = dy / cd;
        const k = Math.min(cd / 300, 1) * o.elasticity * fade;
        const sx = 1 + Math.abs(nx) * k * 0.3 - Math.abs(ny) * k * 0.15;
        const sy = 1 + Math.abs(ny) * k * 0.3 - Math.abs(nx) * k * 0.15;
        scale = `scaleX(${Math.max(0.8, sx)}) scaleY(${Math.max(0.8, sy)})`;
      }
      const tf = `translate(${tx.toFixed(1)}px, ${ty.toFixed(1)}px) ${scale}`;
      if (tf !== s.lastTf) { s.lastTf = tf; el.style.transform = tf; }
    }
  }

  let pointedCard = null;
  let raf = 0;
  function schedule() {
    if (raf) return;
    raf = requestAnimationFrame(() => { raf = 0; registry.forEach(update); if (pointedCard && pointedCard.el.isConnected) update(pointedCard); });
  }
  window.addEventListener('mousemove', (e) => { mouse = { x: e.clientX, y: e.clientY }; pointedCard = e.target.closest?.('.todo-item')?.__lg || null; schedule(); });
  document.addEventListener('mouseleave', () => { mouse = { x: 0, y: 0 }; pointedCard = null; schedule(); });
  window.addEventListener('resize', schedule);

  window.LiquidGlass = { wrap, refresh: schedule };
})();
