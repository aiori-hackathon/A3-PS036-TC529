/**
 * The home page's interactive copy, assembled at build time.
 *
 * The prototype typed the explorer branches, the configuration captions and
 * the capability descriptions into an inline script. They are data now: the
 * page emits this object in a non-executable JSON block and the script reads
 * it. Every number is interpolated from the fact register, so a stale value
 * cannot sit in a string, and `cargo xtask site-check` still rejects a typed
 * benchmark number in the source.
 */
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

export const evidenceLabels = {
  m: 'Measured',
  d: 'Derived',
  a: 'Analytical',
  n: '',
} as const;

export type EvidenceLetter = keyof typeof evidenceLabels;

/** [leaf title, evidence letter, description, reproducing command] */
export type ExplorerLeaf = [string, EvidenceLetter, string, string];

/** [branch title, leaves] */
export type ExplorerBranch = [string, ExplorerLeaf[]];

export const explorer: ExplorerBranch[] = [
  [
    'Signing performance',
    [
      [
        'SLH-DSA signing',
        'm',
        'Criterion benchmark groups time signing for plain SLH-DSA (A) and MTL with and without amortisation (B, C).',
        'cargo xtask bench-all',
      ],
      [
        'Batch speed-up',
        'd',
        `Medium dataset, C vs A: ${ratio('F-sign-medium-c-vs-a')}. Per-message cost multiplied by message count, not a whole-batch run.`,
        'cargo xtask deck-facts',
      ],
    ],
  ],
  [
    'Verification performance',
    [
      [
        'Per-message verify',
        'm',
        'Verification cost per message under each configuration.',
        'cargo xtask bench-all',
      ],
      [
        'Trusted cached ladder',
        'd',
        `With a verifier that trusts an already-checked ladder, C vs A is ${ratio('F-verify-medium-c-vs-a')}.`,
        'cargo xtask deck-facts',
      ],
    ],
  ],
  [
    'Wire efficiency',
    [
      [
        'Wire bytes, Medium',
        'm',
        `A: ${bytes('F-wire-medium-a')}. B: ${bytes('F-wire-medium-b')}. C: ${bytes('F-wire-medium-c')}.`,
        'cargo xtask wire-report',
      ],
      [
        'Wire reduction',
        'd',
        `C vs A is ${percent('F-wire-medium-c-vs-a')}, computed from the measured bytes. C vs B is ${percent('F-wire-medium-c-vs-b')}.`,
        'cargo xtask wire-report',
      ],
      [
        'UDP payload fit',
        'a',
        `A closed-form model in climb-analytical. In configuration C, ${display('F-udp-c-fit')} modelled response profiles fit the ${display('F-udp-payload')}-byte payload.`,
        'cargo xtask wire-report',
      ],
    ],
  ],
  [
    'Memory analysis',
    [
      [
        'Memory probe',
        'm',
        'Memory probes in climb-bench record real usage.',
        'cargo xtask memory-report',
      ],
      [
        'Measured vs model',
        'd',
        `Small dataset: measured memory is ${ratio('F-mem-small-ratio')} the analytical estimate.`,
        'cargo xtask memory-report',
      ],
    ],
  ],
  [
    'Ladder visualisation',
    [
      [
        'Ladder verification',
        'n',
        'Checks a recorded ladder fixture from the data folder.',
        'cargo xtask ladder-verify',
      ],
      [
        'Terminal dashboard',
        'n',
        'climb-tui shows ladder shape, results and the Integrity tab. It opens on committed sample results.',
        'cargo run -p climb-tui',
      ],
    ],
  ],
  [
    'DNS packet replay',
    [
      [
        'Recorded capture',
        'd',
        `Live response for www.climb.example. A: condensed vs full is a ${percent('F-dns-www-a')} reduction, baseline B.`,
        'cargo xtask dns-demo',
      ],
      [
        'Live pipeline',
        'n',
        'Patched NSD and Unbound in local Docker capture real wire bytes. Local only, no Internet deployment.',
        'cargo xtask dns-demo',
      ],
    ],
  ],
  [
    'Integrity and provenance',
    [
      [
        'Claims registry',
        'n',
        'Every claim embeds a JSON pointer into the results, its expected value and a pass or fail mark.',
        'cargo xtask report --check',
      ],
      [
        'Docs drift check',
        'n',
        'Fails if any number in the README or docs is not in the quote sheet.',
        'cargo xtask docs-check',
      ],
      [
        'Release bundle',
        'n',
        'Report, results, fixtures, SHA256SUMS and a PROVENANCE.txt naming the commit.',
        'cargo xtask package',
      ],
    ],
  ],
];

/** [tab and panel title, panel copy, diagram caption, diagram group id] */
export type Config = [string, string, string, string];

export const configs: Config[] = [
  [
    'A: one full signature per RRset',
    'No ladder. The DNSSEC status quo before any MTL wrapper.',
    'one full signature each',
    'gA',
  ],
  [
    'B: full MTL signature per message',
    'Every message carries its condensed path joined to the whole ladder.',
    'path plus full ladder, repeated',
    'gB',
  ],
  [
    'C: one ladder, many short paths',
    'The ladder is signed once for the batch. Each message carries only its path.',
    'shared ladder, short path each',
    'gC',
  ],
];

/** [description, reproducing command] in explorer order. */
export const capabilities: [string, string][] = [
  [
    'Criterion timings of SLH-DSA signing, with and without MTL.',
    'cargo xtask bench-all',
  ],
  [
    'Per-message verification, including a trusted cached ladder.',
    'cargo xtask bench-all',
  ],
  [
    'Wire-size and DNS packet analysis plus an analytical UDP-fit model.',
    'cargo xtask wire-report',
  ],
  [
    'Measured memory compared with a closed-form model.',
    'cargo xtask memory-report',
  ],
  [
    'Ladder shape and condensed paths in the terminal dashboard.',
    'cargo run -p climb-tui',
  ],
  [
    'A recorded capture from patched NSD and Unbound, replayed locally.',
    'cargo xtask dns-demo',
  ],
  [
    'Claims registry, drift checks and a checksummed release bundle.',
    'cargo xtask report --check',
  ],
];

export const sections = [
  'Top',
  'Results',
  'Capabilities',
  'Ladder',
  'How it works',
  'Architecture',
  'Transparency',
  'Team',
];

export const homeData = {
  explorer,
  configs,
  capabilities,
  sections,
  evidence: evidenceLabels,
};
