/**
 * The palette is the single machine-readable copy of the site tokens. The
 * stylesheet carries the same values and a test compares the two, so the two
 * cannot drift silently.
 */
export const palette = {
  ink: '#07080A',
  surface: '#0F1216',
  raised: '#161A20',
  paper: '#F2EDE4',
  text: '#EFEAE1',
  muted: '#9AA3AD',
  orange: '#FF5400',
  red: '#FF1A1A',
  teal: '#5CC8C2',
  slate: '#3A4A66',
  'slate-light': '#6B7A93',
  measured: '#3DDC97',
  derived: '#6EA8FF',
  analytical: '#B79CFF',
  extrapolated: '#F5D547',
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
 * Every foreground/background pair the built site actually paints. A pair that
 * is not listed here must not be introduced without adding it and its target.
 */
export const usedPairs: readonly UsedPair[] = [
  { foreground: 'text', background: 'ink', minimum: TEXT, usage: 'body copy' },
  { foreground: 'text', background: 'surface', minimum: TEXT, usage: 'header and footer copy' },
  { foreground: 'text', background: 'raised', minimum: TEXT, usage: 'card copy' },
  { foreground: 'paper', background: 'ink', minimum: TEXT, usage: 'display headline' },
  { foreground: 'muted', background: 'ink', minimum: TEXT, usage: 'captions and metadata' },
  { foreground: 'muted', background: 'surface', minimum: TEXT, usage: 'header metadata' },
  { foreground: 'muted', background: 'raised', minimum: TEXT, usage: 'card metadata' },
  { foreground: 'orange', background: 'ink', minimum: TEXT, usage: 'eyebrow and inline link' },
  { foreground: 'orange', background: 'surface', minimum: TEXT, usage: 'header link hover' },
  { foreground: 'orange', background: 'raised', minimum: TEXT, usage: 'card link' },
  {
    foreground: 'teal',
    background: 'ink',
    minimum: TEXT,
    usage: 'focus ring and secondary accent',
  },
  { foreground: 'teal', background: 'surface', minimum: TEXT, usage: 'header focus ring' },
  { foreground: 'red', background: 'ink', minimum: TEXT, usage: 'alert accent' },
  { foreground: 'measured', background: 'ink', minimum: TEXT, usage: 'measured chip on the page' },
  {
    foreground: 'measured',
    background: 'surface',
    minimum: TEXT,
    usage: 'measured chip in the header',
  },
  { foreground: 'measured', background: 'raised', minimum: TEXT, usage: 'measured chip on a card' },
  { foreground: 'derived', background: 'ink', minimum: TEXT, usage: 'derived chip on the page' },
  {
    foreground: 'derived',
    background: 'surface',
    minimum: TEXT,
    usage: 'derived chip in the header',
  },
  { foreground: 'derived', background: 'raised', minimum: TEXT, usage: 'derived chip on a card' },
  {
    foreground: 'analytical',
    background: 'ink',
    minimum: TEXT,
    usage: 'analytical chip on the page',
  },
  {
    foreground: 'analytical',
    background: 'surface',
    minimum: TEXT,
    usage: 'analytical chip in the header',
  },
  {
    foreground: 'analytical',
    background: 'raised',
    minimum: TEXT,
    usage: 'analytical chip on a card',
  },
  {
    foreground: 'extrapolated',
    background: 'ink',
    minimum: TEXT,
    usage: 'extrapolated chip on the page',
  },
  {
    foreground: 'extrapolated',
    background: 'surface',
    minimum: TEXT,
    usage: 'extrapolated chip in the header',
  },
  {
    foreground: 'extrapolated',
    background: 'raised',
    minimum: TEXT,
    usage: 'extrapolated chip on a card',
  },
  { foreground: 'ink', background: 'orange', minimum: TEXT, usage: 'label on the orange button' },
  { foreground: 'ink', background: 'teal', minimum: TEXT, usage: 'label on a teal fill' },
  { foreground: 'ink', background: 'measured', minimum: TEXT, usage: 'label on a measured fill' },
  { foreground: 'ink', background: 'derived', minimum: TEXT, usage: 'label on a derived fill' },
  {
    foreground: 'ink',
    background: 'analytical',
    minimum: TEXT,
    usage: 'label on an analytical fill',
  },
  {
    foreground: 'ink',
    background: 'extrapolated',
    minimum: TEXT,
    usage: 'label on an extrapolated fill',
  },
  {
    foreground: 'slate-light',
    background: 'ink',
    minimum: BOUNDARY,
    usage: 'interactive boundary on the page',
  },
  {
    foreground: 'slate-light',
    background: 'surface',
    minimum: BOUNDARY,
    usage: 'interactive boundary in the header',
  },
  {
    foreground: 'slate-light',
    background: 'raised',
    minimum: BOUNDARY,
    usage: 'interactive boundary on a card',
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
