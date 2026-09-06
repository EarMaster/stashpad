// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.

/**
 * Photograph the app for the website.
 *
 * Starts the Vite dev server on the demo page (the real UI, a fake backend, invented
 * data), walks each scene, and writes one PNG per scene per locale into
 * `screenshots/out/`. The website's release job picks those up; nothing here writes
 * outside this folder.
 *
 *   npm run screenshots                 every scene, both locales
 *   npm run screenshots -- --lang en    one locale
 *   npm run screenshots -- --scene queue,settings
 *   npm run screenshots -- --headed     watch it happen
 *
 * The screenshots are meant to be reproducible: the demo freezes the clock, the fixtures
 * are fixed, and the viewport and device scale factor are pinned here. Two runs on the
 * same machine should produce identical files; two different operating systems will
 * still differ in font rendering, which is why the release job is the one that counts.
 */

import { spawn } from 'node:child_process';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const HERE = dirname(fileURLToPath(import.meta.url));
const APP_ROOT = join(HERE, '..');
const OUT_DIR = join(HERE, 'out');

/** The app's own default window size, so the proportions are the ones users see. */
const VIEWPORT = { width: 800, height: 600 };
/** Retina-grade output: the website shows these at half size on a HiDPI display. */
const SCALE = 2;
const PORT = Number(process.env.SCREENSHOT_PORT ?? 5199);

const LOCALES = ['en', 'de'];

/**
 * What to photograph.
 *
 * `drive` receives the page once the app has mounted and leaves it in the state to be
 * captured. Every step goes through the real UI - clicking what a user clicks - so a
 * scene that stops matching the app fails loudly here instead of quietly producing a
 * screenshot of the wrong thing.
 */
const SCENES = [
    {
        id: 'queue',
        async drive() {
            // The landing state, which is the point of it. Nothing to drive.
        },
    },
    {
        id: 'editor',
        async drive(page) {
            const editor = page.locator('textarea').first();
            await editor.click();
            await editor.fill(
                'Upload dies when a file name repeats\n\n' +
                    'The second file wins on disk and the row keeps the first one size. ' +
                    'Log from the staging box is attached. #bug',
            );
            // Show the toolbar doing something: make the first line a heading, then put
            // the caret at the end so the screenshot has no selection highlight in it.
            await editor.evaluate((el) => el.setSelectionRange(0, 35));
            await page.locator('button:has(svg.lucide-heading)').click();
            await editor.evaluate((el) => el.setSelectionRange(el.value.length, el.value.length));
            await page.waitForTimeout(200);
        },
    },
    {
        id: 'filters',
        async drive(page) {
            await page.locator('button:has(svg.lucide-funnel)').click();
            await page.waitForTimeout(300);
        },
    },
    {
        id: 'contexts',
        async drive(page) {
            await openSettings(page);
            await clickByText(page, [/manage contexts/i, /kontexte verwalten/i]);
            await page.waitForTimeout(400);
            // Sort by last used, so the context the demo is sitting in leads instead of
            // whichever one happens to sort first alphabetically.
            await page.locator('button:has(svg.lucide-clock)').first().click();
            await page.waitForTimeout(300);
        },
    },
    {
        id: 'settings-sync',
        async drive(page) {
            await openSettings(page);
            await showSection(page, SECTION.cloudSync);
        },
    },
    {
        id: 'settings-ai',
        async drive(page) {
            await openSettings(page);
            await showSection(page, SECTION.promptEnhancement);
        },
    },
];

/**
 * Settings sections, by their order in the panel.
 *
 * Their headings are translated, so an index is what survives running the same scene in
 * a second locale. It does not survive a section being inserted above one of these -
 * which is the trade, and why a wrong screenshot here is obvious rather than subtle.
 */
const SECTION = {
    contextManagement: 0,
    cloudSync: 1,
    general: 2,
    shortcuts: 3,
    appearance: 4,
    promptEnhancement: 5,
    updates: 6,
};

