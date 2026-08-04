import { expect, test, type Page, type Route } from '@playwright/test';

const token = 'anonymous-fixture-token';
const secondHost = 'http://workbench.test:4173';
const agents = [
  { pane_id: 'pane-codex', agent: 'codex', status: 'working', title: 'Reviewing the test plan', state_change_seq: 41 },
  { pane_id: 'pane-claude', agent: 'claude', status: 'blocked', title: 'Waiting for approval', state_change_seq: 18 },
];
const workspaces = [
  { workspace_id: 'workspace-app', number: 1, label: 'sample-app', status: 'working', focused: true, tab_count: 2 },
];
const tabs = [
  { tab_id: 'tab-build', workspace_id: 'workspace-app', number: 1, label: 'build', status: 'working', focused: true },
  { tab_id: 'tab-tests', workspace_id: 'workspace-app', number: 2, label: 'tests', status: 'idle', focused: false },
];

// A Claude session as the pane renders it: the welcome header, the submitted
// prompt directly under it, the tool transcript, then the composer and status
// line pinned to the bottom. The fixture terminal is exactly 48x50 once the
// font is pinned, so the line count below is the viewport height and every
// line must stay under 48 columns; anything wider wraps and pushes the
// composer off the bottom.
const terminalRows = 50;
const terminalPrompt = 'Fix the failing navigation test and rerun it.';
const composerInner = 44;
const dim = (text: string) => `\u001b[90m${text}\u001b[0m`;
const logo = (text: string) => `\u001b[38;5;209m${text}\u001b[0m`;

// The Claude mark, as Claude Code draws it: three rows of quadrant blocks.
// Column widths differ per row so the text column lines up at column 11.
const terminalHeader = [
  '',
  `${logo(' ▐▛███▜▌')}   \u001b[1mClaude Code\u001b[0m ${dim('v2.1.220')}`,
  `${logo('▝▜█████▛▘')}  ${dim('Fable 5 · Claude Pro')}`,
  `${logo('  ▘▘ ▝▝')}    ${dim('~/projects/sample-app')}`,
  '',
  `${dim('>')} ${terminalPrompt}`,
  '',
];

const terminalBody = [
  '\u001b[1m⏺\u001b[0m I will start from the failing assertion.',
  '',
  '\u001b[32m⏺\u001b[0m Read(tests/ui/navigation.test.ts)',
  dim('  ⎿  Read 84 lines'),
  '',
  '\u001b[1m⏺\u001b[0m The fixture still expects the old route name.',
  '',
  '\u001b[32m⏺\u001b[0m Update(tests/ui/navigation.test.ts)',
  dim('  ⎿  Updated 1 addition and 1 removal'),
  `${dim('     42')} \u001b[31m-  expect(route).toBe('/overview')\u001b[0m`,
  `${dim('     42')} \u001b[32m+  expect(route).toBe('/dashboard')\u001b[0m`,
  '',
  '\u001b[32m⏺\u001b[0m Bash(bun test tests/ui/navigation.test.ts)',
  dim('  ⎿  12 pass, 0 fail in 2.10s'),
  '',
  '\u001b[1m⏺\u001b[0m The navigation suite is green again. The route',
  '  rename in the fixture was the only mismatch.',
];

const terminalComposer = [
  dim(`╭${'─'.repeat(composerInner)}╮`),
  `${dim('│')} > ${' '.repeat(composerInner - 3)}${dim('│')}`,
  dim(`╰${'─'.repeat(composerInner)}╯`),
  dim('  ⏵⏵ accept edits on · ? for shortcuts'),
  dim('  sample-app · main · Fable 5 · 42% left'),
];

const terminalLines = [
  ...terminalHeader,
  ...terminalBody,
  ...Array(terminalRows - terminalHeader.length - terminalBody.length - terminalComposer.length).fill(''),
  ...terminalComposer,
];

// Park the cursor in the composer rather than after the status line: up three
// rows, back to column one, then right past the "│ > " gutter.
const terminalTranscript = `${terminalLines.join('\r\n')}\u001b[3A\r\u001b[4C`;

const updateView = {
  build: { tag: 'v0.4.0', sha: '0123456789abcdef', kind: 'stable' },
  latest: { tag: 'v0.4.0' }, state: 'idle', update_available: false, install_allowed: true, reason: null,
};

function json(route: Route, value: unknown, status = 200) {
  return route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(value) });
}

