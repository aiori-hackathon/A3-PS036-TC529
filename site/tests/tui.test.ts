import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import commands from '../src/data/tui-commands.json';
import {
  TOUCH_CONTROLS,
  TUI_DATA_DIR,
  TUI_DATA_FILES,
  TUI_KEY_HINTS,
  TUI_MANIFEST_PATH,
  isTuiAssetPath,
  keyEventInit,
  nextTabIndex,
  parseManifest,
  tuiManifestFrom,
} from '../src/lib/tui';

const repoRoot = fileURLToPath(new URL('../..', import.meta.url));
const readme = readFileSync(join(repoRoot, 'README.md'), 'utf8');

function read(relative: string): string {
  return readFileSync(join(repoRoot, relative), 'utf8');
}

/** The text between two marker comments, markers included. */
function between(text: string, begin: string, end: string): string {
  const start = text.indexOf(begin);
  const stop = text.indexOf(end);
  if (start === -1 || stop === -1 || stop < start) {
    throw new Error(`missing markers ${begin} … ${end}`);
  }
  return text.slice(start, stop + end.length);
}

/** The A/B/C install commands from the README's dashboard guide. */
function guideBlocks(): string[][] {
  const guide = between(readme, '<!-- tui-guide:begin -->', '<!-- tui-guide:end -->');
  const pattern = /\*\*([ABC])\. [^\n]*\*\*[^\n]*\n\n```bash\n([\s\S]*?)```/g;
  const blocks: string[][] = [];
  for (const match of guide.matchAll(pattern)) {
    const body = match[2];
    if (body === undefined) throw new Error('a README code block has no body');
    blocks.push(
      body
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line.length > 0),
    );
  }
  return blocks;
}

describe('the dashboard manifest', () => {
  it('accepts only a same-origin build under the published prefix', () => {
    for (const valid of ['/tui/climb-web-0123abcd.js', '/tui/climb-web-0123abcd_bg.wasm']) {
      expect(isTuiAssetPath(valid), valid).toBe(true);
    }
    for (const invalid of [
      '/data/climb-results.json',
      'climb-web.js',
      'https://example.test/tui/climb-web.js',
      '//example.test/tui/climb-web.js',
      '/tui/../crates/climb-web.js',
      '/tui/nested/climb-web.js',
      '/tui/climb-web.js?v=2',
      '/tui/',
      '',
      42,
      null,
    ]) {
      expect(isTuiAssetPath(invalid), String(invalid)).toBe(false);
    }
  });

  it('parses the two published files and refuses anything else', () => {
    expect(
      parseManifest({ entry: '/tui/climb-web-0123abcd.js', wasm: '/tui/climb-web-0123abcd_bg.wasm' }),
    ).toEqual({ entry: '/tui/climb-web-0123abcd.js', wasm: '/tui/climb-web-0123abcd_bg.wasm' });

    for (const invalid of [
      null,
      [],
      {},
      { entry: '/tui/climb-web.js' },
      { entry: '/tui/climb-web.js', wasm: '/tui/climb-web.wasm' },
      { entry: '/tui/notes.txt', wasm: '/tui/climb-web_bg.wasm' },
      { entry: 'https://example.test/app.js', wasm: '/tui/climb-web_bg.wasm' },
      { entry: '/tui/../app.js', wasm: '/tui/climb-web_bg.wasm' },
      { entry: 1, wasm: 2 },
    ]) {
      expect(parseManifest(invalid), JSON.stringify(invalid)).toBeNull();
    }
  });

  it('picks the hashed entry and binary out of a trunk build', () => {
    expect(
      tuiManifestFrom([
        'index.html',
        '.trunk-lock',
        'climb-web-0123abcd.js',
        'climb-web-0123abcd_bg.wasm',
      ]),
    ).toEqual({
      entry: '/tui/climb-web-0123abcd.js',
      wasm: '/tui/climb-web-0123abcd_bg.wasm',
    });

    expect(tuiManifestFrom([])).toBeNull();
    expect(tuiManifestFrom(['index.html'])).toBeNull();
    expect(tuiManifestFrom(['climb-web.js'])).toBeNull();
    expect(tuiManifestFrom(['climb-web-0123abcd_bg.wasm'])).toBeNull();
  });

  it('names the origin paths the Rust loader fetches', () => {
    expect(TUI_MANIFEST_PATH).toBe('/tui/manifest.json');
    expect(TUI_DATA_DIR).toBe('/data');

    const rust = [
      read('crates/climb-tui/src/data/mod.rs'),
      read('crates/climb-tui/src/data/ladder.rs'),
      read('crates/climb-tui/src/data/live.rs'),
    ].join('\n');
    for (const file of TUI_DATA_FILES) {
      expect(rust, `${file} must be a path the dashboard fetches`).toContain(`"${TUI_DATA_DIR}/${file}"`);
      expect(existsSync(join(repoRoot, 'docs', 'sample-output', file)), file).toBe(true);
    }
  });
});

