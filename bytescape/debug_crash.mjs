import { chromium } from 'playwright';
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
page.on('console', msg => console.log('BROWSER CONSOLE:', msg.text()));
page.on('pageerror', err => console.log('BROWSER CRASH:', err.message));
try {
    await page.goto('http://localhost:1420', { waitUntil: 'load', timeout: 5000 });
} catch (e) {
    console.log("Navigation error:", e.message);
}
await new Promise(r => setTimeout(r, 2000));
await browser.close();
