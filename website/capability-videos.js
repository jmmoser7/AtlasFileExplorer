const album = document.querySelector('.demo-album');
const stage = album.querySelector('#demo-stage');
const deck = stage.querySelector('.demo-deck');
const player = stage.querySelector('.demo-player');
const video = player.querySelector('video');
const currentPoster = player.querySelector('.demo-current-poster');
const fallback = player.querySelector('.demo-fallback');
const playback = stage.querySelector('[data-demo-playback]');
const captions = stage.querySelector('[data-demo-captions]');
const announcement = album.querySelector('[data-demo-status]');
const titles = Array.from(album.querySelectorAll('[data-demo-copy]'));
const slides = Array.from(stage.querySelectorAll('[data-demo-slide]'), (panel, index) => ({
  panel,
  source: panel.dataset.videoSrc,
  title: titles[index],
}));
const available = slides.filter(({ source }) => source);
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
const theme = getComputedStyle(album);
const rack = name => Number(theme.getPropertyValue('--demo-rack-' + name));
const tuning = {
  sideStep: rack('side-step-frac'),
  centerBulge: rack('center-bulge-frac'),
  bulgeWidth: rack('bulge-width'),
  yaw: rack('angle-max-deg'),
  yawWidth: rack('angle-width'),
  depth: rack('depth-max'),
  depthWidth: rack('depth-width'),
  focal: rack('focal'),
  bevel: rack('corner-bevel-frac'),
  stiffness: rack('spring-stiffness'),
  damping: rack('spring-damping'),
};

let index = 0;
let playing = !reducedMotion.matches;
let pausedByUser = false;
let inView = false;
let showCaptions = false;
let playRequest = 0;
let swipeStart;
let swipeAxis;
let coverWidth = 0;
let playerWidth = 0;
let stageWidth = 0;
let depthScale = 1;
let position = 0;
let target = 0;
let velocity = 0;
let frame = 0;
let frameTime = 0;

// Same wrapped position and sigmoidal spacing, yaw and depth as Slate Home.
function paintRack() {
  const count = available.length;
  for (const [slot, { panel }] of available.entries()) {
    const distance = slot - position;
    const offset = distance - Math.round(distance / count) * count;
    const abs = Math.abs(offset);
    panel.hidden = abs > 3.1;
    if (panel.hidden && slot !== index) continue;
    const x = coverWidth * (tuning.sideStep * offset + tuning.centerBulge * Math.tanh(offset / tuning.bulgeWidth));
    const yaw = tuning.yaw * Math.tanh(offset / tuning.yawWidth);
    const z = -tuning.depth * depthScale * Math.tanh(abs / tuning.depthWidth);
    panel.style.transform = `translate(-50%, -50%) translate3d(${x}px, 0, ${z}px) rotateY(${yaw}deg)`;
    panel.style.zIndex = String(1000 - Math.round(abs * 100));
    if (slot === index) {
      const scale = tuning.focal * depthScale / (tuning.focal * depthScale - z);
      const scaleX = Math.cos(yaw * Math.PI / 180) * scale;
      const edge = Math.max(0, (stageWidth + playerWidth * scaleX) / 2 - 48);
      const projectedX = Math.max(-edge, Math.min(edge, x * scale));
      player.style.transform = `translate(-50%, -50%) translateX(${projectedX}px) scale(${scaleX}, ${scale})`;
    }
  }
}

function sizeRack() {
  const width = stage.clientWidth;
  const card = parseFloat(getComputedStyle(player).width);
  stageWidth = width;
  playerWidth = card;
  // Keep the same perspective proportions on a narrow screen.
  depthScale = Math.min(1, width / 700);
  coverWidth = card * (0.85 + 0.15 * depthScale);
  deck.style.setProperty('--demo-focal', `${tuning.focal * depthScale}px`);
  stage.style.setProperty('--demo-radius', `${card * tuning.bevel}px`);
  paintRack();
}

function settleRack() {
  cancelAnimationFrame(frame);
  frame = 0;
  frameTime = 0;
  position = target = index;
  velocity = 0;
  paintRack();
}

function springRack(time) {
  const dt = frameTime ? Math.min((time - frameTime) / 1000, 0.05) : 1 / 60;
  frameTime = time;
  velocity += ((target - position) * tuning.stiffness - velocity * tuning.damping) * dt;
  position += velocity * dt;
  if (Math.abs(target - position) < 0.001 && Math.abs(velocity) < 0.02) {
    settleRack();
    return;
  }
  paintRack();
  frame = requestAnimationFrame(springRack);
}

