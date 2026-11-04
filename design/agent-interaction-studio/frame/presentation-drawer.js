(() => {
  'use strict';
  const deck = window.FrameDeck;
  const svgNS = 'http://www.w3.org/2000/svg';

  function make(tag, className, text) {
    const element = document.createElement(tag);
    if (className) element.className = className;
    if (text) element.textContent = text;
    return element;
  }

  function svgElement(tag, attrs) {
    const element = document.createElementNS(svgNS, tag);
    for (const [key, value] of Object.entries(attrs)) element.setAttribute(key, value);
    return element;
  }

  function create({canvas, getState, getCamera, getRevision, getTheme, transact, change, sort, members, remove, dismiss}) {
    const root = make('section', 'frame-presentation-drawer element-node');
    root.hidden = true;
    root.setAttribute('aria-label', 'Create Presentation');
    const bar = make('div', 'frame-presentation-bar');
    const selector = make('button', 'frame-presentation-select', 'Select presentation');
    selector.setAttribute('aria-haspopup', 'menu');
    const chevron = svgElement('svg', {class: 'frame-presentation-chevron', viewBox: '0 0 12 8', 'aria-hidden': 'true'});
    chevron.append(svgElement('path', {d: 'M2 2l4 4 4-4'}));
    selector.append(chevron);
    const add = make('button', 'frame-presentation-add', 'Add / remove frames');
    const toggle = make('span', 'frame-presentation-switch');
    toggle.setAttribute('aria-hidden', 'true');
    add.append(toggle);
    const inline = make('div', 'frame-presentation-inline');
    const count = make('span', 'frame-presentation-count');
    const sortButton = make('button', 'frame-presentation-sort');
    sortButton.setAttribute('aria-label', 'Sort frames on canvas');
    const sortIcon = svgElement('svg', {viewBox: '0 0 20 20', 'aria-hidden': 'true'});
    sortIcon.append(svgElement('path', {d: 'M3 3h5v5H3z M12 3h5v5h-5z M3 12h5v5H3z M12 12h5v5h-5z', fill: 'none', stroke: 'currentColor'}));
    sortButton.append(sortIcon, make('span', '', 'Sort on canvas'));
    const grid = make('div', 'frame-presentation-grid');
    const menu = make('div', 'frame-presentation-menu');
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Presentations');
    const map = make('div', 'frame-presentation-map');
    map.setAttribute('role', 'img');
    map.setAttribute('aria-label', 'Presentation frame positions on canvas');
    bar.append(selector, add, inline, count, sortButton);
    root.append(bar, grid, menu, map);
    canvas.append(root);

    let renderedKey = '', renderedDeck = '', anchor = null;
    let chosen = new Set(), tiles = [], dragging = null, insertion = null;

    function closeMenu() {
      menu.hidden = true;
      map.hidden = true;
      selector.setAttribute('aria-expanded', 'false');
    }
    closeMenu();

    function showMap(presentation) {
      const state = getState(), camera = getCamera();
      const frames = deck.slides(state, presentation);
      if (!frames.length) return;
      const corners = frames.flatMap(frame => FrameBaseline.corners(frame.rect, frame.rotation_deg));
      const xs = corners.map(([x]) => x), ys = corners.map(([, y]) => y);
      const x = Math.min(...xs), y = Math.min(...ys);
      const width = Math.max(1, Math.max(...xs) - x), height = Math.max(1, Math.max(...ys) - y);
      const pad = Math.max(width, height) * .06;
      const diagram = svgElement('svg', {viewBox: [x-pad,y-pad,width+2*pad,height+2*pad].join(' '), 'aria-hidden': 'true'});
      const activeId = state.selectedIds[0];
      for (const frame of frames) {
        const points = FrameBaseline.corners(frame.rect, frame.rotation_deg).map(pair => pair.join(',')).join(' ');
        const active = frame.id === activeId;
        diagram.append(svgElement('polygon', {points, fill: active ? '#00877e33' : 'none',
          stroke: active ? '#00877e' : '#9baeb9', 'stroke-width': active ? 1.7 : .75,
          'vector-effect': 'non-scaling-stroke'}));
      }
      diagram.append(svgElement('rect', {x: -camera.x/camera.zoom, y: -camera.y/camera.zoom,
        width: canvas.clientWidth/camera.zoom, height: canvas.clientHeight/camera.zoom, fill: 'none',
        stroke: '#607888', 'stroke-width': 1, 'stroke-dasharray': '3 3', 'vector-effect': 'non-scaling-stroke'}));
      map.replaceChildren(make('strong', '', 'Canvas map'), diagram);
      map.hidden = false;
      const roomRight = (canvas.clientWidth-root.offsetLeft)/camera.zoom;
      const beside = menu.offsetWidth + 8 + map.offsetWidth <= roomRight;
      map.style.left = beside ? menu.offsetWidth + 8 + 'px' : '0';
      const menuTop = menu.offsetTop;
      const roomBelow = (canvas.clientHeight-root.offsetTop-8)/camera.zoom;
      const roomAbove = (8-root.offsetTop)/camera.zoom;
      map.style.top = (beside ? Math.min(menuTop, roomBelow-map.offsetHeight)
        : Math.max(roomAbove, menuTop-map.offsetHeight-5)) + 'px';
    }

    function positionMenu() {
      const roomBelow = (canvas.clientHeight-root.offsetTop-8)/getCamera().zoom - bar.offsetHeight;
      menu.style.top = (roomBelow >= menu.offsetHeight ? bar.offsetHeight + 3 : -menu.offsetHeight - 4) + 'px';
    }

    function openMenu() {
      const state = getState();
      menu.replaceChildren();
      for (const presentation of state.presentations) {
        const entry = make('button', '', presentation.name);
        entry.setAttribute('role', 'menuitemradio');
        const active = state.activePresentationId === presentation.id;
        entry.setAttribute('aria-checked', String(active));
        if (active) {
          const check = make('span', '', '✓');
          check.setAttribute('aria-hidden', 'true');
          entry.append(check);
        }
        entry.onpointerenter = entry.onfocus = () => showMap(presentation);
        entry.onpointerleave = entry.onblur = () => { map.hidden = true; };
        entry.onclick = () => {
          closeMenu();
          change(state => { state.activePresentationId = presentation.id; });
          selector.focus();
        };
        menu.append(entry);
      }
      const newDeck = make('button', '', '+ New presentation');
      newDeck.setAttribute('role', 'menuitem');
      newDeck.onclick = () => {
        map.hidden = true;
        const form = make('form', 'frame-presentation-name-form');
        const name = make('input');
        name.placeholder = 'Presentation name';
        name.setAttribute('aria-label', 'Presentation name');
        name.required = true;
        name.maxLength = 80;
        form.append(name, make('button', '', 'Create'));
        form.onsubmit = event => {
          event.preventDefault();
          const title = name.value.trim();
          if (!title) return;
          closeMenu();
          transact('create presentation', () => {
            const state = getState();
            let number = state.presentations.length + 1;
            while (state.presentations.some(item => item.id === 'presentation-' + number)) number++;
            const id = 'presentation-' + number;
            state.presentations.push({id, name: title, frameIds: []});
            state.activePresentationId = id;
          });
          selector.focus();
        };
        menu.replaceChildren(form);
        positionMenu();
        name.focus();
      };
      menu.append(make('hr'), newDeck);
      menu.hidden = false;
      selector.setAttribute('aria-expanded', 'true');
      positionMenu();
    }

    selector.onclick = () => menu.hidden ? openMenu() : closeMenu();
    add.onclick = () => change(state => {
      state.deckMode = !state.deckMode;
      delete state.deckHoverId;
    }, false);
    sortButton.onclick = sort;
    document.addEventListener('pointerdown', event => {
      if (!root.contains(event.target)) closeMenu();
    }, true);
    root.addEventListener('keydown', event => {
      if (root.hidden) return;
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        dismiss();
        return;
      }
      if (event.key === ' ' && event.target.matches('button')) event.stopPropagation();
      if (event.key === 'Delete' && !event.target.matches('input')) {
        event.preventDefault();
        event.stopPropagation();
        if (chosen.size) {
          const ids = [...chosen];
          chosen.clear();
          anchor = null;
          remove(ids);
          selector.focus();
        }
      }
      if (event.key.startsWith('Arrow')) event.stopPropagation();
    });

    function updateSelection() {
      for (const tile of tiles) tile.setAttribute('aria-pressed', String(chosen.has(tile.dataset.slideId)));
    }
    function pick(event, id, order) {
      if (event.shiftKey && order.includes(anchor)) {
        const a = order.indexOf(anchor), b = order.indexOf(id);
        const range = order.slice(Math.min(a,b), Math.max(a,b)+1);
        chosen = new Set(event.ctrlKey || event.metaKey ? [...chosen, ...range] : range);
      } else if (event.ctrlKey || event.metaKey) {
        if (chosen.has(id)) chosen.delete(id); else chosen.add(id);
        anchor = id;
      } else {
        chosen = new Set([id]);
        anchor = id;
      }
      updateSelection();
    }

    function clearDrop() {
      if (!insertion) return;
      insertion = null;
      for (const tile of tiles) tile.style.transform = '';
    }

    function finishDrag() {
      dragging = null;
      clearDrop();
    }

    function previewDrop(tile, after) {
      if (insertion?.tile === tile && insertion.after === after) return;
      insertion = {tile, after};
      const {ids, slots} = dragging;
      const order = slots.map(slot => slot.tile.dataset.slideId);
      const positions = new Map(deck.insert(order, ids, tile.dataset.slideId, after).map((id,index) => [id, index]));
      for (const slot of slots) {
        const destination = slots[positions.get(slot.tile.dataset.slideId)];
        slot.tile.style.transform = 'translate(' + (destination.x-slot.x) + 'px,' + (destination.y-slot.y) + 'px)';
      }
    }

    function makeTile(frame, index, order, content) {
      const tile = make('button', 'frame-slide-tile');
      tile.dataset.slideId = frame.id;
      tile.draggable = true;
      tile.title = (index + 1) + ' · ' + frame.title;
      tile.setAttribute('aria-label', 'Slide ' + (index + 1) + ': ' + frame.title);
      tile.append(FrameRenderer.slidePreview(frame, content, getTheme()));
      tile.onclick = event => pick(event, frame.id, order);
      tile.ondragstart = event => {
        if (!chosen.has(frame.id)) pick(event, frame.id, order);
        const bounds = root.getBoundingClientRect(), zoom = getCamera().zoom;
        const slots = tiles.map(tile => {
          for (const animation of tile.getAnimations()) animation.cancel();
          const rect = tile.getBoundingClientRect();
          return {tile,x:(rect.left-bounds.left)/zoom,y:(rect.top-bounds.top)/zoom,
            w:rect.width/zoom,h:rect.height/zoom};
        });
        dragging = {ids:new Set(chosen),slots};
        event.dataTransfer.setData('text/plain', frame.id);
        event.dataTransfer.effectAllowed = 'move';
      };
      tile.ondragend = finishDrag;
      return tile;
    }

    root.addEventListener('dragover', event => {
      if (!dragging || !event.target.closest('.frame-presentation-grid,.frame-presentation-inline')) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = 'move';
      let nearest, distance = Infinity, center;
      const bounds = root.getBoundingClientRect(), zoom = getCamera().zoom;
      for (const slot of dragging.slots) {
        const x = bounds.left+(slot.x+slot.w/2)*zoom;
        const y = bounds.top+(slot.y+slot.h/2)*zoom;
        const dx = event.clientX - x;
        const dy = event.clientY - y;
        const next = dx*dx + dy*dy*4;
        if (next < distance) { nearest = slot.tile; distance = next; center = x; }
      }
      if (!nearest || dragging.ids.has(nearest.dataset.slideId)) { clearDrop(); return; }
      previewDrop(nearest, event.clientX > center);
    });
    root.addEventListener('dragleave', event => {
      if (!root.contains(event.relatedTarget)) clearDrop();
    });
    root.addEventListener('drop', event => {
      if (!dragging || !insertion) return;
      event.preventDefault();
      const {tile, after} = insertion, selection = dragging.ids;
      const presentation = deck.active(getState());
      const next = deck.insert(presentation.frameIds, selection, tile.dataset.slideId, after);
      const moved = next.some((id,index) => id !== presentation.frameIds[index]);
      const before = new Map(tiles.map(item => [item.dataset.slideId,item.getBoundingClientRect()]));
      finishDrag();
      if (!moved) return;
      transact('reorder presentation slides', () => {
        presentation.frameIds = next;
      });
      if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
        const zoom = getCamera().zoom;
        const shifts = tiles.map(item => {
          const prior = before.get(item.dataset.slideId), rect = item.getBoundingClientRect();
          return {item,x:(prior.left-rect.left)/zoom,y:(prior.top-rect.top)/zoom};
        });
        for (const {item,x,y} of shifts) {
          if (Math.abs(x)+Math.abs(y)<1) continue;
          item.animate([{transform:'translate('+x+'px,'+y+'px)'},{transform:'none'}],
            {duration:170,easing:'cubic-bezier(.2,.7,.25,1)'});
        }
      }
    });

    function sync({reveal = false} = {}) {
      const state = getState(), camera = getCamera();
      const host = state.frames.find(frame => frame.id === state.presentationHostId && !frame.hidden);
      root.hidden = !host || canvas.hidden || !!state.presentationId;
      if (root.hidden) {
        closeMenu();
        return;
      }
      const presentation = deck.active(state), slides = deck.slides(state, presentation);
      if (renderedDeck !== presentation.id) {
        chosen.clear();
        anchor = null;
        renderedDeck = presentation.id;
      }
      const geometry = FrameRenderer.propertyGeometry(host, state, camera);
      const placement = FrameRenderer.drawerGeometry(geometry, camera, canvas.clientWidth);
      const width = placement.panelWidth;
      const inlineSpace = canvas.clientWidth <= 490 ? 335 : 420;
      const compact = inlineSpace + 40 * slides.length <= width;
      const columns = Math.min(10, Math.max(1, slides.length), Math.max(1, Math.floor((width-12)/22)));
      const rows = Math.max(1, Math.ceil(slides.length/columns));
      const tileSize = compact ? 34 : Math.max(13, Math.min(50, Math.floor((width-18-6*(columns-1))/columns),
        Math.floor(((canvas.clientHeight-16)/camera.zoom-72)/rows)-9));
      root.style.setProperty('--slide-tile', tileSize + 'px');
      root.style.width = width + 'px';
      root.style.transform = 'scale(' + camera.zoom + ')';
      root.dataset.attached = String(geometry.eligible);
      if (geometry.eligible) {
        const palette = FrameBaseline.tokens.themes[getTheme()];
        const ring = FrameRenderer.deckHighlight(state, host.id, slides.some(frame => frame.id === host.id), palette, true);
        root.style.setProperty('--presentation-rail', ring.stroke);
        root.style.setProperty('--presentation-rail-width', ring.width + 'px');
      }
      add.setAttribute('aria-pressed', String(state.deckMode));
      count.hidden = compact;
      count.textContent = slides.length + ' slides';
      sortButton.disabled = slides.length < 2 || slides.some(frame => frame.locked);
      sortButton.title = slides.some(frame => frame.locked) ? 'Unlock the presentation frames to sort them' : 'Sort frames on canvas';
      inline.hidden = !compact;
      grid.hidden = compact;
      const order = slides.map(frame => frame.id);
      const key = [getRevision(), presentation.id, compact, columns, getTheme(), order.join(',')].join('/');
      if (renderedKey !== key) {
        if (dragging) finishDrag();
        renderedKey = key;
        chosen = new Set([...chosen].filter(id => order.includes(id)));
        const byFrame = members();
        tiles = slides.map((frame, index) => makeTile(frame, index, order, byFrame.get(frame.id) || []));
        updateSelection();
        inline.replaceChildren();
        grid.replaceChildren();
        if (compact) inline.append(...tiles);
        else for (let index = 0; index < tiles.length; index += columns) {
          const row = make('div', 'frame-presentation-row');
          row.append(...tiles.slice(index, index+columns));
          grid.append(row);
        }
      }
      let [left, top] = placement.panel;
      if (reveal) {
        const maxTop = Math.max(8, canvas.clientHeight-root.offsetHeight*camera.zoom-8);
        if (top > maxTop) {
          camera.y -= top-maxTop;
          top = maxTop;
        }
        const projectedWidth = width*camera.zoom;
        if (geometry.eligible && projectedWidth <= canvas.clientWidth-16) {
          const visibleLeft = Math.max(8, Math.min(left,canvas.clientWidth-projectedWidth-8));
          camera.x += visibleLeft-left;
          left = visibleLeft;
        }
      }
      root.style.left = left + 'px';
      root.style.top = top + 'px';
      if (!menu.hidden) positionMenu();
    }
    return {sync, closeMenu, focusSelector:() => selector.focus()};
  }

  window.FramePresentationDrawer = {create};
})();