/** Click the first element whose text matches one of `patterns`. */
async function clickByText(page, patterns) {
    for (const pattern of patterns) {
        const target = page.getByText(pattern).first();
        if ((await target.count()) > 0 && (await target.isVisible())) {
            await target.click();
            return;
        }
    }
    throw new Error(`nothing on the page matched ${patterns.join(' or ')}`);
}

/**
 * Open the settings view.
 *
 * The header's icon buttons carry no accessible name - the app's tooltip action removes
 * the `title` it reads - so the icon itself is the only stable handle. Lucide stamps its
 * name into the class, and a renamed icon fails the run instead of photographing the
 * wrong button.
 */
async function openSettings(page) {
    await page.locator('button:has(svg.lucide-settings)').click();
    await page.waitForSelector('h2');
    await page.waitForTimeout(400);
}

/**
 * Bring a settings section to the top of the scrolling panel.
 *
 * Deliberately not `scrollIntoView`, which scrolls every scrollable ancestor and takes
 * the view's own header with it - the first attempt at this photographed a settings page
 * with no title bar. Only the panel is moved, and the section is left a little below the
 * top edge so it does not read as clipped.
 */
async function showSection(page, index) {
    const heading = page.locator('h2').nth(index);
    await heading.evaluate((el) => {
        let scroller = el.parentElement;
        while (scroller && scroller.scrollHeight <= scroller.clientHeight) {
            scroller = scroller.parentElement;
        }
        if (!scroller) return;
        const top = el.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
        scroller.scrollTop += top - 16;
    });
    await page.waitForTimeout(300);
}

/**
 * Re-encode a PNG screenshot as WebP, using the browser that just took it.
 *
 * These files are committed to the website repo and served on the landing page, and a
 * PNG of a full app window is several hundred kilobytes - times six scenes, times two
 * locales, every release, forever. WebP at this quality is a third of that with no
 * visible difference on UI text, and Chromium already has the encoder, so this costs a
 * canvas rather than a native dependency.
 */
async function toWebp(page, png) {
    const encoded = await page.evaluate(async (base64) => {
        const image = new Image();
        image.src = `data:image/png;base64,${base64}`;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        canvas.getContext('2d').drawImage(image, 0, 0);
        return canvas.toDataURL('image/webp', 0.92).split(',')[1];
    }, png.toString('base64'));
    return Buffer.from(encoded, 'base64');
}

function parseArgs(argv) {
    const args = { locales: LOCALES, scenes: SCENES.map((s) => s.id), headed: false };
    for (let i = 0; i < argv.length; i++) {
        const value = argv[i + 1];
        if (argv[i] === '--lang') args.locales = value.split(',');
        else if (argv[i] === '--scene') args.scenes = value.split(',');
        else if (argv[i] === '--headed') args.headed = true;
    }
    return args;
}

/**
 * The version being photographed, from `package.json`.
 *
 * The website prints it under the gallery, so it has to be the version the source tree
 * is at rather than a label. The release job overrides it with the tag it is publishing;
 * `SCREENSHOT_VERSION` is that hook.
 */
async function appVersion() {
    const manifest = JSON.parse(await readFile(join(APP_ROOT, 'package.json'), 'utf8'));
    return manifest.version;
}

/** Whether something already answers for the demo page on the capture port. */
async function demoPageIsUp() {
    try {
        const response = await fetch(`http://localhost:${PORT}/screenshots/demo.html`);
        return response.ok;
    } catch {
        return false;
    }
}

/**
 * Start Vite on its own port and resolve once it answers.
 *
 * A server already on the port is reused rather than fought with - handy while working
 * on a scene, and the reason the port is not the app's usual 1420: a `npm run dev` you
 * have open should not silently become the thing being photographed.
 */