function updatePlayback(userInitiated = false) {
  const request = ++playRequest;
  stage.toggleAttribute('data-paused', !playing);
  playback.textContent = playing ? 'Pause' : 'Play';
  playback.setAttribute('aria-label', playing ? 'Pause demonstrations' : 'Play demonstrations');
  playback.hidden = !available.length || Boolean(video.error);
  if (!available.length) return;
  if (!playing || (!inView && !userInitiated) || document.hidden || video.error) {
    video.pause();
    return;
  }
  video.muted = true;
  video.play().catch(error => {
    if (request !== playRequest || error.name === 'AbortError') return;
    playing = false;
    updatePlayback();
  });
}

function updateCaptions() {
  const track = video.textTracks[0];
  captions.hidden = !track;
  captions.setAttribute('aria-pressed', String(showCaptions));
  if (track) track.mode = showCaptions ? 'showing' : 'disabled';
}

let lastTap = null;
let touchStartedAt = 0;

function select(nextIndex, announce = false) {
  lastTap = null;
  if (!available.length) return;
  const count = available.length;
  let step = nextIndex - index;
  if (Math.abs(step) > 1) step -= Math.round(step / count) * count;
  video.pause();
  player.removeAttribute('data-video-ready');
  fallback.hidden = true;

  index = (nextIndex + count) % count;
  target += step;
  const active = available[index];
  active.panel.hidden = false;
  const poster = active.panel.querySelector('img')?.getAttribute('src');
  currentPoster.hidden = !poster;
  if (poster) {
    currentPoster.src = poster;
    video.poster = poster;
  } else {
    currentPoster.removeAttribute('src');
    video.removeAttribute('poster');
  }
  video.dataset.demo = active.panel.dataset.demoSlide;
  video.setAttribute('aria-label', active.title.querySelector('h3').textContent + ' demonstration');
  const description = active.panel.getAttribute('aria-describedby');
  if (description) video.setAttribute('aria-describedby', description);
  else video.removeAttribute('aria-describedby');
  video.replaceChildren();
  if (active.panel.dataset.captions) {
    const track = document.createElement('track');
    track.kind = 'captions';
    track.srclang = 'en';
    track.label = 'English';
    track.src = active.panel.dataset.captions;
    video.append(track);
  }
  // Keep one decoder and autoplay permission across every clip.
  video.src = active.source;
  video.load();
  for (const slide of slides) {
    const selected = slide === active;
    slide.panel.toggleAttribute('data-active', selected);
    slide.panel.setAttribute('aria-hidden', String(!selected));
    slide.title.hidden = !selected;
  }
  paintRack();
  if (reducedMotion.matches || count === 1) settleRack();
  else if (!frame && position !== target) frame = requestAnimationFrame(springRack);
  if (step && !reducedMotion.matches) active.title.animate([{ opacity: 0 }, { opacity: 1 }], 120);
  if (announce) {
    const name = active.title.querySelector('h3').textContent;
    announcement.textContent = name + ', ' + (index + 1) + ' of ' + available.length;
  }
  updateCaptions();
  updatePlayback(announce);
}

playback.addEventListener('click', () => {
  playing = !playing;
  pausedByUser = !playing;
  updatePlayback(true);
});
captions.addEventListener('click', () => {
  showCaptions = !showCaptions;
  updateCaptions();
});

