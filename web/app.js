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
    const seq = keys[btn.dataset.key];
    if (seq) send(seq);
    term.focus();
  });

  // --- PWA + inbox + push ---

  if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('/sw.js');
  }

  const statusIcons = { working: '⚙', blocked: '⚠', done: '✓', idle: '·', unknown: '?' };
  let agentList = [];

  async function refreshAgents() {
    try {
      const res = await fetch('/api/agents', { headers: { 'x-mushu-token': token } });
      if (!res.ok) return;
      const { host, agents } = await res.json();
      agentList = agents;
      document.getElementById('hostname').textContent = host;
      document.getElementById('chips').innerHTML = agents
        .map((a, i) => `<span class="chip ${a.status}" data-i="${i}">${statusIcons[a.status] || ''} ${a.agent} ${a.title}</span>`)
        .join('');
    } catch (_) {}
  }
  refreshAgents();
  setInterval(() => document.visibilityState === 'visible' && refreshAgents(), 4000);

  // --- agent action sheet ---

  const sheet = document.getElementById('sheet');
  let sheetAgent = null;

  document.getElementById('chips').addEventListener('click', (ev) => {
    const chip = ev.target.closest('.chip');
    if (!chip) return;
    sheetAgent = agentList[Number(chip.dataset.i)];
    if (!sheetAgent) return;
    document.getElementById('sheet-title').textContent =
      `${sheetAgent.agent} · ${sheetAgent.agent_status} · ${sheetAgent.title}`;
    sheet.classList.remove('hidden');
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
