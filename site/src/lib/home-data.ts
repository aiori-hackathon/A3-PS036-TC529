/**
 * The home page's interactive copy, assembled at build time.
 *
 * The curated text lives in `src/data/ladder-explorer.json`; every number in
 * it is a `{{F-…}}` placeholder that resolves through the fact register. The
 * page emits the resolved object in a non-executable JSON block and the script
 * reads it, so a stale value cannot sit in a string and the typed-number guard
 * still rejects a literal in source.
 */
import explorerFile from '../data/ladder-explorer.json';
import { getFact } from './facts';

const display = (id: string): string => getFact(id).display;
const ratio = (id: string): string => {
  const fact = getFact(id);
  return `${fact.display}${fact.unit}`;
};
const percent = (id: string): string => {
  const fact = getFact(id);
  return `${fact.display}%`;
};
const bytes = (id: string): string => {
  const fact = getFact(id);
  return `${fact.display} ${fact.unit}`;
};

const PLACEHOLDER = /\{\{(F-[a-z0-9]+(?:-[a-z0-9]+)*)\}\}/g;

/** Resolve every `{{F-…}}` placeholder, failing the build on an unknown id. */
export function resolveFactText(text: string): string {
  return text.replace(PLACEHOLDER, (_match, id: string) => display(id));
}

export const evidenceLabels = {
  m: 'Measured',
  d: 'Derived',
  a: 'Analytical',
  x: 'Extrapolated',
  n: '',
} as const;

export type EvidenceLetter = keyof typeof evidenceLabels;

export interface ExplorerLeaf {
  readonly title: string;
  readonly class: EvidenceLetter;
  readonly baseline: string;
  readonly facts: readonly string[];
  readonly claimGroup: string | null;
  readonly text: string;
  readonly command: string;
  readonly verify: string;
}

export interface ExplorerBranch {
  readonly title: string;
  readonly card: string;
  readonly command: string;
  readonly leaves: readonly ExplorerLeaf[];
}

const LETTERS = new Set<EvidenceLetter>(['m', 'd', 'a', 'x', 'n']);

function evidenceLetter(value: string, where: string): EvidenceLetter {
  if (!LETTERS.has(value as EvidenceLetter)) {
    throw new Error(`${where}: unknown evidence letter \`${value}\``);
  }
  return value as EvidenceLetter;
}

/**
 * Resolve the curated data once. Every fact id is looked up even when it only
 * appears in the `facts` list, so a stale id fails `astro build`.
 */
export const explorer: ExplorerBranch[] = explorerFile.branches.map((branch, branchIndex) => ({
  title: branch.title,
  card: resolveFactText(branch.card),
  command: branch.command,
  leaves: branch.leaves.map((leaf, leafIndex) => {
    const where = `ladder-explorer branch ${branchIndex}, leaf ${leafIndex}`;
    for (const id of leaf.facts) {
      getFact(id);
    }
    return {
      title: leaf.title,
      class: evidenceLetter(leaf.class, where),
      baseline: leaf.baseline,
      facts: leaf.facts,
      claimGroup: leaf.claimGroup,
      text: resolveFactText(leaf.text),
      command: leaf.command,
      verify: leaf.verify,
    };
  }),
}));

/** [tab and panel title, panel copy, diagram caption, diagram group id] */
export type Config = [string, string, string, string];

export const configs: Config[] = [
  [
    'A: one full signature per RRset',
    'No ladder. The DNSSEC status quo before any MTL wrapper.',
    `one full signature each, ${bytes('F-message-bytes-a')}`,
    'gA',
  ],
  [
    'B: full MTL signature per message',
    'Every message carries its condensed path joined to the whole ladder.',
    `path plus full ladder, repeated, ${bytes('F-message-bytes-b')} mean`,
    'gB',
  ],
  [
    'C: one ladder, many short paths',
    'The ladder is signed once for the batch. Each message carries only its path.',
    `shared ladder, short path each, ${bytes('F-message-bytes-c')} mean`,
    'gC',
  ],
];

/** The seven capability cards, in explorer order. */
export const capabilities = explorer.map((branch) => ({
  title: branch.title,
  card: branch.card,
  command: branch.command,
}));

/** The three message labels, resolved for the diagram's static legend. */
export const messageLabels = {
  plain: bytes('F-message-bytes-a'),
  condensed: bytes('F-message-bytes-c'),
  signedLadder: bytes('F-ladder-signed-medium'),
  pathMin: bytes('F-condensed-path-bytes'),
} as const;

export const sections = [
  'Top',
  'Results',
  'Capabilities',
  'Ladder',
  'How it works',
  'Architecture',
  'Dashboard',
  'Transparency',
  'Team',
];

export const homeData = {
  explorer,
  configs,
  sections,
  evidence: evidenceLabels,
};

export { display, ratio, percent, bytes };
