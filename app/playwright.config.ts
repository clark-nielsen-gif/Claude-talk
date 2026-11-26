import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: 'tests',
  timeout: 60_000,
  workers: 4,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:4173',
    launchOptions: {
      ...(process.env.PW_CHROME ? { executablePath: process.env.PW_CHROME } : {}),
      // the CI voice page plays and records audio without a click
      args: ['--autoplay-policy=no-user-gesture-required'],
    },
  },
  webServer: { command: 'npm run preview', url: 'http://127.0.0.1:4173', reuseExistingServer: true, timeout: 60_000 },
});