async function installHarness(page: Page, attention = false) {
  const consoleErrors: string[] = [];
  const requestFailures: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('requestfailed', (request) => requestFailures.push(`${request.method()} ${request.url()}: ${request.failure()?.errorText}`));

  // Pin the terminal font before app.js constructs the terminal. The
  // container's generic monospace resolves to WenQuanYi Zen Hei Mono, a CJK
  // face carrying no quadrant block glyphs, so the Claude header art fell back
  // to a proportional font and landed off the cell grid. xterm measures its
  // cell from the fontFamily option rather than from CSS, so the override has
  // to reach the constructor; Liberation Mono and FreeMono both advance 0.6em,
  // which keeps the quadrant fallback on grid.
  await page.addInitScript(() => {
    let terminal: unknown;
    Object.defineProperty(window, 'Terminal', {
      configurable: true,
      get: () => terminal,
      set: (value: new (options: Record<string, unknown>) => unknown) => {
        terminal = new Proxy(value, {
          construct: (target, args: [Record<string, unknown>]) =>
            new target({ ...args[0], fontFamily: '"Liberation Mono", "FreeMono", monospace' }),
        });
      },
    });
  });

  await page.addInitScript(({ fixtureToken, extraHost, transcript, withAttention }) => {
    Object.defineProperty(navigator, 'serviceWorker', {
      configurable: true,
      value: {
        addEventListener() {},
        register: async () => undefined,
        ready: Promise.resolve({ pushManager: { getSubscription: async () => null } }),
      },
    });
    localStorage.setItem('mushu_token', fixtureToken);
    localStorage.setItem('mushu_instances', JSON.stringify([
      { url: location.origin, token: fixtureToken },
      { url: extraHost, token: fixtureToken },
    ]));
    if (withAttention) {
      localStorage.setItem('mushu_pending_attention', JSON.stringify({
        instance_url: location.origin, pane_id: 'pane-claude', seq: 18,
      }));
    }
    class FixtureWebSocket extends EventTarget {
      static OPEN = 1;
      readyState = FixtureWebSocket.OPEN;
      binaryType = 'arraybuffer';
      onopen: ((event: Event) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;
      onclose: ((event: CloseEvent) => void) | null = null;
      onerror: ((event: Event) => void) | null = null;
      constructor(_url: string) {
        super();
        setTimeout(() => {
          this.onopen?.(new Event('open'));
          const bytes = new TextEncoder().encode(transcript);
          this.onmessage?.(new MessageEvent('message', { data: bytes.buffer }));
        }, 30);
      }
      send(_data: unknown) {}
      close() {}
    }
    Object.defineProperty(window, 'WebSocket', { value: FixtureWebSocket });
  }, { fixtureToken: token, extraHost: secondHost, transcript: terminalTranscript, withAttention: attention });

  await page.route('**/*', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    if (url.pathname === '/api/host') return json(route, { host: url.hostname === 'workbench.test' ? 'workbench' : 'studio', theme: { name: 'catppuccin' } });
    if (url.pathname === '/api/agents') return json(route, { host: 'studio', agents, workspaces, tabs });
    if (url.pathname === '/api/attention') return json(route, {
      pane_id: 'pane-claude', seq: 18, agent: 'claude', title: 'Allow the test command?',
      context: 'The agent wants to run:\n\n  bun test tests/ui/navigation.test.ts\n\nChoose how to continue.',
      choices: [{ key: '1', label: 'Allow once' }, { key: '2', label: 'Deny' }],
    });
    if (url.pathname === '/api/update') return json(route, updateView);
    if (url.pathname === '/push/status') return json(route, { subscribed: false });
    if (url.pathname.startsWith('/push/') || url.pathname === '/api/action') return route.fulfill({ status: 204 });
    return route.continue();
  });

  await page.goto('/');
  await page.addStyleTag({ content: '.brand.blocked { animation: none !important; box-shadow: none !important; } #status { display: none !important; }' });
  await page.locator('.settings-note').evaluate((element) => {
    element.textContent = 'Mushu only installs the latest stable release from the configured project repository. Updates are always manual.';
  });
  await expect(page.locator('#hostname')).toHaveText('studio');
  await expect(page.locator('#chips .brand')).toHaveCount(2);
  await expect(page.locator('#chips .bi-openai')).toHaveCount(1);
  await expect(page.locator('#chips .bi-claude')).toHaveCount(1);

  const assertHealthy = async () => {
    await expect.poll(async () => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth && document.body.scrollWidth <= window.innerWidth)).toBe(true);
    expect(await page.locator('body').innerText()).not.toMatch(/(?:\/Users\/|\/home\/|\.ts\.net|tailscale|tailnet|rmsrob|campingas|anonymous-fixture-token)/i);
    expect(consoleErrors, `unexpected console errors:\n${consoleErrors.join('\n')}`).toEqual([]);
    expect(requestFailures, `unexpected request failures:\n${requestFailures.join('\n')}`).toEqual([]);
  };
  return assertHealthy;
}

