'use strict';

(function () {
  const canvas = () => document.getElementById('fx');
  let particles = [];
  let shards = [];
  let raf = null;

  function resize() {
    const c = canvas();
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.floor(window.innerWidth * dpr);
    c.height = Math.floor(window.innerHeight * dpr);
    c.style.width = window.innerWidth + 'px';
    c.style.height = window.innerHeight + 'px';
    const ctx = c.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  let rzT; window.addEventListener('resize', () => { clearTimeout(rzT); rzT = setTimeout(resize, 150); });
  window.addEventListener('DOMContentLoaded', resize);

  function colors(n) {
    const palette = ['#6eb5ff', '#5dde8a', '#ffd166', '#ff6b8a', '#c3a6ff', '#ffffff'];
    const out = [];
    for (let i = 0; i < n; i++) out.push(palette[i % palette.length]);
    return out;
  }

  function burst(x, y, opts) {
    opts = opts || {};
    const count = opts.count || 28;
    const speed = opts.speed || 3.2;
    const life = opts.life || 50;
    const cols = colors(count);
    for (let i = 0; i < count; i++) {
      const ang = (Math.PI * 2 * i) / count + Math.random() * 0.4;
      const sp = speed * (0.55 + Math.random() * 0.7);
      particles.push({
        x, y,
        vx: Math.cos(ang) * sp,
        vy: Math.sin(ang) * sp - 0.6,
        life,
        max: life,
        r: 1.5 + Math.random() * 2,
        color: cols[i],
        g: 0.06 + Math.random() * 0.04
      });
    }
    kick();
  }

  function bigBurst(cx, cy) {
    for (let k = 0; k < 5; k++) {
      setTimeout(() => {
        burst(
          cx + (Math.random() - 0.5) * 80,
          cy + (Math.random() - 0.5) * 60,
          { count: 40, speed: 4.5, life: 70 }
        );
      }, k * 90);
    }
    // secondary ring
    setTimeout(() => burst(cx, cy, { count: 60, speed: 5.5, life: 80 }), 200);
  }

  /** Shatter a DOM element into falling pixels */
  function shatterElement(el, onDone, floorEl) {
    const c = canvas();
    if (!el || !c) { if (onDone) onDone(); return; }
    const rect = el.getBoundingClientRect();
    const w = Math.max(1, Math.floor(rect.width));
    const h = Math.max(1, Math.floor(rect.height));

    // paint text into offscreen canvas for pixel sampling
    const off = document.createElement('canvas');
    off.width = w;
    off.height = h;
    const octx = off.getContext('2d');
    octx.fillStyle = 'rgba(255,255,255,0.85)';
    octx.font = '13px "Segoe UI", "Microsoft YaHei", sans-serif';
    octx.textBaseline = 'middle';
    const text = (el.querySelector('.todo-text') || el).textContent || '';
    octx.fillText(text.slice(0, 40), 28, h / 2);

    const step = Math.max(2, Math.floor(Math.min(w, h) / 18));
    const data = octx.getImageData(0, 0, w, h).data;
    const palette = ['rgba(255,255,255,.92)', 'rgba(255,255,255,.78)', 'rgba(246,248,255,.85)', 'rgba(255,255,255,.65)'];

    for (let y = 0; y < h; y += step) {
      for (let x = 0; x < w; x += step) {
        const i = (y * w + x) * 4;
        const a = data[i + 3];
        // also spawn some background glass shards even if empty
        if (a < 20 && Math.random() > 0.18) continue;
        const px = rect.left + x + step / 2;
        const py = rect.top + y + step / 2;
        shards.push({
          x: px,
          y: py,
          vx: (Math.random() - 0.5) * 2.8,
          vy: -0.5 - Math.random() * 2.2,
          life: 150 + Math.random() * 40,
          max: 190,
          floorEl: floorEl || null,
          resting: false,
          size: step * (0.7 + Math.random() * 0.6),
          color: palette[(x + y) % palette.length],
          rot: Math.random() * Math.PI,
          vr: (Math.random() - 0.5) * 0.25,
          g: 0.18 + Math.random() * 0.1
        });
      }
    }
    kick();
    setTimeout(() => { if (onDone) onDone(); }, 2100);
  }

  const K = 1 / 3; // shard time scale: 3x slower than the original
  function kick() {
    if (raf) return;
    raf = requestAnimationFrame(tick);
  }

  function tick() {
    const c = canvas();
    if (!c) { raf = null; return; }
    const ctx = c.getContext('2d');
    ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);

    for (let i = particles.length - 1; i >= 0; i--) {
      const p = particles[i];
      p.vy += p.g;
      p.x += p.vx;
      p.y += p.vy;
      p.vx *= 0.99;
      p.life -= 1;
      if (p.life <= 0) { particles.splice(i, 1); continue; }
      const alpha = Math.max(0, p.life / p.max);
      ctx.globalAlpha = alpha;
      ctx.fillStyle = p.color;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
      ctx.fill();
    }

    const bottom = window.innerHeight - 8;
    const floorCache = new Map();
    const floorFor = (s) => {
      const el = s.floorEl;
      if (!el || !el.isConnected) return bottom;
      let r = floorCache.get(el);
      if (!r) { r = el.getBoundingClientRect(); floorCache.set(el, r); }
      // only powder above the card lands on it; anything past its sides falls to the bottom
      if (s.x < r.left + 2 || s.x > r.right - 2 || r.height < 4) return bottom;
      return r.top - 0.5;
    };
    for (let i = shards.length - 1; i >= 0; i--) {
      const s = shards[i];
      const floor = floorFor(s) - s.size / 2;
      if (s.resting) {
        // ride along with the card as it moves up; gently settle
        s.y = floor;
        s.vx *= 0.9; s.x += s.vx * K;
      } else {
        s.vy += s.g * K;
        s.x += s.vx * K;
        s.y += s.vy * K;
        s.rot += s.vr * K;
        if (s.y >= floor) {
          s.y = floor;
          if (Math.abs(s.vy) < 1.2) { s.resting = true; s.vy = 0; s.rot = Math.round(s.rot / (Math.PI / 2)) * (Math.PI / 2); }
          else { s.vy *= -0.25; s.vx *= 0.6; }
        }
      }
      s.life -= K;
      if (s.life <= 0) { shards.splice(i, 1); continue; }
      const t = Math.min(1, s.life / 70); const alpha = t * t * (3 - 2 * t); // smoothstep fade
      ctx.save();
      ctx.globalAlpha = alpha;
      ctx.translate(s.x, s.y);
      ctx.rotate(s.rot);
      ctx.fillStyle = s.color;
      ctx.fillRect(-s.size / 2, -s.size / 2, s.size, s.size);
      ctx.restore();
    }

    ctx.globalAlpha = 1;
    if (particles.length || shards.length) {
      raf = requestAnimationFrame(tick);
    } else {
      raf = null;
      ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
    }
  }

  window.FX = { burst, bigBurst, shatterElement, resize };
})();