describe('Run it locally', () => {
  it('offers exactly the commands the README prints, in the same order', () => {
    const blocks = guideBlocks();
    expect(blocks.length, 'the README must show three install routes').toBe(3);
    expect(commands.tabs.length).toBe(blocks.length);

    commands.tabs.forEach((tab, index) => {
      expect(tab.lines, `${tab.id} must match README block ${index}`).toEqual(blocks[index]);
    });

    for (const tab of commands.tabs) {
      for (const line of tab.lines) {
        expect(readme, `${tab.id} prints a command the README does not`).toContain(line);
      }
    }
  });

  it('keeps one tab per README route and no duplicate ids', () => {
    const ids = commands.tabs.map((tab) => tab.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toEqual(['clone', 'install', 'container']);
    for (const tab of commands.tabs) {
      expect(tab.label.length).toBeGreaterThan(0);
      expect(tab.note.length).toBeGreaterThan(0);
    }
  });
});

describe('the touch row and the hint chip', () => {
  it('sends keys the dashboard documents, with the modifiers a keyboard sends', () => {
    const spellings: Record<string, string> = { ArrowUp: '↑', ArrowDown: '↓' };
    for (const control of TOUCH_CONTROLS) {
      const spelling = spellings[control.key] ?? control.key;
      expect(readme, `${control.id} sends ${control.key}, which the keys table omits`).toContain(
        `\`${spelling}\``,
      );
    }

    // The accessible name must contain the visible label as a whole word, or
    // a screen reader and the button say different things.
    for (const control of TOUCH_CONTROLS) {
      expect(
        control.label.toLowerCase().split(/\W+/),
        `${control.id} says ${control.hint} but is named ${control.label}`,
      ).toContain(control.hint.toLowerCase());
    }

    const init = keyEventInit(TOUCH_CONTROLS[0]!);
    expect(init).toEqual({ key: 'Tab', shiftKey: true, bubbles: true, cancelable: true });
    expect(new Set(TOUCH_CONTROLS.map((control) => control.id)).size).toBe(TOUCH_CONTROLS.length);
    expect(new Set(TOUCH_CONTROLS.map((control) => `${control.key}:${control.shift}`)).size).toBe(
      TOUCH_CONTROLS.length,
    );
  });

  it('documents every hint key in the README', () => {
    for (const hint of TUI_KEY_HINTS) {
      expect(hint.keys.length).toBeGreaterThan(0);
      expect(hint.action.length).toBeGreaterThan(0);
    }
    for (const key of ['Tab', '?', 'T', 't']) {
      expect(readme, `the hint chip names ${key}`).toContain(`\`${key}\``);
    }
  });

  it('moves the run tabs with the arrow, home and end keys only', () => {
    expect(nextTabIndex(0, 'ArrowRight', 3)).toBe(1);
    expect(nextTabIndex(2, 'ArrowRight', 3)).toBe(0);
    expect(nextTabIndex(0, 'ArrowLeft', 3)).toBe(2);
    expect(nextTabIndex(1, 'Home', 3)).toBe(0);
    expect(nextTabIndex(1, 'End', 3)).toBe(2);
    expect(nextTabIndex(1, 'Enter', 3)).toBeNull();
    expect(nextTabIndex(1, 'Tab', 3)).toBeNull();
    expect(nextTabIndex(0, 'ArrowRight', 0)).toBeNull();
    expect(nextTabIndex(9, 'ArrowRight', 3)).toBe(1);
  });
});

describe('the window is wired into the site', () => {
  const shell = read('site/src/components/TuiWindow.astro');
  const page = read('site/src/pages/tui.astro');
  const home = read('site/src/pages/index.astro');
  const config = read('site/astro.config.mjs');

  it('offers every state a reader can meet', () => {
    for (const hook of [
      'data-tui-state',
      'data-tui-boot',
      'data-tui-paused',
      'data-tui-fault',
      'data-tui-fault-code',
      'data-tui-retry',
      'data-tui-load',
      'data-tui-controls',
      'data-tui-hints',
    ]) {
      expect(shell, `${hook} is part of the window`).toContain(hook);
    }
    expect(shell).toContain('id="grid"');
    expect(shell).toContain('data-keys="focus"');
    expect(shell).toContain('Loading the recorded run');
    expect(shell).toContain('trunk build --release');
    expect(shell).toContain('TUI_MANIFEST_PATH');
  });

  it('loads the module on idle, and the home window only once reached', () => {
    expect(shell).toContain('requestIdleCallback');
    expect(shell).toContain('IntersectionObserver');
    expect(shell).toContain("window.scrollY === 0");
    expect(shell).toContain("prefers-reduced-data: reduce");
    expect(shell).toContain("frame.dataset.tuiMode === 'page'");
  });

  it('publishes the module and the data from one build integration', () => {
    expect(config).toContain("'astro:server:setup'");
    expect(config).toContain("'astro:build:done'");
    expect(config).toContain('tuiManifestFrom');
    expect(config).toContain('TUI_DATA_FILES');
    expect(config).toContain("'/tui/manifest.json'");
    expect(config).toContain("join(out, 'tui')");
    expect(config).toContain("join(out, 'data')");
  });

  it('renders the full page with its controls, tabs and tour link', () => {
    expect(page).toContain('<TuiWindow />');
    expect(page).toContain('href="/tui?tour=1"');
    expect(page).toContain('data-tui-tabs');
    expect(page).toContain("role=\"tablist\"");
    expect(page).toContain('nextTabIndex');
    expect(page).toContain("import commands from '../data/tui-commands.json'");
  });

  it('puts the embedded window after the hero on the home route', () => {
    expect(home).toContain('TuiWindow');
    expect(home).toContain('mode="home"');
    expect(home.indexOf('<TuiWindow')).toBeGreaterThan(home.indexOf('<HeroArt'));
  });
});
