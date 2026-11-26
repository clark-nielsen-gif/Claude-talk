// Screenshots of every screen and error state, light/dark, 100 % and 150 %.
// Rendered with the scenario backend (src/api.ts), never with fake results in
// the real app.
import { test } from '@playwright/test';

const scenarios = [
  'onboarding-welcome', 'onboarding-setup', 'onboarding-cable-missing', 'onboarding-mic-blocked', 'onboarding-ready',
  'main-ok', 'main-off', 'main-restored', 'main-restored-cable-chosen', 'main-restored-still-on-cable', 'main-output-elsewhere', 'main-muted', 'main-output-fixed', 'main-hotkey-taken', 'main-claude-closed', 'main-claude-login', 'main-claude-not-listening',
  'main-discord-closed', 'main-discord-wrong-mic', 'main-discord-restart', 'main-speakers', 'main-update',
  'error-mic-blocked', 'error-mic-missing', 'error-cable-missing', 'error-unsupported',
  'instructions', 'advanced', 'advanced-add', 'settings', 'advanced-extra-cable', 'help',
];
const themes = ['light', 'dark'] as const;
const scales = [1, 1.5];
const out = process.env.SHOT_DIR ?? 'screenshots';

for (const scale of scales) {
  test.describe(`scale ${scale}`, () => {
    test.use({ viewport: { width: 980, height: 700 }, deviceScaleFactor: scale });
    for (const theme of themes) {
      for (const s of scenarios) {
        test(`${s} ${theme} ${scale}`, async ({ page }) => {
          await page.emulateMedia({ reducedMotion: 'reduce' });
          await page.goto(`/?scenario=${s}&theme=${theme}&freeze=1`);
          await page.waitForLoadState('networkidle');
          await page.evaluate(() => document.fonts.ready);
          if (s === 'onboarding-setup') await page.waitForTimeout(1500);
          if (s === 'onboarding-cable-missing' || s === 'onboarding-mic-blocked') await page.waitForTimeout(3200);
          if (s === 'main-discord-wrong-mic') await page.getByRole('button', { name: /Vis mig hvordan/ }).click();
          if (s === 'advanced-extra-cable') await page.getByRole('heading', { name: 'Claude i et andet program' }).scrollIntoViewIfNeeded();
          if (s === 'advanced-add') await page.getByRole('button', { name: 'Tilføj kilde' }).click();
          if (s === 'help') await page.getByRole('button', { name: 'Kør selvtest' }).click().then(() => page.waitForTimeout(600));
          await page.waitForTimeout(250);
          await page.screenshot({ path: `${out}/${s}--${theme}--${scale * 100}.png` });
        });
      }
    }
  });
}
