/**
 * The numeric-literal guard.
 *
 * A benchmark number may reach a page only through `getFact`, so a literal
 * followed by a result unit in `site/src` is a bug by construction. This
 * module is the single implementation: `cargo xtask site-check` runs this file
 * as a CLI over the site tree, and the Vitest suite imports the same function.
 *
 * The allow-list is deliberately small:
 * - years (`2026`) and versions (`7.4.4`) are identifiers, not measurements;
 * - CSS values are allowed: lengths in `px`/`rem`/`em`/`vh`/`vw`/`ch`/`fr`/
 *   `deg`, and the ambiguous `%`/`s`/`ms` only when they appear in a style
 *   context (a `style=` attribute, a `<style>` block or a CSS declaration).
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export interface GuardViolation {
  readonly path: string;
  readonly line: number;
  readonly text: string;
  readonly match: string;
}

export interface GuardSource {
  readonly path: string;
  readonly text: string;
}

/** Units that can only be a benchmark result. Longest first. */
const RESULT_UNITS = [
  'bytes',
  'byte',
  'KiB',
  'MiB',
  'GiB',
  'TiB',
  'kB',
  'KB',
  'MB',
  'GB',
  'TB',
  'µs',
  'ns',
  'ms',
  'us',
  '×',
  '%',
  'B',
  's',
] as const;

/** Units whose meaning depends on a CSS context. */
const AMBIGUOUS_UNITS = new Set(['%', 's', 'ms']);

const YEAR = /^(?:19|20)\d{2}$/;
const VERSION = /^\d+(?:\.\d+){2,}$/;
const IDENTIFIER_CHAR = /[A-Za-z0-9_.]/;
const UNIT_BOUNDARY = /[A-Za-z0-9_]/;

function isDigit(character: string): boolean {
  return character >= '0' && character <= '9';
}

function isNumberChar(character: string): boolean {
  return isDigit(character) || character === ',' || character === '.';
}

function isHex(character: string): boolean {
  return /[0-9a-fA-F]/.test(character);
}

function inStyleContext(line: string, index: number, inStyleBlock: boolean): boolean {
  if (inStyleBlock) {
    return true;
  }
  const before = line.slice(0, index);
  if (/style\s*=\s*["'{]/.test(before)) {
    return true;
  }
  return /^\s*[a-z-]+\s*:/.test(line);
}

function unitAt(rest: string): string | undefined {
  for (const unit of RESULT_UNITS) {
    if (!rest.startsWith(unit)) {
      continue;
    }
    const next = rest.charAt(unit.length);
    if (next === '' || !UNIT_BOUNDARY.test(next)) {
      return unit;
    }
  }
  return undefined;
}

/**
 * Find every numeric literal followed by a result unit that is not on the
 * allow-list.
 */
export function findTypedNumbers(sources: readonly GuardSource[]): GuardViolation[] {
  const violations: GuardViolation[] = [];
  for (const source of sources) {
    let inStyleBlock = false;
    const lines = source.text.split(/\r?\n/);
    for (let index = 0; index < lines.length; index += 1) {
      const line = lines[index] ?? '';
      if (/<style\b/.test(line)) {
        inStyleBlock = true;
      }
      for (let i = 0; i < line.length; i += 1) {
        const character = line.charAt(i);
        if (character === '#') {
          let j = i + 1;
          while (j < line.length && isHex(line.charAt(j))) {
            j += 1;
          }
          i = j - 1;
          continue;
        }
        if (!isDigit(character)) {
          continue;
        }
        const previous = i > 0 ? line.charAt(i - 1) : '';
        if (previous !== '' && IDENTIFIER_CHAR.test(previous)) {
          continue;
        }
        // A grouped number continues after its comma (`2,743,352`); only the
        // first group starts a token.
        if (previous === ',' && i >= 2 && isDigit(line.charAt(i - 2))) {
          continue;
        }
        let end = i;
        let token = '';
        while (end < line.length && isNumberChar(line.charAt(end))) {
          token += line.charAt(end);
          end += 1;
        }
        while (token.endsWith('.') || token.endsWith(',')) {
          token = token.slice(0, -1);
          end -= 1;
        }
        let unitStart = end;
        while (unitStart < line.length && line.charAt(unitStart) === ' ') {
          unitStart += 1;
        }
        const unit = unitAt(line.slice(unitStart));
        if (unit === undefined) {
          continue;
        }
        if (YEAR.test(token) || VERSION.test(token)) {
          continue;
        }
        if (AMBIGUOUS_UNITS.has(unit) && !inStyleContext(line, i, inStyleBlock)) {
          violations.push({
            path: source.path,
            line: index + 1,
            text: line,
            match: `${token} ${unit}`,
          });
        } else if (!AMBIGUOUS_UNITS.has(unit)) {
          violations.push({
            path: source.path,
            line: index + 1,
            text: line,
            match: `${token} ${unit}`,
          });
        }
      }
      if (/<\/style>/.test(line)) {
        inStyleBlock = false;
      }
    }
  }
  return violations;
}

/** File extensions the guard scans. */
export const GUARD_EXTENSIONS = ['.astro', '.tsx', '.mdx', '.ts'] as const;

function isScanned(path: string): boolean {
  return GUARD_EXTENSIONS.some((extension) => path.endsWith(extension));
}

/** Read every scanned file under a directory, recursively. */
export function collectSources(root: string): GuardSource[] {
  const sources: GuardSource[] = [];
  const walk = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        walk(path);
      } else if (isScanned(entry.name)) {
        sources.push({ path, text: readFileSync(path, 'utf8') });
      }
    }
  };
  if (statSync(root).isDirectory()) {
    walk(root);
  }
  return sources;
}

/** Whether this module is the process entry point rather than an import. */
export function isEntryPoint(metaUrl: string, argv1: string | undefined): boolean {
  if (argv1 === undefined) {
    return false;
  }
  return resolve(argv1) === resolve(fileURLToPath(metaUrl));
}

if (isEntryPoint(import.meta.url, process.argv[1])) {
  const target = process.argv[2] ?? 'site/src';
  const sources = collectSources(target);
  const violations = findTypedNumbers(sources);
  for (const violation of violations) {
    console.error(
      `${violation.path}:${violation.line}: ${violation.match} — ${violation.text.trim()}`,
    );
  }
  if (violations.length > 0) {
    console.error(
      `guard: ${violations.length} typed benchmark number(s) in ${target}; use a Fact id`,
    );
    process.exit(1);
  }
  console.log(`guard: ${sources.length} file(s) clean in ${target}`);
}