stage.addEventListener('focusin', event => {
  if (event.target === stage && stage.matches(':focus-visible') && playing) {
    playing = false;
    updatePlayback();
  }
});
stage.addEventListener('keydown', event => {
  if (available.length < 2 || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  switch (event.key) {
    case 'ArrowLeft': select(index - 1, true); break;
    case 'ArrowRight': select(index + 1, true); break;
    case 'Home': select(0, true); break;
    case 'End': select(available.length - 1, true); break;
    default: return;
  }
  event.preventDefault();
});
function finishSwipe(event, end) {
  const start = swipeStart;
  const axis = swipeAxis;
  swipeStart = null;
  swipeAxis = null;
  if (!start || axis === 'vertical' || available.length < 2) return;
  const dx = end.clientX - start.clientX;
  const dy = end.clientY - start.clientY;
  if (Math.abs(dx) < 36 || Math.abs(dx) < 1.4 * Math.abs(dy)) return;
  lastTap = null;
  event.preventDefault();
  // Retry playback on a fresh gesture after a browser autoplay denial.
  if (!pausedByUser) playing = true;
  select(index + (dx < 0 ? 1 : -1), true);
}

stage.addEventListener('touchstart', event => {
  swipeStart = event.touches.length === 1 && !event.target.closest('button') ? event.touches[0] : null;
  swipeAxis = null;
  touchStartedAt = event.timeStamp;
  if (!swipeStart) lastTap = null;
}, { passive: true });
stage.addEventListener('touchmove', event => {
  if (!swipeStart || event.touches.length !== 1) return;
  const dx = Math.abs(event.touches[0].clientX - swipeStart.clientX);
  const dy = Math.abs(event.touches[0].clientY - swipeStart.clientY);
  if (!swipeAxis && Math.max(dx, dy) >= 4) swipeAxis = dx > dy ? 'horizontal' : 'vertical';
  if (swipeAxis === 'horizontal' && event.cancelable) event.preventDefault();
}, { passive: false });
stage.addEventListener('touchcancel', () => {
  swipeStart = null;
  swipeAxis = null;
  lastTap = null;
});
stage.addEventListener('touchend', event => {
  const end = event.changedTouches[0];
  const isTap = swipeStart && !swipeAxis && event.touches.length === 0 &&
    event.timeStamp - touchStartedAt <= 300 &&
    Math.hypot(end.clientX - swipeStart.clientX, end.clientY - swipeStart.clientY) < 12;
  finishSwipe(event, end);
  if (!isTap || !Number.isFinite(video.duration)) {
    lastTap = null;
    return;
  }

  const { left, right, top, bottom, width } = player.getBoundingClientRect();
  const { clientX: x, clientY: y } = end;
  if (x < left || x > right || y < top || y > bottom) {
    lastTap = null;
    return;
  }
  const side = x < left + width / 2 ? -1 : 1;
  if (lastTap?.side === side && event.timeStamp - lastTap.time <= 350 &&
      Math.hypot(x - lastTap.x, y - lastTap.y) <= 40) {
    lastTap = { side, time: event.timeStamp, x, y };
    if (event.cancelable) event.preventDefault();
    video.currentTime = Math.max(0, Math.min(video.duration, video.currentTime + side * 5));
    return;
  }
  lastTap = { side, time: event.timeStamp, x, y };
}, { passive: false });
stage.addEventListener('pointerdown', event => {
  if (event.pointerType !== 'mouse' || event.button !== 0 || event.target.closest('button')) return;
  swipeStart = event;
  swipeAxis = null;
  stage.setPointerCapture(event.pointerId);
});
stage.addEventListener('pointerup', event => {
  if (event.pointerType === 'mouse') finishSwipe(event, event);
});
stage.addEventListener('pointercancel', event => {
  if (event.pointerType === 'mouse') swipeStart = null;
});

video.addEventListener('playing', () => {
  const source = video.currentSrc;
  const reveal = () => {
    const selectedSource = new URL(available[index].source, document.baseURI).href;
    if (video.currentSrc === source && source === selectedSource && video.readyState >= 2 && !video.paused) {
      player.setAttribute('data-video-ready', '');
    }
  };
  if (video.requestVideoFrameCallback) video.requestVideoFrameCallback(reveal);
  else reveal();
});
video.addEventListener('ended', () => {
  if (playing && inView && !document.hidden) select(index + 1);
});
video.addEventListener('error', () => {
  if (!video.error) return;
  fallback.hidden = false;
  updatePlayback();
});

if (available.length) {
  sizeRack();
  select(0);
  if (available.length === 1) stage.setAttribute('aria-label', 'Video demonstration');
  new IntersectionObserver(([entry]) => {
    inView = entry.isIntersecting;
    updatePlayback();
  }, { threshold: .2 }).observe(stage);
  new ResizeObserver(sizeRack).observe(stage);
  document.addEventListener('visibilitychange', () => updatePlayback());
  reducedMotion.addEventListener('change', event => {
    if (event.matches) {
      settleRack();
      playing = false;
      updatePlayback();
    }
  });
} else {
  player.hidden = true;
}