async function capture(page: Page, name: string, assertHealthy: () => Promise<void>) {
  await assertHealthy();
  await expect(page).toHaveScreenshot(name, { fullPage: true });
}

async function anonymousScreenshot(page: Page) {
  const bytes = await page.evaluate(async () => {
    const canvas = document.createElement('canvas');
    canvas.width = 320;
    canvas.height = 180;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('canvas unavailable');
    context.fillStyle = '#181825';
    context.fillRect(0, 0, 320, 180);
    context.fillStyle = '#313244';
    context.fillRect(16, 16, 288, 28);
    context.fillStyle = '#89b4fa';
    context.beginPath();
    context.arc(32, 30, 5, 0, Math.PI * 2);
    context.fill();
    context.strokeStyle = '#45475a';
    context.fillStyle = '#1e1e2e';
    for (const [x, y, width, height] of [[16, 58, 88, 106], [116, 58, 188, 48], [116, 116, 88, 48], [216, 116, 88, 48]]) {
      context.fillRect(x, y, width, height);
      context.strokeRect(x, y, width, height);
    }
    context.strokeStyle = '#a6e3a1';
    context.lineWidth = 5;
    context.lineCap = 'round';
    context.lineJoin = 'round';
    context.beginPath();
    context.moveTo(130, 92);
    for (const [x, y] of [[157, 75], [181, 85], [216, 67], [249, 79], [288, 69]]) context.lineTo(x, y);
    context.stroke();
    const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob((value) => value ? resolve(value) : reject(new Error('PNG encoding failed')), 'image/png'));
    return Array.from(new Uint8Array(await blob.arrayBuffer()));
  });
  return Buffer.from(bytes);
}

test('terminal Claude session', async ({ page }) => {
  const healthy = await installHarness(page);
  await expect(page.locator('.xterm-rows')).toContainText('Claude Code');
  await expect(page.locator('.xterm-rows')).toContainText(terminalPrompt);
  await expect(page.locator('.xterm-rows')).toContainText('accept edits on');
  await capture(page, 'terminal-chat.png', healthy);
});

test('two-host drawer', async ({ page }) => {
  const healthy = await installHarness(page);
  await page.locator('#hostname').click();
  await expect(page.locator('#host-list .host')).toHaveCount(2);
  await expect(page.locator('#drawer')).toBeVisible();
  await capture(page, 'two-host-drawer.png', healthy);
});

test('screenshot compose', async ({ page }) => {
  const healthy = await installHarness(page);
  await page.locator('#image-compose').click();
  await page.locator('.vt[data-t="pane-codex"]').click();
  await page.locator('#voice-input').fill('Please compare this screenshot with the expected layout.');
  await page.locator('#voice-image-file').setInputFiles({
    name: 'layout.png', mimeType: 'image/png', buffer: await anonymousScreenshot(page),
  });
  await expect(page.locator('#voice-image-status')).toHaveText('Screenshot ready to send.');
  await capture(page, 'screenshot-compose.png', healthy);
});

test('Settings', async ({ page }) => {
  const healthy = await installHarness(page);
  await page.locator('#settings-btn').click();
  await expect(page.locator('#settings')).toBeVisible();
  await expect(page.locator('.host-card')).toHaveCount(2);
  await expect(page.locator('.host-version')).toContainText(['v0.4.0', 'v0.4.0']);
  await capture(page, 'settings.png', healthy);
});

test('in-app notification attention card', async ({ page }) => {
  const healthy = await installHarness(page, true);
  await expect(page.locator('#attention-card')).toBeVisible();
  await expect(page.locator('#attention-title')).toContainText('claude needs you');
  await capture(page, 'notification-attention.png', healthy);
});

test('fixtures remain anonymous', async () => {
  const serialized = JSON.stringify({ token, secondHost, agents, workspaces, tabs, terminalTranscript });
  expect(serialized).not.toMatch(/(?:\/Users\/|\/home\/|\.ts\.net|tailscale|tailnet|rmsrob|campingas|@)/i);
  expect(secondHost).toMatch(/^http:\/\/[a-z-]+\.test:4173$/);
});

