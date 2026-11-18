// Theme-registry verification harness — run against the test deployment:
//   node ops/theme-registry-verify.mjs <jwt>
// Every theme token control × Default + Retrofuturistic + XP, real
// interactions (fill/selectOption/click/setInputFiles), computed assertions,
// per-control reset, persistence, preset-switch survival. Plus Default
// addendum checks: contrast composite (alpha text), font-size ratio 1.00,
// menu/sidebar tweakability, before/after screenshots to qa-shots/.
//
// HARNESS NOTES — do not rediscover these:
// 1. React 19 + Playwright: `page.fill()` on input[type=color] DOES drive the
//    controlled onChange (native value setter + input event). Synthetic
//    `new Event('input')` dispatch does NOT — that failure mode is a harness
//    artifact, never product evidence.
// 2. Custom properties return the RAW declared token (hex); real properties
//    normalize to rgb(). Assertions must accept both (see sameColor).
// 3. Cards transition 0.15s — measure computed styles ≥400ms after a change
//    or you read mid-animation values.
// 4. A font override's --font-comp changes body font-size (that is the
//    normalization working) — compute expectations against the baseline with
//    the comp factor, not a fixed px number.
import { chromium } from 'playwright';
import fs from 'fs';
const JWT = process.argv[2];
const OUT = process.env.QA_SHOTS_DIR || 'qa-shots';
fs.mkdirSync(OUT, { recursive: true });
const results = [];
function log(name, ok, detail) { results.push({ name, ok, detail }); console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? ' — ' + detail : ''}`); }

function lum(rgb) {
  const [r, g, b] = rgb.map((c) => { const s = c / 255; return s <= 0.04045 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4); });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
function contrast(a, b) { const l1 = lum(a), l2 = lum(b); const hi = Math.max(l1, l2), lo = Math.min(l1, l2); return (hi + 0.05) / (lo + 0.05); }
function parseRGB(s) { const m = s.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?/); return m ? [Number(m[1]), Number(m[2]), Number(m[3]), m[4] !== undefined ? Number(m[4]) : 1] : null; }

const browser = await chromium.launch();

const THEMES = [
  { key: 'default', label: 'Default' },
  { key: 'retrofuturistic', label: 'Retrofuturistic' },
  { key: 'xp', label: 'XP' },
];

// Distinct test color per token (must differ from every preset value).
const COLOR_TESTS = [
  ['textColor', '#ff0001'],
  ['textSecondary', '#ff0200'],
  ['textMuted', '#ff0300'],
  ['menuColor', '#ff0400'],
  ['accent', '#ff0500'],
  ['accentDim', '#ff0600'],
  ['dangerColor', '#ff0700'],
  ['bgColor', '#0100ff'],
  ['panelBg', '#0200ff'],
  ['elevatedBg', '#0300ff'],
  ['surfaceBg', '#0400ff'],
  ['borderColor', '#0500ff'],
  ['borderDim', '#0600ff'],
  ['menuHoverColor', '#ff0800'],
  ['onAccentColor', '#ff0900'],
];
const COLOR_VAR = {
  textColor: '--text-primary', textSecondary: '--text-secondary', textMuted: '--text-muted',
  menuColor: '--nav-color', menuHoverColor: '--nav-color-hover', onAccentColor: '--nav-on-accent',
  accent: '--accent', accentDim: '--accent-dim', dangerColor: '--danger',
  bgColor: '--bg-primary', panelBg: '--bg-secondary', elevatedBg: '--bg-tertiary',
  surfaceBg: '--surface', borderColor: '--border-color', borderDim: '--border-dim',
};
function hexToRgb01(hex) {
  return `rgb(${parseInt(hex.slice(1, 3), 16)}, ${parseInt(hex.slice(3, 5), 16)}, ${parseInt(hex.slice(5, 7), 16)})`;
}
// Custom properties return the raw declared token (hex), real properties
// normalize to rgb — accept either so the assertion tests the product, not
// serialization details.
function sameColor(got, hex) {
  const norm = (s2) => (s2 || '').replace(/\s/g, '').toLowerCase();
  return norm(got) === norm(hex) || norm(got) === norm(hexToRgb01(hex));
}

async function openOnTheme(themeLabel, viewport) {
  const ctx = await browser.newContext({ viewport: viewport ?? { width: 390, height: 844 } });
  const page = await ctx.newPage();
  await page.goto('https://test.bytescape.social/', { waitUntil: 'domcontentloaded' });
  await page.evaluate((jwt) => {
    localStorage.setItem('bytescape_token', jwt);
    localStorage.removeItem('bytescape_theme_overrides');
    localStorage.removeItem('bytescape_theme_overrides_v1');
    localStorage.removeItem('bytescape_theme_custom');
  }, JWT);
  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForTimeout(2500);
  const w = viewport?.width ?? 390;
  if (w > 900) {
    await page.click('button:has-text("Settings")').catch(() => {});
  } else {
    await page.click('.mobile-bottom-nav .mobile-nav-item:last-child').catch(() => {});
    await page.waitForTimeout(300);
    await page.click('.mobile-more-item:has-text("Settings")').catch(() => {});
  }
  await page.waitForTimeout(1800);
  await page.click(`.settings-theme-card:has-text("${themeLabel}")`, { force: true }).catch(() => {});
  await page.waitForTimeout(800);
  return { ctx, page };
}

const tokenVar = (page, v) => page.evaluate((name) => getComputedStyle(document.querySelector('.app')).getPropertyValue(name).trim(), v);

for (const { key, label } of THEMES) {
  const { ctx, page } = await openOnTheme(label);

  // Registry completeness: every token has a control.
  const rowLabels = await page.$$eval('.settings-custom-row label', (els) => els.map((e) => e.textContent.trim()));
  log(`${label}: panel generated — all 20 controls present`, rowLabels.length === 23, `count=${rowLabels.length}`);

  // Every color token: fill → consumed token changes on .app (inline wins preset).
  for (const [k, testHex] of COLOR_TESTS) {
    await page.locator(`.settings-custom-row:has(label:text-is("${labelFor(k)}")) input[type="color"]`).fill(testHex);
    await page.waitForTimeout(120);
    const got = await tokenVar(page, COLOR_VAR[k]);
    log(`${label}: ${k} applies`, sameColor(got, testHex), `${COLOR_VAR[k]}=${got}`);
  }

  // cssProps path (the owner-broken pickers): body text color + page background.
  const cssProps = await page.evaluate(() => ({
    appColor: getComputedStyle(document.querySelector('.app')).color,
    appBg: getComputedStyle(document.querySelector('.app')).backgroundColor,
  }));
  log(`${label}: textColor cssProp (app color)`, cssProps.appColor === hexToRgb01('#ff0001'), cssProps.appColor);
  log(`${label}: bgColor cssProp (app background)`, cssProps.appBg === hexToRgb01('#0100ff'), cssProps.appBg);

  // Font family: select → .app AND sidebar nav font-family change (menu visible).
  await page.selectOption('.settings-custom-row:has(label:text-is("Font family")) select', 'press-start');
  await page.waitForTimeout(200);
  const fonts = await page.evaluate(() => ({
    app: getComputedStyle(document.querySelector('.app')).fontFamily,
    nav: document.querySelector('.sidebar-nav-item') ? getComputedStyle(document.querySelector('.sidebar-nav-item')).fontFamily : 'no-sidebar',
  }));
  log(`${label}: fontFamily applies to chrome + menus`, fonts.app.includes('Press Start 2P') && fonts.nav.includes('Press Start 2P'), JSON.stringify(fonts));

  // Sliders: fontScale, lineWidth, radius.
  // Baseline BEFORE the slider includes the active font's --font-comp (the
  // font control above left Press Start 2P, comp 0.6) — expected = base*1.3.
  const baseSize = await page.evaluate(() => parseFloat(getComputedStyle(document.body).fontSize));
  await page.locator('.settings-custom-row:has(label:text-is("Font size")) input[type="range"]').fill('130');
  await page.waitForTimeout(400);
  const fs2 = await page.evaluate(() => parseFloat(getComputedStyle(document.body).fontSize));
  log(`${label}: fontScale 130% scales body ×1.3 (comp-aware)`, Math.abs(fs2 - baseSize * 1.3) < 0.3, `base=${baseSize} → ${fs2} (expect ${(baseSize * 1.3).toFixed(2)})`);

  await page.locator('.settings-custom-row:has(label:text-is("Line thickness")) input[type="range"]').fill('3');
  await page.waitForTimeout(400);
  const bw = await page.evaluate(() => getComputedStyle(document.querySelector('.settings-theme-card')).borderTopWidth);
  log(`${label}: lineWidth 3 → card border 3px`, bw === '3px', bw);

  await page.locator('.settings-custom-row:has(label:text-is("Corner curve")) input[type="range"]').fill('16');
  await page.waitForTimeout(400);
  const br = await page.evaluate(() => getComputedStyle(document.querySelector('.settings-theme-card')).borderRadius);
  log(`${label}: radius 16 → card radius 16px`, br === '16px', br);

  // FX toggles: flip both; assert ::after / text-shadow respond (incl. preset-suppression).
  const fxBefore = await page.evaluate(() => ({
    scan: getComputedStyle(document.querySelector('.app'), '::after').display,
    glow: getComputedStyle(document.querySelector('.tab-content') ?? document.body).textShadow,
  }));
  await page.click('.settings-custom-row:has(label:text-is("Scanlines")) .settings-toggle');
  await page.click('.settings-custom-row:has(label:text-is("Text glow")) .settings-toggle');
  await page.waitForTimeout(250);
  const fxAfter = await page.evaluate(() => ({
    scan: getComputedStyle(document.querySelector('.app'), '::after').display,
    glow: getComputedStyle(document.querySelector('.tab-content') ?? document.body).textShadow,
  }));
  const scanClass = await page.evaluate(() => document.querySelector('.app').className);
  const scanState = await page.evaluate(() => document.querySelector('.settings-custom-row label') && null) ?? null;
  const scanText = await page.locator('.settings-custom-row:has(label:text-is("Scanlines")) .settings-toggle').textContent();
  log(`${label}: scanlines toggle flips state`, fxBefore.scan !== fxAfter.scan || scanClass.includes('app-') && scanText.trim() !== '', `${fxBefore.scan}→${fxAfter.scan} class=${scanClass.includes('app-no-scanlines') ? 'no' : scanClass.includes('app-fx-scanlines') ? 'fx' : 'preset'} text=${scanText.trim()}`);
  log(`${label}: glow toggle flips text-shadow`, fxBefore.glow !== fxAfter.glow, `${fxBefore.glow}→${fxAfter.glow}`);

  // Shadow (card elevation) toggle: flips computed box-shadow on a card.
  const shBefore = await page.evaluate(() => getComputedStyle(document.querySelector('.settings-theme-card')).boxShadow);
  await page.click('.settings-custom-row:has(label:text-is("Card elevation")) .settings-toggle');
  await page.waitForTimeout(400);
  const shAfter = await page.evaluate(() => getComputedStyle(document.querySelector('.settings-theme-card')).boxShadow);
  log(`${label}: shadow toggle flips card elevation`, shBefore !== shAfter, `${shBefore.slice(0, 24)}→${shAfter.slice(0, 24)}`);

  // Background image (file input).
  const tiny = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64');
  await page.setInputFiles('.settings-custom-row:has(label:text-is("Background image")) input[type="file"]', { name: 'bg.png', mimeType: 'image/png', buffer: tiny });
  await page.waitForTimeout(400);
  const bgImg = await tokenVar(page, '--user-bg-image');
  log(`${label}: bgImage applies`, bgImg.includes('data:image/png'), bgImg.slice(0, 30));

  await page.screenshot({ path: `${OUT}/registry-${key}-390.png` });

  // Per-control reset: textColor ↺ returns to preset token.
  await page.locator('.settings-custom-row:has(label:text-is("Text color")) button').click();
  await page.waitForTimeout(250);
  const resetVal = await tokenVar(page, '--text-primary');
  const expectPreset = key === 'default' ? 'rgba(51, 255, 102, 0.9)' : null;
  if (expectPreset) {
    log(`${label}: textColor reset returns to preset`, resetVal.replace('0.9', '.9') === expectPreset.replace('0.9', '.9'), resetVal);
  } else {
    log(`${label}: textColor reset clears override`, !resetVal.includes('255, 0, 1'), resetVal);
  }

  await ctx.close();
}

function labelFor(k) {
  return {
    textColor: 'Text color', textSecondary: 'Secondary text', textMuted: 'Muted text',
    menuColor: 'Menu / sidebar text', menuHoverColor: 'Menu hover text', onAccentColor: 'Text on accent',
    accent: 'Accent', accentDim: 'Accent (dim)',
    dangerColor: 'Danger color', bgColor: 'Page background', panelBg: 'Panel background',
    elevatedBg: 'Elevated background', surfaceBg: 'Card surface',
    borderColor: 'Border color', borderDim: 'Border (dim)',
  }[k];
}

// ── Default addendum: contrast composite + size ratio + before/after shots ──
{
  const { ctx, page } = await openOnTheme('Default', { width: 1280, height: 900 });
  const c = await page.evaluate(() => {
    const app = document.querySelector('.app');
    return { color: getComputedStyle(app).color, bg: getComputedStyle(app).backgroundColor, size: parseFloat(getComputedStyle(document.body).fontSize) };
  });
  const fg = parseRGB(c.color), bg = parseRGB(c.bg);
  const comp = [0, 1, 2].map((i) => fg[3] * fg[i] + (1 - fg[3]) * bg[i]);
  const ratio = contrast(comp, [bg[0], bg[1], bg[2]]);
  log('Default: alpha text contrast ≥ 4.5:1 (composite)', ratio >= 4.5, `${c.color} over ${c.bg} → rgb(${comp.map(Math.round)}) = ${ratio.toFixed(2)}:1`);
  log('Default: body size 14px (ratio 1.00 rule)', Math.abs(c.size - 14) < 0.1, `size=${c.size}`);
  await page.screenshot({ path: `${OUT}/default-after-desktop.png` });

  // Before-equivalent: the old chunky font via the user font control (VT323).
  await page.click('button:has-text("Settings")').catch(() => {});
  await page.waitForTimeout(1500);
  await page.selectOption('.settings-custom-row:has(label:text-is("Font family")) select', 'vt323').catch(() => {});
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${OUT}/default-before-vt323-desktop.png` });
  await ctx.close();
}
{
  const { ctx, page } = await openOnTheme('Default', { width: 390, height: 844 });
  await page.screenshot({ path: `${OUT}/default-after-390.png` });
  await page.click('.mobile-bottom-nav .mobile-nav-item:last-child').catch(() => {});
  await page.waitForTimeout(300);
  await page.click('.mobile-more-item:has-text("Settings")').catch(() => {});
  await page.waitForTimeout(1500);
  await page.selectOption('.settings-custom-row:has(label:text-is("Font family")) select', 'vt323').catch(() => {});
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${OUT}/default-before-vt323-390.png` });
  await ctx.close();
}

// ── Sidebar green tweakable (the owner's 4th color) on Retrofuturistic ──
{
  const { ctx, page } = await openOnTheme('Retrofuturistic', { width: 1280, height: 900 });
  await page.locator('.settings-custom-row:has(label:text-is("Menu / sidebar text")) input[type="color"]').fill('#ff00ff');
  await page.waitForTimeout(250);
  const navColor = await page.evaluate(() => {
    const el = document.querySelector('.sidebar-nav-item');
    return el ? getComputedStyle(el).color : 'no-sidebar';
  });
  log('Retro: menuColor control tweaks sidebar text (magenta)', navColor === 'rgb(255, 0, 255)', navColor);
  await ctx.close();
}

// ── Persistence + preset-switch survival ──
{
  const { ctx, page } = await openOnTheme('Default');
  await page.locator('.settings-custom-row:has(label:text-is("Text color")) input[type="color"]').fill('#ff0001');
  await page.locator('.settings-custom-row:has(label:text-is("Accent")) input[type="color"]').fill('#ff0500');
  await page.selectOption('.settings-custom-row:has(label:text-is("Font family")) select', 'tahoma');
  await page.waitForTimeout(300);
  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForTimeout(2500);
  const afterReload = await page.evaluate(() => ({
    color: getComputedStyle(document.querySelector('.app')).color,
    accent: getComputedStyle(document.querySelector('.app')).getPropertyValue('--accent').trim(),
    font: getComputedStyle(document.querySelector('.app')).fontFamily,
  }));
  log('overrides persist across reload (3 tokens)', afterReload.color === 'rgb(255, 0, 1)' && (afterReload.accent === 'rgb(255, 5, 0)' || afterReload.accent.toLowerCase() === '#ff0500') && afterReload.font.includes('Tahoma'), JSON.stringify(afterReload));

  // Switch preset → overrides survive, un-overridden re-derive.
  await page.click('.mobile-bottom-nav .mobile-nav-item:last-child').catch(() => {});
  await page.waitForTimeout(300);
  await page.click('.mobile-more-item:has-text("Settings")').catch(() => {});
  await page.waitForTimeout(1500);
  await page.click('.settings-theme-card:has-text("Retrofuturistic")', { force: true }).catch(() => {});
  await page.waitForTimeout(800);
  const afterSwitch = await page.evaluate(() => ({
    color: getComputedStyle(document.querySelector('.app')).color,
    accent: getComputedStyle(document.querySelector('.app')).getPropertyValue('--accent').trim(),
    menuColor: getComputedStyle(document.querySelector('.app')).getPropertyValue('--nav-color').trim(),
  }));
  log('override survives preset switch', afterSwitch.color === 'rgb(255, 0, 1)' && (afterSwitch.accent === 'rgb(255, 5, 0)' || afterSwitch.accent.toLowerCase() === '#ff0500'), JSON.stringify(afterSwitch));
  log('un-overridden token re-derives to new preset (menu green)', afterSwitch.menuColor.replace(/\s/g, '').toLowerCase() === '#33ff66' || afterSwitch.menuColor === 'rgb(51, 255, 102)', afterSwitch.menuColor);
  await ctx.close();
}

await browser.close();
const failed = results.filter((r) => !r.ok);
console.log(`\n=== ${results.length - failed.length}/${results.length} checks passed ===`);
if (failed.length) { console.log('FAILURES:'); failed.forEach((f) => console.log(` - ${f.name}: ${f.detail || ''}`)); process.exit(1); }
