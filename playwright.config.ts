import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/visual',
  outputDir: './test-results/visual',
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  workers: 1,
  reporter: process.env.CI ? [['line'], ['html', { outputFolder: 'test-results/visual-report', open: 'never' }]] : 'line',
  snapshotPathTemplate: 'output/playwright/mushu-gallery/{arg}{ext}',
  expect: { toHaveScreenshot: { animations: 'disabled', caret: 'hide', maxDiffPixelRatio: 0 } },
  use: {
    ...devices['Desktop Chrome'],
    viewport: { width: 390, height: 844 },
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    locale: 'en-US',
    timezoneId: 'UTC',
    deviceScaleFactor: 1,
    baseURL: 'http://studio.test:4173',
    launchOptions: {
      args: ['--host-resolver-rules=MAP studio.test 127.0.0.1,MAP workbench.test 127.0.0.1', '--no-proxy-server'],
    },
    serviceWorkers: 'block',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'node tests/visual/serve.mjs',
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: false,
  },
});