async function startDevServer() {
    if (await demoPageIsUp()) {
        process.stdout.write(`reusing the dev server already on port ${PORT}\n`);
        return { kill() {} };
    }

    // Vite is run directly rather than through `npx`, so that killing the child kills
    // the server: on Windows the npx shim is a shell that dies on its own and leaves
    // Vite holding the port, which the next run then reuses without noticing.
    const child = spawn(
        process.execPath,
        [join(APP_ROOT, 'node_modules', 'vite', 'bin', 'vite.js'), '--port', String(PORT), '--strictPort'],
        { cwd: APP_ROOT, stdio: 'pipe' },
    );
    child.stdout.on('data', (d) => process.env.SCREENSHOT_VERBOSE && process.stdout.write(d));
    child.stderr.on('data', (d) => process.stderr.write(d));

    // Cold, Vite spends most of a minute pre-bundling before it answers.
    const deadline = Date.now() + 180_000;
    while (Date.now() < deadline) {
        if (await demoPageIsUp()) return child;
        if (child.exitCode !== null) throw new Error(`the dev server exited (${child.exitCode})`);
        await new Promise((r) => setTimeout(r, 500));
    }
    child.kill();
    throw new Error('the dev server never came up');
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const scenes = SCENES.filter((s) => args.scenes.includes(s.id));
    if (scenes.length === 0) throw new Error('no scene matched --scene');

    await rm(OUT_DIR, { recursive: true, force: true });

    const server = await startDevServer();
    const browser = await chromium.launch({ headless: !args.headed });
    const manifest = [];

    // A blank page used as an image encoder; see `toWebp`.
    const encoder = await browser.newPage();

    try {
        for (const locale of args.locales) {
            const context = await browser.newContext({
                viewport: VIEWPORT,
                deviceScaleFactor: SCALE,
                locale,
                colorScheme: 'dark',
                // The app animates on mount and on every view change. Left running, the
                // capture catches whatever frame it lands on and no two runs agree.
                reducedMotion: 'reduce',
            });
            const page = await context.newPage();
            page.on('pageerror', (error) => {
                throw new Error(`the demo threw: ${error.message}`);
            });

            await mkdir(join(OUT_DIR, locale), { recursive: true });

            for (const scene of scenes) {
                await page.goto(
                    `http://localhost:${PORT}/screenshots/demo.html?lang=${locale}&theme=dark`,
                    { waitUntil: 'domcontentloaded' },
                );
                await page.waitForSelector('html[data-demo="ready"]', { timeout: 30_000 });
                // Web fonts and the first paint of the queue; the app hydrates its list
                // asynchronously and a screenshot taken here would miss the rows.
                await page.evaluate(() => document.fonts.ready);
                await page.waitForTimeout(500);

                await scene.drive(page);

                // Park the pointer in a dead corner so no button is left hovered.
                await page.mouse.move(2, VIEWPORT.height - 2);
                await page.waitForTimeout(400);

                // Then sweep up any tooltip that is still on screen.
                //
                // Clicking an icon button that changes the view strands its tooltip: the
                // trigger unmounts while the tooltip is up, and with no element left to
                // leave, nothing ever hides it. It parks itself in the top-left corner -
                // "Settings" floating over the contexts page - and a real user sees the
                // same thing after clicking the gear. Worth fixing in the app; until then
                // a screenshot must not ship it.
                await page.evaluate(() => {
                    document.querySelectorAll('.tooltip').forEach((el) => el.remove());
                });

                const png = await page.screenshot({ animations: 'disabled' });
                const name = `${scene.id}.webp`;
                await writeFile(join(OUT_DIR, locale, name), await toWebp(encoder, png));
                manifest.push({ scene: scene.id, locale, file: `${locale}/${name}` });
                process.stdout.write(`captured ${locale}/${name}\n`);
            }

            await context.close();
        }

        await writeFile(
            join(OUT_DIR, 'manifest.json'),
            `${JSON.stringify(
                {
                    // The website reads this: the version stamps the gallery, and the
                    // scene ids are the keys its captions are translated under
                    // (`showcase.<id>` in `website/src/i18n/ui.ts`). A new scene needs a
                    // caption there or it shows up unlabelled.
                    generatedFor: process.env.SCREENSHOT_VERSION ?? (await appVersion()),
                    viewport: VIEWPORT,
                    scale: SCALE,
                    scenes: SCENES.map(({ id }) => id),
                    files: manifest,
                },
                null,
                2,
            )}\n`,
        );
    } finally {
        await browser.close();
        server.kill();
    }
}

await main();
