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

const terminalTranscript = [
  '\u001b[1;36mCodex\u001b[0m  I inspected the sample project and found one failing check.\r\n',
  '\u001b[90m       tests/ui/navigation.test.ts:42\u001b[0m\r\n\r\n',
  '\u001b[1;35mClaude\u001b[0m Shall I update the fixture and rerun the focused suite?\r\n',
  '\u001b[33m       Waiting for approval…\u001b[0m\r\n\r\n',
  '\u001b[32mCodex\u001b[0m  The remaining checks are stable.\r\n',
  '\u001b[90m$ ready\u001b[0m ',
].join('');

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

test('terminal Codex and Claude chat', async ({ page }) => {
  const healthy = await installHarness(page);
  await expect(page.locator('.xterm-rows')).toContainText('Waiting for approval');
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
