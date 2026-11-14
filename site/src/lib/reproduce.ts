/**
 * The reproduction page's commands and expected output.
 *
 * The command lines live in `src/data/reproduce-commands.json`; the unit
 * tests compare them verbatim with the README and the reproducibility notes,
 * so a shortcut that stops working is a failing test rather than a page that
 * quietly drifts. The expected-output lines are assembled from the generated
 * evidence, claims and provenance documents, never typed with a recorded
 * count.
 */

import commandsFile from '../data/reproduce-commands.json';

import type { EvidenceCheck } from './evidence';
import type { ClaimCounts } from './verify';

export interface CommandGroup {
  readonly id: string;
  readonly label: string;
  readonly note: string;
  readonly lines: readonly string[];
}

export interface ReproCommands {
  readonly verify: CommandGroup;
  readonly quickstart: readonly CommandGroup[];
  readonly regenerate: readonly CommandGroup[];
}

export const commands = commandsFile as ReproCommands;

/** Every command line in the document, in reading order. */
export function allCommandLines(document: ReproCommands = commands): string[] {
  return [
    ...document.verify.lines,
    ...document.quickstart.flatMap((group) => group.lines),
    ...document.regenerate.flatMap((group) => group.lines),
  ];
}

export interface ExpectedOutputInput {
  readonly claims: ClaimCounts;
  readonly check: EvidenceCheck | null;
  readonly manifestRows: number;
  readonly documents: number;
  readonly contentIdentity: string;
  readonly commitShort: string;
}

/**
 * The lines a visitor should see after running `cargo xtask sample-check`.
 *
 * A missing report still prints the claim count; the check fields then read
 * as unavailable rather than as zero drift.
 */
export function expectedOutputLines(input: ExpectedOutputInput): string[] {
  const check = input.check;
  const checkLine =
    check === null
      ? `sample-check: ${input.claims.total} claims, report not present, check unavailable`
      : [
          `sample-check: ${input.claims.total} claims,`,
          `${check.agree} in report,`,
          `${check.agree} agree,`,
          `${check.drift} drift,`,
          `${check.missing} missing`,
        ].join(' ');
  return [
    `site-data: wrote ${input.documents} document(s) to ./site/src/data/generated`,
    `site-check: ${input.documents} document(s) fresh; site/src has no typed benchmark number`,
    checkLine,
    `sample-check: ${input.manifestRows} manifest entries match`,
    `sample-check: content identity ${input.contentIdentity} verified`,
    `sample-check: provenance commit ${input.commitShort} present in this repository; content identity verified`,
    'sample-check: committed sample pair is consistent',
  ];
}

/** A readable label for the recorded reproduction mode. */
export function reproductionModeLabel(mode: string): string {
  switch (mode) {
    case 'remeasure':
      return 're-measured';
    case 'report-only':
      return 'report-only (measurements reused)';
    case 'unknown':
      return 'not recorded in this artifact';
    default:
      return mode;
  }
}
