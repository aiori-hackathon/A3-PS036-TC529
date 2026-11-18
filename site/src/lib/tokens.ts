/**
 * The palette is the single machine-readable copy of the site tokens. The
 * stylesheet carries the same values and a test compares the two, so the two
 * cannot drift silently.
 */
export const palette = {
  ink: '#07080A',
  ground: '#13161B',
  surface: '#1B1E25',
  raised: '#0D0F13',
  paper: '#F2EDE4',
  text: '#F2EDE4',
  muted: '#A9A398',
  orange: '#FF5400',
  'orange-bright': '#FE7F00',
  rose: '#FFA6A6',
  // The hairlines as they composite over the ground; the stylesheet keeps the
  // translucent originals, and the Python contrast check samples those stops.
  slate: '#2F3135',
  'slate-light': '#7C7A75',
  measured: '#F2EDE4',
  derived: '#FFA6A6',
  analytical: '#A9A398',
  extrapolated: '#FE7F00',
} as const;

export type TokenName = keyof typeof palette;

export interface UsedPair {
  foreground: TokenName;
  background: TokenName;
  minimum: number;
  usage: string;
}

const TEXT = 4.5;
const BOUNDARY = 3;

/**
 * Every foreground/background pair the built pages paint. A pair that is not
 * listed here must not be introduced without adding it and its target.
 */
export const usedPairs: readonly UsedPair[] = [
  { foreground: 'text', background: 'ink', minimum: TEXT, usage: 'body copy' },
  { foreground: 'text', background: 'ground', minimum: TEXT, usage: 'body copy on the slate' },
  { foreground: 'text', background: 'surface', minimum: TEXT, usage: 'header and footer copy' },
  { foreground: 'text', background: 'raised', minimum: TEXT, usage: 'card copy' },
  { foreground: 'paper', background: 'ink', minimum: TEXT, usage: 'display headline' },
  { foreground: 'muted', background: 'ink', minimum: TEXT, usage: 'captions and metadata' },
  { foreground: 'muted', background: 'ground', minimum: TEXT, usage: 'captions on the slate' },
  { foreground: 'muted', background: 'surface', minimum: TEXT, usage: 'header metadata' },
  { foreground: 'muted', background: 'raised', minimum: TEXT, usage: 'card metadata' },
  { foreground: 'orange', background: 'ink', minimum: TEXT, usage: 'accent and inline link' },
  { foreground: 'orange', background: 'ground', minimum: TEXT, usage: 'accent on the slate' },
  { foreground: 'orange', background: 'surface', minimum: TEXT, usage: 'header accent' },
  { foreground: 'orange-bright', background: 'ink', minimum: TEXT, usage: 'focus ring' },
  { foreground: 'orange-bright', background: 'ground', minimum: TEXT, usage: 'hover accent' },
  { foreground: 'rose', background: 'ink', minimum: TEXT, usage: 'derived mark' },
  { foreground: 'rose', background: 'ground', minimum: TEXT, usage: 'derived mark on the slate' },
  { foreground: 'ink', background: 'orange', minimum: TEXT, usage: 'label on the orange button' },
  { foreground: 'ink', background: 'orange-bright', minimum: TEXT, usage: 'label on the hover button' },
  { foreground: 'ink', background: 'measured', minimum: TEXT, usage: 'label on a measured fill' },
  { foreground: 'ink', background: 'derived', minimum: TEXT, usage: 'label on a derived fill' },
  { foreground: 'ink', background: 'analytical', minimum: TEXT, usage: 'label on an analytical fill' },
  {
    foreground: 'ink',
    background: 'extrapolated',
    minimum: TEXT,
    usage: 'label on an extrapolated fill',
  },
  { foreground: 'slate-light', background: 'ink', minimum: BOUNDARY, usage: 'control boundary' },
  {
    foreground: 'slate-light',
    background: 'ground',
    minimum: BOUNDARY,
    usage: 'control boundary on the slate',
  },
  {
    foreground: 'slate-light',
    background: 'raised',
    minimum: BOUNDARY,
    usage: 'control boundary in a well',
  },
];

function channel(value: number): number {
  const normalized = value / 255;
  return normalized <= 0.04045 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance(hex: string): number {
  const value = hex.replace('#', '');
  const red = Number.parseInt(value.slice(0, 2), 16);
  const green = Number.parseInt(value.slice(2, 4), 16);
  const blue = Number.parseInt(value.slice(4, 6), 16);
  return 0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue);
}

export function contrastRatio(foreground: string, background: string): number {
  const first = relativeLuminance(foreground);
  const second = relativeLuminance(background);
  const high = Math.max(first, second);
  const low = Math.min(first, second);
  return (high + 0.05) / (low + 0.05);
}
