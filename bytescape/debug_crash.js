const { chromium } = require('playwright');
(async () => {
    const browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    page.on('console', msg => console.log('BROWSER CONSOLE:', msg.text()));
    page.on('pageerror', err => console.log('BROWSER CRASH:', err.message));
    try {
        await page.goto('http://localhost:1420', { waitUntil: 'networkidle', timeout: 5000 });
    } catch (e) {
        console.log("Navigation timeout or error:", e.message);
    }
    await browser.close();
})();
