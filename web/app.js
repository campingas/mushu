/* global Terminal, FitAddon */
(() => {
  let token = localStorage.getItem('mushu_token');
  if (!token) {
    token = prompt('mushu access token');
    if (token) localStorage.setItem('mushu_token', token);
  }

  const term = new Terminal({
    fontFamily: 'ui-monospace, Menlo, monospace',
    fontSize: 13,
    theme: { background: '#0d1117' },
    cursorBlink: true,
    scrollback: 3000,
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  term.open(document.getElementById('term'));
  fit.fit();

  const status = document.getElementById('status');
  const encoder = new TextEncoder();
  let ws = null;
  let retryMs = 500;
  let ctrlOn = false;

  function setStatus(text, ok) {
    status.textContent = text;
    status.classList.toggle('ok', !!ok);
    status.classList.remove('hidden');
    if (ok) setTimeout(() => status.classList.add('hidden'), 1500);
  }

  function connect() {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    const url = `${proto}://${location.host}/ws?token=${encodeURIComponent(token)}&cols=${term.cols}&rows=${term.rows}`;
    ws = new WebSocket(url);
    ws.binaryType = 'arraybuffer';

    ws.onopen = () => {
      retryMs = 500;
      setStatus('connected', true);
      sendResize();
    };
    ws.onmessage = (ev) => term.write(new Uint8Array(ev.data));
    ws.onclose = (ev) => {
      if (ev.code === 1008 || ev.code === 4401) {
        localStorage.removeItem('mushu_token');
        setStatus('bad token, reload to retry', false);
        return;
      }
      setStatus('reconnecting…', false);
      setTimeout(connect, retryMs);
      retryMs = Math.min(retryMs * 2, 10000);
    };
    ws.onerror = () => ws.close();
  }

  function send(data) {
    if (ws && ws.readyState === WebSocket.OPEN) ws.send(encoder.encode(data));
  }

  function sendResize() {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ resize: { cols: term.cols, rows: term.rows } }));
    }
  }

  term.onData((data) => {
    if (ctrlOn && data.length === 1) {
      const code = data.toUpperCase().charCodeAt(0);
      if (code >= 64 && code < 96) {
        data = String.fromCharCode(code - 64);
      }
      toggleCtrl(false);
    }
    send(data);
  });

  const resizeObserver = new ResizeObserver(() => {
    fit.fit();
    sendResize();
  });
  resizeObserver.observe(document.getElementById('term'));
  window.visualViewport?.addEventListener('resize', () => {
    fit.fit();
    sendResize();
  });

  const keys = {
    esc: '\x1b',
    tab: '\t',
    up: '\x1b[A',
    down: '\x1b[B',
    left: '\x1b[D',
    right: '\x1b[C',
    'ctrl-c': '\x03',
  };

  function toggleCtrl(on) {
    ctrlOn = on ?? !ctrlOn;
    document.getElementById('ctrl').classList.toggle('on', ctrlOn);
  }

  document.getElementById('toolbar').addEventListener('click', (ev) => {
    const btn = ev.target.closest('button');
    if (!btn) return;
    if (btn.id === 'ctrl') return toggleCtrl();
    if (btn.id === 'kbd') return term.focus();
    if (btn.id === 'mic') return openVoiceBar();
    const seq = keys[btn.dataset.key];
    if (seq) send(seq);
    term.focus();
  });

  // --- voice / compose bar (on-device dictation via the iOS keyboard mic) ---

  const voicebar = document.getElementById('voicebar');
  const voiceInput = document.getElementById('voice-input');
  let voiceTarget = 'terminal'; // 'terminal' or an agent pane_id

  function renderVoiceTargets() {
    const chips = [
      `<span class="vt ${voiceTarget === 'terminal' ? 'selected' : ''}" data-t="terminal">terminal</span>`,
      ...agentList.map(
        (a) =>
          `<span class="vt ${voiceTarget === a.pane_id ? 'selected' : ''}" data-t="${esc(a.pane_id)}">${iconFor(a.agent)}${esc(a.agent)}</span>`
      ),
    ];
    document.getElementById('voice-targets').innerHTML = chips.join('');
    document.getElementById('voice-send-enter').style.display =
      voiceTarget === 'terminal' ? '' : 'none';
  }

  function openVoiceBar() {
    if (!agentList.some((a) => a.pane_id === voiceTarget)) voiceTarget = 'terminal';
    renderVoiceTargets();
    voicebar.classList.remove('hidden');
    voiceInput.focus();
  }

  function closeVoiceBar() {
    voicebar.classList.add('hidden');
    term.focus();
  }

  document.getElementById('voice-targets').addEventListener('click', (ev) => {
    const chip = ev.target.closest('.vt');
    if (!chip) return;
    voiceTarget = chip.dataset.t;
    renderVoiceTargets();
    voiceInput.focus();
  });

  async function voiceSend(withEnter) {
    const text = voiceInput.value;
    if (!text.trim()) return;
    if (voiceTarget === 'terminal') {
      send(withEnter ? text + '\r' : text);
      voiceInput.value = '';
      closeVoiceBar();
      return;
    }
    const agent = agentList.find((a) => a.pane_id === voiceTarget);
    if (!agent) return setStatus('agent gone, pick a target', false);
    const res = await fetch('/api/action', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-mushu-token': token },
      body: JSON.stringify({ pane_id: agent.pane_id, seq: agent.state_change_seq, action: 'prompt', text }),
    });
    if (res.status === 204) {
      voiceInput.value = '';
      setStatus(`sent to ${agent.agent}`, true);
      closeVoiceBar();
    } else if (res.status === 409) {
      setStatus('agent state changed, try again', false);
      refreshAgents();
    } else {
      setStatus(`send failed (${res.status})`, false);
    }
  }

  document.getElementById('voice-send').addEventListener('click', () => voiceSend(false));
  document.getElementById('voice-send-enter').addEventListener('click', () => voiceSend(true));
  document.getElementById('voice-close').addEventListener('click', closeVoiceBar);
  voiceInput.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') {
      ev.preventDefault();
      voiceSend(voiceTarget === 'terminal');
    }
  });

  // Web Speech API bonus: works in Safari tabs but not installed PWAs, so the
  // in-page mic only appears where it can actually function.
  const recBtn = document.getElementById('voice-rec');
  if ('webkitSpeechRecognition' in window && !navigator.standalone) {
    recBtn.classList.remove('hidden');
    let rec = null;
    recBtn.addEventListener('click', () => {
      if (rec) {
        rec.stop();
        return;
      }
      rec = new webkitSpeechRecognition();
      rec.interimResults = true;
      rec.continuous = false;
      const base = voiceInput.value;
      rec.onresult = (ev) => {
        voiceInput.value = base + Array.from(ev.results).map((r) => r[0].transcript).join('');
      };
      rec.onend = () => {
        rec = null;
        recBtn.classList.remove('recording');
      };
      rec.onerror = rec.onend;
      recBtn.classList.add('recording');
      rec.start();
    });
  }

  // --- PWA + inbox + push ---

  if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('/sw.js');
  }

  let agentList = [];
  let workspaceList = [];
  let tabList = [];
  const expandedWs = new Set();

  function hostHue(name) {
    let h = 7;
    for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
    return h;
  }

  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

  // Brand icons (Bootstrap Icons, currentColor) inlined so CSS can tint them.
  const brandOf = { claude: 'claude', codex: 'openai' };
  const icons = {};
  for (const name of ['claude', 'openai']) {
    fetch(`/vendor/${name}.svg`).then((r) => r.text()).then((svg) => { icons[name] = svg; });
  }
  const fallbackIcon = '<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><rect x="1" y="2" width="14" height="12" rx="2" fill="none" stroke="currentColor"/><path d="M4 6l2.5 2L4 10M8 10h4" stroke="currentColor" fill="none" stroke-linecap="round"/></svg>';
  const iconFor = (agent) => icons[brandOf[agent]] || fallbackIcon;

  const prio = { blocked: 3, working: 2, done: 1, idle: 0, unknown: 0 };

  function renderHeader() {
    const brands = new Map();
    for (const a of agentList) {
      const b = brands.get(a.agent) || { count: 0, status: 'idle' };
      b.count += 1;
      if ((prio[a.status] || 0) > (prio[b.status] || 0)) b.status = a.status;
      brands.set(a.agent, b);
    }
    document.getElementById('chips').innerHTML = [...brands]
      .map(
        ([agent, b]) =>
          `<span class="brand ${b.status}" title="${esc(agent)}">${iconFor(agent)}${b.count > 1 ? `<span class="count">${b.count}</span>` : ''}</span>`
      )
      .join('');
  }

  async function refreshAgents() {
    try {
      const res = await fetch('/api/agents', { headers: { 'x-mushu-token': token } });
      if (!res.ok) return;
      const { host, agents, workspaces, tabs } = await res.json();
      agentList = agents;
      workspaceList = workspaces || [];
      tabList = tabs || [];
      document.documentElement.style.setProperty('--host-h', hostHue(host));
      document.getElementById('hostname').textContent = host;
      renderHeader();
      if (!drawer.classList.contains('hidden')) {
        renderWorkspaces();
        renderAgents();
      }
    } catch (_) {}
  }
  refreshAgents();
  setInterval(() => document.visibilityState === 'visible' && refreshAgents(), 4000);

  // --- workspace drawer (swipe down from the header) ---

  const drawer = document.getElementById('drawer');
  const backdrop = document.getElementById('backdrop');

  function renderWorkspaces() {
    document.getElementById('drawer-list').innerHTML = workspaceList
      .map((w) => {
        const expanded = expandedWs.has(w.workspace_id);
        let html =
          `<div class="ws ${w.status} ${w.focused ? 'focused' : ''}" data-id="${esc(w.workspace_id)}" data-tabs="${w.tab_count}">` +
          `<span class="num">${w.number}</span>` +
          `<span class="label">${esc(w.label)}</span>` +
          `<span class="meta">${w.tab_count > 1 ? `${w.tab_count} tabs ${expanded ? '▾' : '▸'}` : '1 tab'}</span>` +
          `<span class="dot"></span></div>`;
        if (expanded) {
          html += tabList
            .filter((t) => t.workspace_id === w.workspace_id)
            .map(
              (t) =>
                `<div class="ws tab ${t.status} ${t.focused ? 'focused' : ''}" data-tab-id="${esc(t.tab_id)}">` +
                `<span class="num">${t.number}</span>` +
                `<span class="label">${esc(t.label)}</span>` +
                `<span class="dot"></span></div>`
            )
            .join('');
        }
        return html;
      })
      .join('');
  }

  function renderAgents() {
    document.getElementById('agent-list').innerHTML = agentList
      .map(
        (a, i) =>
          `<div class="ws agentrow ${a.status}" data-i="${i}">${iconFor(a.agent)}` +
          `<span class="label">${esc(a.agent)}</span>` +
          `<span class="t">${esc(a.title)}</span>` +
          `<button class="act" data-i="${i}">&#8942;</button>` +
          `<span class="dot"></span></div>`
      )
      .join('') || '<div class="ws"><span class="t">no agents detected</span></div>';
  }

  function openDrawer() {
    renderWorkspaces();
    renderAgents();
    backdrop.classList.remove('hidden');
    drawer.classList.remove('hidden');
  }

  function closeDrawer() {
    backdrop.classList.add('hidden');
    drawer.classList.add('hidden');
  }

  let touchY = null;
  document.getElementById('header').addEventListener('touchstart', (ev) => {
    touchY = ev.touches[0].clientY;
  }, { passive: true });
  document.getElementById('header').addEventListener('touchmove', (ev) => {
    if (touchY !== null && ev.touches[0].clientY - touchY > 40) {
      touchY = null;
      openDrawer();
    }
  }, { passive: true });
  drawer.addEventListener('touchstart', (ev) => {
    touchY = ev.touches[0].clientY;
  }, { passive: true });
  drawer.addEventListener('touchmove', (ev) => {
    if (touchY !== null && touchY - ev.touches[0].clientY > 40) {
      touchY = null;
      closeDrawer();
    }
  }, { passive: true });
  document.getElementById('hostname').addEventListener('click', openDrawer);
  backdrop.addEventListener('click', closeDrawer);

  async function focusTarget(action, id) {
    const res = await fetch('/api/action', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-mushu-token': token },
      body: JSON.stringify({ pane_id: '', seq: 0, action, text: id }),
    });
    if (res.status === 204) {
      closeDrawer();
      refreshAgents();
    } else {
      setStatus(`switch failed (${res.status})`, false);
    }
  }

  document.getElementById('drawer-list').addEventListener('click', (ev) => {
    const row = ev.target.closest('.ws');
    if (!row) return;
    if (row.dataset.tabId) return focusTarget('focus-tab', row.dataset.tabId);
    // Multi-tab workspace: first tap expands to its tabs, tap again to collapse.
    if (Number(row.dataset.tabs) > 1) {
      const id = row.dataset.id;
      expandedWs.has(id) ? expandedWs.delete(id) : expandedWs.add(id);
      return renderWorkspaces();
    }
    focusTarget('focus-workspace', row.dataset.id);
  });

  // --- agent action sheet ---

  const sheet = document.getElementById('sheet');
  let sheetAgent = null;

  document.getElementById('chips').addEventListener('click', openDrawer);

  function openSheet(i) {
    sheetAgent = agentList[i];
    if (!sheetAgent) return;
    document.getElementById('sheet-title').textContent =
      `${sheetAgent.agent} · ${sheetAgent.status} · ${sheetAgent.title}`;
    sheet.classList.remove('hidden');
  }

  document.getElementById('agent-list').addEventListener('click', (ev) => {
    const act = ev.target.closest('button.act');
    if (act) {
      closeDrawer();
      return openSheet(Number(act.dataset.i));
    }
    const row = ev.target.closest('.agentrow');
    if (!row) return;
    const a = agentList[Number(row.dataset.i)];
    if (a) focusTarget('focus-agent', a.pane_id);
  });

  document.getElementById('sheet-close').addEventListener('click', () => {
    sheet.classList.add('hidden');
    term.focus();
  });

  async function postAction(action, text) {
    if (!sheetAgent) return;
    const res = await fetch('/api/action', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-mushu-token': token },
      body: JSON.stringify({
        pane_id: sheetAgent.pane_id,
        seq: sheetAgent.state_change_seq,
        action,
        text,
      }),
    });
    if (res.status === 204) {
      setStatus('sent', true);
      sheet.classList.add('hidden');
    } else if (res.status === 409) {
      setStatus('agent state changed, try again', false);
    } else {
      setStatus(`action failed (${res.status})`, false);
    }
    refreshAgents();
  }

  sheet.addEventListener('click', (ev) => {
    const btn = ev.target.closest('button[data-act]');
    if (!btn) return;
    if (btn.dataset.act === 'prompt') {
      const text = document.getElementById('sheet-prompt').value;
      if (text.trim()) {
        postAction('prompt', text);
        document.getElementById('sheet-prompt').value = '';
      }
    } else {
      postAction('keys', btn.dataset.text);
    }
  });

  function b64ToBytes(b64) {
    const pad = '='.repeat((4 - (b64.length % 4)) % 4);
    const raw = atob((b64 + pad).replace(/-/g, '+').replace(/_/g, '/'));
    return Uint8Array.from(raw, (c) => c.charCodeAt(0));
  }

  const bell = document.getElementById('bell');

  async function syncBell() {
    const reg = await navigator.serviceWorker?.ready;
    const sub = await reg?.pushManager?.getSubscription();
    bell.classList.toggle('on', !!sub);
  }
  syncBell();

  bell.addEventListener('click', async () => {
    if (!('Notification' in window) || !navigator.serviceWorker) {
      return setStatus('push unsupported here; add to home screen first', false);
    }
    const perm = await Notification.requestPermission();
    if (perm !== 'granted') return setStatus('notifications denied', false);
    const reg = await navigator.serviceWorker.ready;
    const { key } = await (await fetch('/push/vapid')).json();
    const sub = await reg.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: b64ToBytes(key),
    });
    const json = sub.toJSON();
    await fetch('/push/subscribe', {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-mushu-token': token },
      body: JSON.stringify({ endpoint: json.endpoint, p256dh: json.keys.p256dh, auth: json.keys.auth }),
    });
    bell.classList.add('on');
    setStatus('notifications enabled', true);
  });

  connect();
  term.focus();
})();
