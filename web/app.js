/* global Terminal, FitAddon */
(async () => {
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

  const makePalette = (colors) => {
    const p = {
      surfaceDim: colors.panel,
      overlay0: colors.surface1,
      overlay1: colors.muted,
      cursor: colors.fg,
      selection: colors.accent + '66',
      ...colors,
    };
    p.ansi = [p.bg, p.red, p.green, p.yellow, p.blue, p.mauve, p.teal, p.fg];
    p.bright = [p.muted, p.red, p.green, p.yellow, p.blue, p.mauve, p.teal, '#ffffff'];
    return p;
  };

  // These palettes adapt Herdr's built-in identities to Mushu's phone UI.
  // They are intentionally coherent approximations, not copied desktop values.
  const palettes = {
    catppuccin: makePalette({ bg: '#181825', fg: '#cdd6f4', panel: '#1e1e2e', surface0: '#313244', surface1: '#45475a', muted: '#a6adc8', accent: '#89b4fa', red: '#f38ba8', green: '#a6e3a1', yellow: '#f9e2af', blue: '#89b4fa', mauve: '#cba6f7', teal: '#94e2d5', peach: '#fab387' }),
    'catppuccin-latte': makePalette({ bg: '#eff1f5', fg: '#4c4f69', panel: '#e6e9ef', surface0: '#dce0e8', surface1: '#ccd0da', muted: '#6c6f85', accent: '#1e66f5', red: '#d20f39', green: '#358a2f', yellow: '#8a6100', blue: '#1e66f5', mauve: '#8839ef', teal: '#087d8b', peach: '#b84d00' }),
    'tokyo-night': makePalette({ bg: '#16161e', fg: '#c0caf5', panel: '#1a1b26', surface0: '#24283b', surface1: '#414868', muted: '#9aa5ce', accent: '#7aa2f7', red: '#f7768e', green: '#9ece6a', yellow: '#e0af68', blue: '#7aa2f7', mauve: '#bb9af7', teal: '#7dcfff', peach: '#ff9e64' }),
    'tokyo-night-day': makePalette({ bg: '#e6e7ed', fg: '#343b58', panel: '#dcdde4', surface0: '#d1d3dc', surface1: '#b7bac7', muted: '#5f6785', accent: '#34548a', red: '#8c4351', green: '#33635c', yellow: '#8f5e15', blue: '#34548a', mauve: '#5a4a78', teal: '#0f6d7a', peach: '#965027' }),
    dracula: makePalette({ bg: '#191a21', fg: '#f8f8f2', panel: '#282a36', surface0: '#343746', surface1: '#44475a', muted: '#b7b7c5', accent: '#bd93f9', red: '#ff5555', green: '#50fa7b', yellow: '#f1fa8c', blue: '#8be9fd', mauve: '#ff79c6', teal: '#8be9fd', peach: '#ffb86c' }),
    nord: makePalette({ bg: '#242933', fg: '#d8dee9', panel: '#2e3440', surface0: '#3b4252', surface1: '#4c566a', muted: '#aeb8c8', accent: '#88c0d0', red: '#bf616a', green: '#8fbc8f', yellow: '#d6b978', blue: '#81a1c1', mauve: '#b48ead', teal: '#8fbcbb', peach: '#d08770' }),
    gruvbox: makePalette({ bg: '#1d2021', fg: '#ebdbb2', panel: '#282828', surface0: '#3c3836', surface1: '#504945', muted: '#bdae93', accent: '#d79921', red: '#fb4934', green: '#b8bb26', yellow: '#fabd2f', blue: '#83a598', mauve: '#d3869b', teal: '#8ec07c', peach: '#fe8019' }),
    'gruvbox-light': makePalette({ bg: '#fbf1c7', fg: '#3c3836', panel: '#f2e5bc', surface0: '#ebdbb2', surface1: '#d5c4a1', muted: '#665c54', accent: '#b57614', red: '#9d0006', green: '#79740e', yellow: '#8f6500', blue: '#076678', mauve: '#8f3f71', teal: '#427b58', peach: '#af3a03' }),
    'one-dark': makePalette({ bg: '#21252b', fg: '#abb2bf', panel: '#282c34', surface0: '#353b45', surface1: '#4b5263', muted: '#8b93a3', accent: '#61afef', red: '#e06c75', green: '#98c379', yellow: '#e5c07b', blue: '#61afef', mauve: '#c678dd', teal: '#56b6c2', peach: '#d19a66' }),
    'one-light': makePalette({ bg: '#fafafa', fg: '#383a42', panel: '#f0f0f1', surface0: '#e5e5e6', surface1: '#c8c8ca', muted: '#5c6370', accent: '#4078f2', red: '#a62626', green: '#397c30', yellow: '#986801', blue: '#4078f2', mauve: '#8f36a5', teal: '#0184bc', peach: '#b85c00' }),
    solarized: makePalette({ bg: '#002b36', fg: '#eee8d5', panel: '#073642', surface0: '#164956', surface1: '#586e75', muted: '#93a1a1', accent: '#268bd2', red: '#dc322f', green: '#859900', yellow: '#b58900', blue: '#268bd2', mauve: '#d33682', teal: '#2aa198', peach: '#cb4b16' }),
    'solarized-light': makePalette({ bg: '#fdf6e3', fg: '#073642', panel: '#eee8d5', surface0: '#e3dcc9', surface1: '#c8c1af', muted: '#586e75', accent: '#1679b5', red: '#c52d2a', green: '#687a00', yellow: '#8b6900', blue: '#1679b5', mauve: '#b42d70', teal: '#1b7f78', peach: '#a83d11' }),
    kanagawa: makePalette({ bg: '#16161d', fg: '#dcd7ba', panel: '#1f1f28', surface0: '#2a2a37', surface1: '#54546d', muted: '#a6a69c', accent: '#7e9cd8', red: '#e46876', green: '#98bb6c', yellow: '#e6c384', blue: '#7e9cd8', mauve: '#957fb8', teal: '#7fb4ca', peach: '#ffa066' }),
    'kanagawa-lotus': makePalette({ bg: '#f2ecde', fg: '#43436c', panel: '#e7dfcf', surface0: '#ddd4c5', surface1: '#b8afa2', muted: '#625e78', accent: '#4d699b', red: '#c84053', green: '#597b35', yellow: '#836f2e', blue: '#4d699b', mauve: '#624c83', teal: '#4d7d85', peach: '#a45a1c' }),
    'rose-pine': makePalette({ bg: '#191724', fg: '#e0def4', panel: '#1f1d2e', surface0: '#26233a', surface1: '#403d52', muted: '#aaa5c4', accent: '#c4a7e7', red: '#eb6f92', green: '#9ccf8b', yellow: '#f6c177', blue: '#31748f', mauve: '#c4a7e7', teal: '#9ccfd8', peach: '#f6c177' }),
    'rose-pine-dawn': makePalette({ bg: '#faf4ed', fg: '#423c52', panel: '#f2e9e1', surface0: '#ebe1d9', surface1: '#cecacd', muted: '#6e687b', accent: '#907aa9', red: '#b4637a', green: '#557c64', yellow: '#8f6534', blue: '#286983', mauve: '#907aa9', teal: '#568a93', peach: '#a96735' }),
    vesper: makePalette({ bg: '#101010', fg: '#e0e0e0', panel: '#181818', surface0: '#232323', surface1: '#3a3a3a', muted: '#a0a0a0', accent: '#ffc799', red: '#ff8080', green: '#99ffe4', yellow: '#ffc799', blue: '#a0c8ff', mauve: '#e6b3ff', teal: '#99ffe4', peach: '#ffb38a' }),
  };

  const phoneDark = makePalette({ bg: '#0d1117', fg: '#c9d1d9', panel: '#161b22', surface0: '#21262d', surface1: '#30363d', surfaceDim: '#1a1f26', overlay0: '#30363d', overlay1: '#484f58', muted: '#8b949e', accent: '#1f6feb', red: '#f85149', green: '#3fb950', yellow: '#d29922', blue: '#58a6ff', mauve: '#bc8cff', teal: '#39c5cf', peach: '#f0883e', selection: '#264f78' });
  const phoneLight = makePalette({ bg: '#f6f8fa', fg: '#24292f', panel: '#ffffff', surface0: '#eaeef2', surface1: '#d0d7de', muted: '#57606a', accent: '#0969da', red: '#cf222e', green: '#1a7f37', yellow: '#7d4e00', blue: '#0969da', mauve: '#8250df', teal: '#1b7c83', peach: '#bc4c00' });
  const prefersLight = () => matchMedia('(prefers-color-scheme: light)').matches;

  function luminance(hex) {
    const channels = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
      .map((v) => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
    return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  }

  function contrast(a, b) {
    const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (light + 0.05) / (dark + 0.05);
  }

  const meetsContrast = (color, backgrounds, ratio) =>
    backgrounds.every((background) => contrast(color, background) >= ratio);

  const contrastText = (background) =>
    contrast('#ffffff', background) >= contrast('#000000', background) ? '#ffffff' : '#000000';

  function resolvePalette(theme) {
    if (!theme) return phoneDark;
    let name = theme.name;
    if (theme.auto_switch) {
      name = prefersLight()
        ? (theme.light_name || 'catppuccin-latte')
        : (theme.dark_name || 'catppuccin');
    }
    let palette = name === 'terminal'
      ? (prefersLight() ? phoneLight : phoneDark)
      : (palettes[name] || palettes.catppuccin);
    palette = { ...palette, ansi: [...palette.ansi], bright: [...palette.bright] };
    const custom = theme.custom || {};
    const direct = { accent: 'accent', overlay0: 'overlay0', overlay1: 'overlay1', mauve: 'mauve', blue: 'blue', teal: 'teal', peach: 'peach' };
    for (const [source, target] of Object.entries(direct)) {
      if (custom[source]) palette[target] = custom[source];
    }
    const backgroundOverrides = {
      panel_bg: ['panel', custom.panel_bg === 'reset' ? palette.bg : custom.panel_bg],
      surface0: ['surface0', custom.surface0],
      surface1: ['surface1', custom.surface1],
      surface_dim: ['surfaceDim', custom.surface_dim],
    };
    const requestedBackgrounds = [
      palette.bg,
      backgroundOverrides.panel_bg[1] || palette.panel,
      backgroundOverrides.surface0[1] || palette.surface0,
      backgroundOverrides.surface1[1] || palette.surface1,
      backgroundOverrides.surface_dim[1] || palette.surfaceDim,
    ];
    const requestedText = custom.text
      && meetsContrast(custom.text, requestedBackgrounds, 4.5)
      ? custom.text
      : palette.fg;
    for (const [, [target, color]] of Object.entries(backgroundOverrides)) {
      if (color && contrast(requestedText, color) >= 4.5) palette[target] = color;
    }
    palette.fg = requestedText;
    const contentBackgrounds = [palette.bg, palette.panel, palette.surface0, palette.surface1, palette.surfaceDim];
    if (custom.subtext0 && meetsContrast(custom.subtext0, contentBackgrounds, 4.5)) palette.muted = custom.subtext0;
    for (const color of ['red', 'green', 'yellow']) {
      if (custom[color] && meetsContrast(custom[color], contentBackgrounds, 3)) palette[color] = custom[color];
    }
    palette.cursor = palette.fg;
    palette.onAccent = contrastText(palette.accent);
    palette.selection = palette.accent + '66';
    palette.ansi = [palette.bg, palette.red, palette.green, palette.yellow, palette.blue, palette.mauve, palette.teal, palette.fg];
    palette.bright = [palette.muted, palette.red, palette.green, palette.yellow, palette.blue, palette.mauve, palette.teal, luminance(palette.bg) > 0.5 ? '#24292f' : '#ffffff'];
    return palette;
  }

  function applyPalette(palette) {
    const css = { bg: 'bg', fg: 'fg', panel: 'panel', surface0: 'surface-0', surface1: 'surface-1', surfaceDim: 'surface-dim', overlay0: 'overlay-0', overlay1: 'overlay-1', muted: 'muted', accent: 'accent', red: 'red', green: 'green', yellow: 'yellow', blue: 'blue', mauve: 'mauve', teal: 'teal', peach: 'peach', cursor: 'cursor', selection: 'selection', onAccent: 'on-accent' };
    for (const [key, variable] of Object.entries(css)) {
      document.documentElement.style.setProperty(`--${variable}`, palette[key]);
    }
    document.documentElement.style.setProperty('--yellow-soft', palette.yellow + '55');
    document.documentElement.style.setProperty('--red-glow', palette.red + 'aa');
    term.options.theme = {
      background: palette.bg, foreground: palette.fg, cursor: palette.cursor,
      selectionBackground: palette.selection,
      black: palette.ansi[0], red: palette.ansi[1], green: palette.ansi[2], yellow: palette.ansi[3],
      blue: palette.ansi[4], magenta: palette.ansi[5], cyan: palette.ansi[6], white: palette.ansi[7],
      brightBlack: palette.bright[0], brightRed: palette.bright[1], brightGreen: palette.bright[2], brightYellow: palette.bright[3],
      brightBlue: palette.bright[4], brightMagenta: palette.bright[5], brightCyan: palette.bright[6], brightWhite: palette.bright[7],
    };
    document.querySelector('meta[name="theme-color"]').content = palette.bg;
  }

  applyPalette(phoneDark);

  function setStatus(text, ok) {
    status.textContent = text;
    status.classList.toggle('ok', !!ok);
    status.classList.remove('hidden');
    if (ok) setTimeout(() => status.classList.add('hidden'), 1500);
  }

  // --- instance registry + optional Face ID vault ---
  // The PWA talks to every saved instance from this one origin (cross-origin
  // ws + CORS API), so tokens for all hosts live here. With the vault enabled
  // they are AES-GCM encrypted under a passkey PRF secret that the Secure
  // Enclave only releases after Face ID / Touch ID.

  const b64e = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf)));
  const b64d = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

  let token = null;
  let instances = null;
  let vaultKey = null;
  let vaultCredId = null;

  function loadInstances() {
    let list = [];
    try {
      list = JSON.parse(localStorage.getItem('mushu_instances')) || [];
    } catch (_) {}
    const home = list.find((i) => i.url === location.origin);
    if (!home) {
      list.unshift({ url: location.origin, token });
    } else if (token && home.token !== token) {
      home.token = token;
    }
    localStorage.setItem('mushu_instances', JSON.stringify(list));
    return list;
  }

  async function prfKey(credId) {
    const cred = await navigator.credentials.get({
      publicKey: {
        challenge: crypto.getRandomValues(new Uint8Array(32)),
        allowCredentials: [{ type: 'public-key', id: b64d(credId) }],
        userVerification: 'required',
        extensions: { prf: { eval: { first: new TextEncoder().encode('mushu-vault-v1') } } },
      },
    });
    const secret = cred.getClientExtensionResults().prf?.results?.first;
    if (!secret) throw new Error('passkey returned no PRF secret');
    return crypto.subtle.importKey('raw', secret, 'AES-GCM', false, ['encrypt', 'decrypt']);
  }

  async function persistVault() {
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const data = await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv },
      vaultKey,
      new TextEncoder().encode(JSON.stringify(instances))
    );
    localStorage.setItem('mushu_vault', JSON.stringify({ credId: vaultCredId, iv: b64e(iv), data: b64e(data) }));
    localStorage.removeItem('mushu_instances');
    localStorage.removeItem('mushu_token');
  }

  async function unlockVault(vault) {
    const lock = document.getElementById('lock');
    const btn = document.getElementById('lock-unlock');
    lock.classList.remove('hidden');
    for (;;) {
      await new Promise((res) => btn.addEventListener('click', res, { once: true }));
      try {
        const key = await prfKey(vault.credId);
        const plain = await crypto.subtle.decrypt(
          { name: 'AES-GCM', iv: b64d(vault.iv) },
          key,
          b64d(vault.data)
        );
        vaultKey = key;
        vaultCredId = vault.credId;
        lock.classList.add('hidden');
        return JSON.parse(new TextDecoder().decode(plain));
      } catch (_) {
        setStatus('unlock failed, try again', false);
      }
    }
  }

  // `mushuctl pair` encodes the token in the QR's URL fragment. Fragments are
  // never sent to the server, and keeping it in the URL while running in Safari
  // means "Add to Home Screen" captures it, so the installed app is signed in
  // even though iOS gives it a storage jar separate from Safari's.
  const pairedToken = (() => {
    const raw = decodeURIComponent(location.hash.replace(/^#/, '')).trim();
    return raw.length >= 16 ? raw : null;
  })();

  function consumePairFragment() {
    if (pairedToken && window.navigator.standalone) {
      history.replaceState(null, '', location.pathname + location.search);
    }
  }

  let vault = null;
  try {
    vault = JSON.parse(localStorage.getItem('mushu_vault'));
  } catch (_) {}
  if (vault) {
    instances = await unlockVault(vault);
  } else {
    token = pairedToken || localStorage.getItem('mushu_token');
    if (!token) {
      token = prompt('mushu access token');
    }
    if (token) localStorage.setItem('mushu_token', token);
    instances = loadInstances();
  }
  consumePairFragment();

  const saveInstances = () => {
    if (vaultKey) return void persistVault().catch(() => setStatus('vault write failed', false));
    localStorage.setItem('mushu_instances', JSON.stringify(instances));
  };
  const shortHost = (url) => new URL(url).hostname.split('.')[0];

  // Re-pairing this host while the vault holds the tokens.
  if (vaultKey && pairedToken) {
    const home = instances.find((i) => i.url === location.origin);
    if (home && home.token !== pairedToken) {
      home.token = pairedToken;
      saveInstances();
    }
  }

  let active =
    instances.find((i) => i.url === localStorage.getItem('mushu_active')) ||
    instances.find((i) => i.url === location.origin) ||
    instances[0];

  const api = (path, opts = {}) =>
    fetch(active.url + path, {
      ...opts,
      headers: { ...(opts.headers || {}), 'x-mushu-token': active.token || '' },
    });

  document.documentElement.style.setProperty('--host-h', hostHue(shortHost(active.url)));
  document.getElementById('hostname').textContent = shortHost(active.url);

  let connectEpoch = 0;
  let connectInFlightEpoch = null;
  let preflightController = null;
  let reconnectTimer = null;

  async function connect() {
    const epoch = connectEpoch;
    if (connectInFlightEpoch === epoch) return;
    connectInFlightEpoch = epoch;
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
    preflightController?.abort();
    const controller = new AbortController();
    preflightController = controller;
    applyPalette(phoneDark);
    const timeout = setTimeout(() => controller.abort(), 2000);
    const instance = active;
    try {
      const res = await fetch(instance.url + '/api/host', {
        headers: { 'x-mushu-token': instance.token || '' },
        signal: controller.signal,
      });
      if (epoch === connectEpoch && instance === active && res.ok) {
        const descriptor = await res.json();
        if (epoch !== connectEpoch || instance !== active) return;
        applyPalette(resolvePalette(descriptor.theme));
        if (descriptor.host) {
          document.documentElement.style.setProperty('--host-h', hostHue(descriptor.host));
          document.getElementById('hostname').textContent = descriptor.host;
        }
      }
    } catch (_) {
      // The fallback palette is already active; theme discovery never blocks a terminal.
    } finally {
      clearTimeout(timeout);
      if (preflightController === controller) preflightController = null;
      if (connectInFlightEpoch === epoch) connectInFlightEpoch = null;
    }
    if (epoch !== connectEpoch || instance !== active) return;
    const base = new URL(active.url);
    const proto = base.protocol === 'https:' ? 'wss' : 'ws';
    const url = `${proto}://${base.host}/ws?token=${encodeURIComponent(active.token || '')}&cols=${term.cols}&rows=${term.rows}`;
    ws = new WebSocket(url);
    ws.binaryType = 'arraybuffer';

    ws.onopen = () => {
      if (epoch !== connectEpoch) return;
      retryMs = 500;
      setStatus(`connected to ${shortHost(active.url)}`, true);
      sendResize();
    };
    ws.onmessage = (ev) => {
      if (epoch === connectEpoch) term.write(new Uint8Array(ev.data));
    };
    ws.onclose = (ev) => {
      if (epoch !== connectEpoch) return;
      if (ev.code === 1008 || ev.code === 4401) {
        if (active.url === location.origin && !vaultKey) {
          localStorage.removeItem('mushu_token');
          setStatus('bad token, reload to retry', false);
        } else {
          setStatus(`bad token for ${shortHost(active.url)}, fix it in settings`, false);
        }
        return;
      }
      setStatus('reconnecting…', false);
      reconnectTimer = setTimeout(() => {
        if (epoch === connectEpoch) connect();
      }, retryMs);
      retryMs = Math.min(retryMs * 2, 10000);
    };
    ws.onerror = (ev) => ev.target.close();
  }

  function setActive(url) {
    const inst = instances.find((i) => i.url === url);
    if (!inst || inst === active) return;
    active = inst;
    localStorage.setItem('mushu_active', url);
    connectEpoch += 1;
    preflightController?.abort();
    connectInFlightEpoch = null;
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
    try {
      ws?.close();
    } catch (_) {}
    term.reset();
    retryMs = 500;
    document.documentElement.style.setProperty('--host-h', hostHue(shortHost(url)));
    document.getElementById('hostname').textContent = shortHost(url);
    connect();
    refreshAgents();
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

  let lastVisualViewportHeight = window.visualViewport?.height || window.innerHeight;

  function syncViewport(followTerminalOnShrink = false) {
    const viewport = window.visualViewport;
    const height = viewport?.height || window.innerHeight;
    const offsetTop = viewport?.offsetTop || 0;
    // A shrinking visual viewport with xterm focused is the keyboard sliding in.
    const followTerminal =
      followTerminalOnShrink &&
      height < lastVisualViewportHeight &&
      document.activeElement === term.textarea;
    lastVisualViewportHeight = height;
    document.documentElement.style.setProperty('--viewport-height', height + 'px');
    document.documentElement.style.setProperty('--viewport-top', offsetTop + 'px');
    fit.fit();
    if (followTerminal) term.scrollToBottom();
    sendResize();
  }
  syncViewport();
  window.visualViewport?.addEventListener('resize', () => syncViewport(true));
  window.visualViewport?.addEventListener('scroll', () => syncViewport(true));
  if (!window.visualViewport) window.addEventListener('resize', () => syncViewport());

  // iOS standalone loses env(safe-area-inset-*) after the in-app browser
  // overlay used for host switches, collapsing the header under the status
  // bar. Read the probe while the values are valid and pin them as CSS vars;
  // ignore zero readings so a buggy collapse can never shrink the layout.
  function pinSafeArea() {
    const probe = getComputedStyle(document.getElementById('safe-probe'));
    const top = parseFloat(probe.paddingTop) || 0;
    const bottom = parseFloat(probe.paddingBottom) || 0;
    if (top > 0) document.documentElement.style.setProperty('--safe-top', top + 'px');
    if (bottom > 0) document.documentElement.style.setProperty('--safe-bottom', bottom + 'px');
  }
  pinSafeArea();

  // Returning from another app can leave the page scrolled or with collapsed
  // insets; re-pin and snap back whenever we regain the screen.
  function restoreViewport() {
    pinSafeArea();
    window.scrollTo(0, 0);
    syncViewport();
  }
  window.addEventListener('pageshow', restoreViewport);
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') restoreViewport();
  });

  const keys = {
    esc: '\x1b',
    tab: '\t',
    'ctrl-c': '\x03',
  };

  function toggleCtrl(on) {
    ctrlOn = on ?? !ctrlOn;
    document.getElementById('ctrl').classList.toggle('on', ctrlOn);
  }

  const terminalElement = document.getElementById('term');
  let terminalTap = null;
  let suppressTerminalMouseDown = false;

  terminalElement.addEventListener('mousedown', (ev) => {
    if (!suppressTerminalMouseDown) return;
    suppressTerminalMouseDown = false;
    ev.preventDefault();
    ev.stopPropagation();
  }, true);

  terminalElement.addEventListener('pointerdown', (ev) => {
    suppressTerminalMouseDown =
      ev.pointerType === 'touch' && document.activeElement === term.textarea;
    terminalTap = {
      id: ev.pointerId,
      pointerType: ev.pointerType,
      x: ev.clientX,
      y: ev.clientY,
      lastY: ev.clientY,
      wasFocused: document.activeElement === term.textarea,
      moved: false,
    };
  });
  terminalElement.addEventListener('pointermove', (ev) => {
    if (!terminalTap || terminalTap.id !== ev.pointerId) return;
    if (Math.hypot(ev.clientX - terminalTap.x, ev.clientY - terminalTap.y) > 8) {
      terminalTap.moved = true;
    }
    const deltaY = terminalTap.lastY - ev.clientY;
    terminalTap.lastY = ev.clientY;
    if (
      terminalTap.moved &&
      terminalTap.pointerType === 'touch' &&
      term.modes.mouseTrackingMode !== 'none' &&
      deltaY !== 0
    ) {
      ev.preventDefault();
      term.element.dispatchEvent(new WheelEvent('wheel', {
        deltaY,
        deltaMode: WheelEvent.DOM_DELTA_PIXEL,
        clientX: ev.clientX,
        clientY: ev.clientY,
        bubbles: true,
        cancelable: true,
      }));
    }
  });
  terminalElement.addEventListener('pointerup', (ev) => {
    if (!terminalTap || terminalTap.id !== ev.pointerId) return;
    const tap = terminalTap;
    terminalTap = null;
    if (tap.moved) return;
    if (tap.wasFocused) {
      term.blur();
    } else {
      term.focus();
    }
  });
  terminalElement.addEventListener('pointercancel', () => {
    terminalTap = null;
  });

  const toolbar = document.getElementById('toolbar');
  let toolbarKeyboardWasOpen = false;
  toolbar.addEventListener('pointerdown', () => {
    toolbarKeyboardWasOpen = document.activeElement === term.textarea;
  });
  toolbar.addEventListener('click', (ev) => {
    const btn = ev.target.closest('button');
    if (!btn || btn.disabled) return;
    if (btn.id === 'mic') return openVoiceBar();
    if (btn.id === 'ctrl') {
      toggleCtrl();
    } else {
      const seq = keys[btn.dataset.key];
      if (seq) send(seq);
    }
    toolbarKeyboardWasOpen ? term.focus() : term.blur();
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
      term.paste(text);
      if (withEnter) send('\r');
      voiceInput.value = '';
      closeVoiceBar();
      return;
    }
    const agent = agentList.find((a) => a.pane_id === voiceTarget);
    if (!agent) return setStatus('agent gone, pick a target', false);
    const res = await api('/api/action', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
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

  document.getElementById('voice-paste').addEventListener('click', async () => {
    if (!navigator.clipboard?.readText) {
      setStatus('clipboard unavailable; press and hold in the field to paste', false);
      voiceInput.focus();
      return;
    }
    try {
      const text = await navigator.clipboard.readText();
      const start = voiceInput.selectionStart;
      const end = voiceInput.selectionEnd;
      voiceInput.setRangeText(text, start, end, 'end');
      voiceInput.focus();
    } catch (_) {
      setStatus('clipboard access failed; press and hold in the field to paste', false);
      voiceInput.focus();
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
    const forInstance = active;
    try {
      const res = await api('/api/agents');
      if (!res.ok || forInstance !== active) return;
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

  function renderHosts() {
    document.getElementById('host-list').innerHTML = instances
      .map(
        (inst) =>
          `<div class="ws host ${inst === active ? 'focused' : ''}" data-url="${esc(inst.url)}">` +
          `<span class="label">${esc(shortHost(inst.url))}</span>` +
          `<span class="t">${esc(new URL(inst.url).host)}</span></div>`
      )
      .join('');
  }

  function openDrawer() {
    renderHosts();
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
  document.getElementById('host-list').addEventListener('click', (ev) => {
    const row = ev.target.closest('.ws');
    if (!row) return;
    setActive(row.dataset.url);
    closeDrawer();
  });
  backdrop.addEventListener('click', closeDrawer);

  async function focusTarget(action, id) {
    const res = await api('/api/action', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
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
  });

  async function postAction(action, text) {
    if (!sheetAgent) return;
    const res = await api('/api/action', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
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

  // --- instances (favorite mushu URLs, per-instance alerts) ---
  // One local push subscription (this origin's VAPID key) serves every
  // instance: hosts share the VAPID keypair, so alert on/off per instance is
  // just this endpoint's presence in that instance's server-side sub store.

  const settings = document.getElementById('settings');
  const instHeaders = (inst) => ({ 'content-type': 'application/json', 'x-mushu-token': inst.token });

  async function localSubscription() {
    const reg = await navigator.serviceWorker?.ready;
    return (await reg?.pushManager?.getSubscription()) || null;
  }

  async function ensureSubscription() {
    if (!('Notification' in window) || !navigator.serviceWorker) {
      throw new Error('push unsupported here; add to home screen first');
    }
    if ((await Notification.requestPermission()) !== 'granted') {
      throw new Error('notifications denied');
    }
    const reg = await navigator.serviceWorker.ready;
    let sub = await reg.pushManager.getSubscription();
    if (!sub) {
      const { key } = await (await fetch('/push/vapid')).json();
      sub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: b64ToBytes(key),
      });
    }
    return sub;
  }

  async function renderInstances() {
    document.getElementById('instance-list').innerHTML = instances
      .map((inst, i) => {
        const home = inst.url === location.origin;
        return (
          `<div class="ws inst ${inst === active ? 'focused' : ''}">` +
          `<span class="label">${esc(shortHost(inst.url))}</span>` +
          `<span class="t">${esc(new URL(inst.url).host)}</span>` +
          `<button class="alert" data-alert="${i}">&#8230;</button>` +
          (home ? '' : `<button class="rm" data-rm="${i}">&#10005;</button>`) +
          `</div>`
        );
      })
      .join('');
    const sub = await localSubscription();
    await Promise.all(
      instances.map(async (inst, i) => {
        const btn = document.querySelector(`[data-alert="${i}"]`);
        if (!btn) return;
        if (!sub) return setAlertBtn(btn, false);
        try {
          const res = await fetch(inst.url + '/push/status', {
            method: 'POST',
            headers: instHeaders(inst),
            body: JSON.stringify({ endpoint: sub.endpoint }),
          });
          if (!res.ok) return void (btn.textContent = res.status === 401 ? 'bad token' : `err ${res.status}`);
          setAlertBtn(btn, (await res.json()).subscribed);
        } catch (_) {
          btn.textContent = 'offline';
        }
      })
    );
  }

  function setAlertBtn(btn, on) {
    btn.textContent = on ? 'alerts on' : 'alerts off';
    btn.classList.toggle('on', on);
  }

  async function toggleAlerts(i, on) {
    const inst = instances[i];
    try {
      const sub = await ensureSubscription();
      const json = sub.toJSON();
      const body = on
        ? { endpoint: json.endpoint, p256dh: json.keys.p256dh, auth: json.keys.auth }
        : { endpoint: json.endpoint };
      const res = await fetch(inst.url + (on ? '/push/subscribe' : '/push/unsubscribe'), {
        method: 'POST',
        headers: instHeaders(inst),
        body: JSON.stringify(body),
      });
      if (res.status === 204 || res.status === 404) {
        setStatus(`${shortHost(inst.url)} alerts ${on ? 'on' : 'off'}`, true);
      } else {
        setStatus(`toggle failed (${res.status})`, false);
      }
    } catch (e) {
      setStatus(e.message || 'toggle failed', false);
    }
    renderInstances();
  }

  async function addInstance() {
    const urlInput = document.getElementById('inst-url');
    const tokenInput = document.getElementById('inst-token');
    let url = urlInput.value.trim();
    const instToken = tokenInput.value.trim();
    if (!url) return;
    if (!/^https?:/.test(url)) url = 'https://' + url;
    try {
      url = new URL(url).origin;
    } catch (_) {
      return setStatus('invalid url', false);
    }
    if (instances.some((i) => i.url === url)) return setStatus('already saved', false);
    try {
      const res = await fetch(url + '/api/agents', { headers: { 'x-mushu-token': instToken } });
      if (res.status === 401) return setStatus('bad token for that instance', false);
      if (!res.ok) return setStatus(`instance error (${res.status})`, false);
      const { key } = await (await fetch(url + '/push/vapid')).json();
      const { key: localKey } = await (await fetch('/push/vapid')).json();
      if (key !== localKey) {
        setStatus('added, but VAPID keys differ: its alerts cannot reach this app', false);
      } else {
        setStatus(`${shortHost(url)} added`, true);
      }
    } catch (_) {
      return setStatus('instance unreachable', false);
    }
    instances.push({ url, token: instToken });
    saveInstances();
    urlInput.value = '';
    tokenInput.value = '';
    renderInstances();
  }

  document.getElementById('settings-btn').addEventListener('click', () => {
    settings.classList.remove('hidden');
    renderInstances();
    syncLockToggle();
  });
  document.getElementById('settings-close').addEventListener('click', () => {
    settings.classList.add('hidden');
  });
  document.getElementById('inst-add').addEventListener('click', addInstance);
  document.getElementById('instance-list').addEventListener('click', (ev) => {
    const alertBtn = ev.target.closest('button.alert');
    if (alertBtn) {
      const i = Number(alertBtn.dataset.alert);
      return toggleAlerts(i, !alertBtn.classList.contains('on'));
    }
    const rmBtn = ev.target.closest('button.rm');
    if (rmBtn) {
      const [removed] = instances.splice(Number(rmBtn.dataset.rm), 1);
      if (removed === active) setActive(location.origin);
      saveInstances();
      renderInstances();
    }
  });

  // --- Face ID vault toggle ---

  const lockToggle = document.getElementById('lock-toggle');

  async function syncLockToggle() {
    const supported =
      !!window.PublicKeyCredential &&
      (await PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable().catch(() => false));
    lockToggle.classList.toggle('hidden', !supported && !vaultKey);
    lockToggle.textContent = vaultKey ? 'disable face id lock' : 'enable face id lock';
  }

  async function enableLock() {
    const cred = await navigator.credentials.create({
      publicKey: {
        challenge: crypto.getRandomValues(new Uint8Array(32)),
        rp: { name: 'mushu' },
        user: { id: crypto.getRandomValues(new Uint8Array(16)), name: 'mushu', displayName: 'mushu' },
        pubKeyCredParams: [
          { type: 'public-key', alg: -7 },
          { type: 'public-key', alg: -257 },
        ],
        authenticatorSelection: {
          authenticatorAttachment: 'platform',
          residentKey: 'required',
          userVerification: 'required',
        },
        extensions: { prf: {} },
      },
    });
    if (!cred.getClientExtensionResults().prf?.enabled) {
      throw new Error('this device cannot protect the vault (no PRF support)');
    }
    vaultCredId = b64e(cred.rawId);
    vaultKey = await prfKey(vaultCredId);
    await persistVault();
  }

  function disableLock() {
    localStorage.setItem('mushu_instances', JSON.stringify(instances));
    const home = instances.find((i) => i.url === location.origin);
    if (home?.token) localStorage.setItem('mushu_token', home.token);
    localStorage.removeItem('mushu_vault');
    vaultKey = null;
    vaultCredId = null;
  }

  lockToggle.addEventListener('click', async () => {
    try {
      if (vaultKey) {
        disableLock();
        setStatus('face id lock disabled', true);
      } else {
        await enableLock();
        setStatus('face id lock enabled', true);
      }
    } catch (e) {
      setStatus(e.message || 'face id setup failed', false);
    }
    syncLockToggle();
  });

  connect();
})();
