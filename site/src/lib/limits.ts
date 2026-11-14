/**
 * Build-time text for the Honest limits accordion.
 *
 * The accordion is not hand-written in the page: the three method sections
 * come from the committed methodology document, and the scope sentence and
 * the "what we did not measure" list come from the copy deck. Parsing happens
 * during `astro build`, so a section that is renamed or removed fails the
 * build rather than shipping an empty panel.
 */

import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

export interface LimitItem {
  readonly id: string;
  readonly title: string;
  readonly paragraphs: readonly string[];
  readonly bullets: readonly string[];
}

/**
 * The repository root, found by walking up from the working directory. The
 * site builds from `site/`, while the two source documents live one level
 * above it; a test or a build from the repository root resolves the same
 * files. Vite rewrites a `new URL(..., import.meta.url)` in a bundled module,
 * so path resolution cannot rely on the module's own location here.
 */
function repositoryRoot(): string {
  let directory = resolve(process.cwd());
  for (let depth = 0; depth < 6; depth += 1) {
    if (existsSync(join(directory, 'docs', 'METHODOLOGY.md'))) {
      return directory;
    }
    const parent = resolve(directory, '..');
    if (parent === directory) {
      break;
    }
    directory = parent;
  }
  throw new Error('could not find docs/METHODOLOGY.md above the working directory');
}

const REPOSITORY_ROOT = repositoryRoot();
const METHODOLOGY_PATH = join(REPOSITORY_ROOT, 'docs', 'METHODOLOGY.md');
const COPY_PATH = join(REPOSITORY_ROOT, 'docs', 'site', 'copy.md');

const METHOD_SECTIONS = [
  { id: 'environment', title: 'Environment pinning' },
  { id: 'sampling', title: 'Sampling and extrapolation' },
  { id: 'baselines', title: 'Baselines' },
] as const;

function joinLines(lines: readonly string[]): string {
  return lines
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .join(' ');
}

/**
 * Extract one `## Heading` section as paragraphs and bullets. A bullet keeps
 * its wrapped continuation lines; a paragraph ends at a blank line, a bullet
 * or the next heading.
 */
export function markdownSection(
  markdown: string,
  heading: string,
): { title: string; paragraphs: string[]; bullets: string[] } | null {
  const lines = markdown.split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === `## ${heading}`);
  if (start === -1) {
    return null;
  }
  const body: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (/^##\s+/.test(line)) {
      break;
    }
    body.push(line);
  }

  const paragraphs: string[] = [];
  const bullets: string[] = [];
  let buffer: string[] = [];
  let bullet: string[] | null = null;

  const flush = (): void => {
    if (bullet !== null) {
      const text = joinLines(bullet);
      if (text.length > 0) {
        bullets.push(text);
      }
      bullet = null;
    }
    if (buffer.length > 0) {
      const text = joinLines(buffer);
      if (text.length > 0) {
        paragraphs.push(text);
      }
      buffer = [];
    }
  };

  for (const line of body) {
    const trimmed = line.trim();
    if (trimmed === '') {
      flush();
      continue;
    }
    if (trimmed.startsWith('- ')) {
      if (bullet !== null) {
        const text = joinLines(bullet);
        if (text.length > 0) {
          bullets.push(text);
        }
      }
      if (buffer.length > 0) {
        const text = joinLines(buffer);
        if (text.length > 0) {
          paragraphs.push(text);
        }
        buffer = [];
      }
      bullet = [trimmed.slice(2)];
      continue;
    }
    if (bullet !== null) {
      bullet.push(trimmed);
    } else {
      buffer.push(trimmed);
    }
  }
  flush();

  return { title: heading, paragraphs, bullets };
}

/** The raw lines inside one `<!-- copy:begin id=… -->` block. */
export function copyBlock(text: string, id: string): string | null {
  const begin = new RegExp(`<!--\\s*copy:begin\\s+id=${id}\\s*-->`);
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => begin.test(line));
  if (start === -1) {
    return null;
  }
  const block: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (/<!--\s*copy:end\s*-->/.test(line)) {
      break;
    }
    block.push(line);
  }
  return block.join('\n');
}

/**
 * The value after a labelled line (`Scope: …`, `Body: …`), including wrapped
 * continuation lines up to a blank line or the next label.
 */
function labelledValues(block: string, label: string): string[] {
  const labelPattern = /^[A-Z][A-Za-z ]+:/;
  const lines = block.split(/\r?\n/);
  const values: string[] = [];
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index] ?? '';
    if (!line.startsWith(`${label}:`)) {
      continue;
    }
    const parts = [line.slice(label.length + 1).trim()];
    for (let next = index + 1; next < lines.length; next += 1) {
      const continuation = lines[next] ?? '';
      if (continuation.trim() === '' || labelPattern.test(continuation)) {
        break;
      }
      parts.push(continuation.trim());
      index = next;
    }
    values.push(joinLines(parts));
  }
  return values;
}

/** The exact scope sentence, read from the footer block of the copy deck. */
export function scopeSentence(copy: string): string {
  const block = copyBlock(copy, 'footer');
  if (block === null) {
    throw new Error('the copy deck has no footer block');
  }
  const [scope] = labelledValues(block, 'Scope');
  if (scope === undefined || scope.length === 0) {
    throw new Error('the footer block has no Scope sentence');
  }
  return scope;
}

/** The paragraph list under "What we did not measure". */
export function notMeasuredItems(copy: string): string[] {
  const block = copyBlock(copy, 'not-measured');
  if (block === null) {
    throw new Error('the copy deck has no not-measured block');
  }
  return labelledValues(block, 'Body');
}

/** Assemble the accordion in reading order. */
export function buildLimitItems(methodology: string, copy: string): LimitItem[] {
  const items: LimitItem[] = [
    {
      id: 'scope',
      title: 'Scope',
      paragraphs: [scopeSentence(copy)],
      bullets: [],
    },
  ];

  for (const section of METHOD_SECTIONS) {
    const parsed = markdownSection(methodology, section.title);
    if (parsed === null) {
      throw new Error(`docs/METHODOLOGY.md has no "## ${section.title}" section`);
    }
    items.push({
      id: section.id,
      title: parsed.title,
      paragraphs: parsed.paragraphs,
      bullets: parsed.bullets,
    });
  }

  items.push({
    id: 'not-measured',
    title: 'What we did not measure',
    paragraphs: [],
    bullets: notMeasuredItems(copy),
  });

  return items;
}

const METHODOLOGY = readFileSync(METHODOLOGY_PATH, 'utf8');
const COPY = readFileSync(COPY_PATH, 'utf8');

/** The accordion content, parsed once per build from the committed files. */
export function loadLimitItems(): LimitItem[] {
  return buildLimitItems(METHODOLOGY, COPY);
}

function escapeHtml(text: string): string {
  return text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}

/**
 * A deliberately small inline renderer for the methodology prose: escape
 * first, then the backtick and bold forms the document uses. Links keep their
 * label but lose their target, because the source document lives beside the
 * site rather than under a route.
 */
export function renderInline(text: string): string {
  return escapeHtml(text)
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
}
