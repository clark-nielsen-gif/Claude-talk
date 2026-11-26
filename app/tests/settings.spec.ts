// Behaviour of the settings that change the sound (scenario backend).
import { expect, test } from '@playwright/test';

test('the chosen microphone stays chosen, and Windows’ default can be chosen again', async ({ page }) => {
  await page.goto('/?scenario=settings&theme=light&freeze=1');
  const mic = page.getByRole('combobox', { name: 'Din mikrofon' });
  await mic.selectOption('realtek');
  await expect(mic).toHaveValue('realtek');
  await mic.selectOption('');
  await expect(mic).toHaveValue('');
});

test('“Vælg en anden mikrofon” leads to the microphone choice with focus on it', async ({ page }) => {
  await page.goto('/?scenario=main-ok&theme=light&freeze=1');
  await page.getByRole('button', { name: 'Vælg en anden mikrofon' }).click();
  await expect(page.getByRole('combobox', { name: 'Din mikrofon' })).toBeFocused();
});

test('restoring the sound settings asks once, and “Nej” keeps everything', async ({ page }) => {
  await page.goto('/?scenario=settings&theme=light&freeze=1');
  await page.getByRole('button', { name: 'Gendan normale lydindstillinger' }).click();
  await expect(page.getByText('Claude går ud af opkaldet. Er du sikker?')).toBeVisible();
  await page.getByRole('button', { name: 'Nej' }).click();
  await expect(page.getByRole('button', { name: 'Gendan normale lydindstillinger' })).toBeVisible();
});

test('a shortcut without Ctrl is refused, and Fortryd works from the keyboard', async ({ page }) => {
  await page.goto('/?scenario=settings&theme=light&freeze=1');
  await page.getByRole('button', { name: 'Skift: Gør Claude stille i Discord' }).click();
  await page.keyboard.press('Shift+M');
  await expect(page.getByText('Brug Ctrl (eller Windows-tasten) sammen med tasten')).toBeVisible();
  const cancel = page.getByRole('button', { name: 'Fortryd' });
  await cancel.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: 'Fortryd' })).toHaveCount(0);
});

test('after “Gendan”, Discord set to the Claude Talk cable itself is pointed out with a fix', async ({ page }) => {
  await page.goto('/?scenario=main-restored-cable-chosen&theme=light&freeze=1');
  await expect(page.getByText('Discord er sat til »Claude Talk – til Discord«', { exact: false })).toBeVisible();
  await page.getByRole('button', { name: 'Vis mig hvordan' }).first().click();
  await expect(page.getByRole('heading', { name: 'Sæt Discords mikrofon til »Standard«' })).toBeVisible();
});

test('after “Gendan”, a Discord still on the cable from before is told calmly, without a fix to choose', async ({ page }) => {
  await page.goto('/?scenario=main-restored-still-on-cable&theme=light&freeze=1');
  await expect(page.getByText('Discord hører dig gennem Claude Talk, indtil du genstarter Discord.', { exact: false })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Vis mig hvordan' })).toHaveCount(0);
});
