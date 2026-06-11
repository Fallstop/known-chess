import { chromium } from 'playwright-core';

const browser = await chromium.launch({ executablePath: '/usr/bin/google-chrome', headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
page.on('console', (m) => { if (m.type() === 'error') console.log('CONSOLE ERR:', m.text()); });
page.on('pageerror', (e) => console.log('PAGE ERR:', e.message));

await page.goto('http://localhost:5173');
await page.waitForSelector('.path', { timeout: 10000 });
await page.waitForTimeout(1500);

// 1. select the e2 pawn -> targets with count badges
await page.click('button.hit[aria-label="e2"]');
await page.waitForTimeout(400);
await page.screenshot({ path: '/tmp/kc-shots/1-selected.png' });

// 2. play e4
await page.click('button.hit[aria-label="e4"]');
await page.waitForTimeout(900);
await page.screenshot({ path: '/tmp/kc-shots/2-after-e4.png' });

// 3. hover a continuation in the list -> board preview
await page.hover('.path >> nth=1');
await page.waitForTimeout(300);
await page.screenshot({ path: '/tmp/kc-shots/3-hover-preview.png' });

// 4. play several plies by always picking the LEAST popular known move,
//    to narrow the game count fast and (hopefully) hit a forced line.
for (let i = 0; i < 24; i++) {
	const done = await page.evaluate(() => !document.querySelector('.paths'));
	if (done) break;
	const paths = page.locator('.path');
	const n = await paths.count();
	if (n === 0) break;
	await paths.nth(n - 1).click();
	// wait for either the next choice, an autoplay chain, or the veil
	await page.waitForTimeout(1200);
	const state = await page.evaluate(() => document.querySelector('.state-main')?.textContent);
	const total = await page.evaluate(() => document.querySelector('.counter-row')?.textContent?.trim());
	console.log(`ply ${i}: state=${state} total=${total}`);
	if (state === 'Locked in') {
		await page.screenshot({ path: '/tmp/kc-shots/4-locked-in.png' });
		// let the autoplay run for a while
		for (let j = 0; j < 120; j++) {
			await page.waitForTimeout(1000);
			const veil = await page.evaluate(() => document.querySelector('.veil-title')?.textContent);
			if (veil) { console.log('GAME ENDED:', veil); break; }
		}
		await page.screenshot({ path: '/tmp/kc-shots/5-end.png' });
		break;
	}
	const veil = await page.evaluate(() => document.querySelector('.veil-title')?.textContent);
	if (veil) { console.log('GAME ENDED:', veil); await page.screenshot({ path: '/tmp/kc-shots/5-end.png' }); break; }
}

await page.screenshot({ path: '/tmp/kc-shots/6-final-state.png' });
const hist = await page.evaluate(() => document.querySelector('.history')?.innerText?.slice(0, 400));
console.log('HISTORY:', hist);
await browser.close();
