/**
 * Typed access to the generated fact register.
 *
 * `facts.json` is written at build time from the recorded artifacts. A page
 * must reach a number through `getFact`; the call throws when the id is
 * missing, so a stale id fails `astro build` instead of rendering a blank.
 *
 * The formatter reproduces the conventions shared with the terminal
 * dashboard: thousands separators, four significant digits for a ratio,
 * three decimals for a percentage, and the approximation mark on an
 * extrapolated value.
 */

import factsFile from '../data/generated/facts.json';

export type FactClass =
  'measured' | 'derived' | 'analytical' | 'extrapolated' | 'constant' | 'generated';

export type FactKind = 'ratio' | 'percent' | 'count' | 'text' | 'count_of';

export interface Fact {
  readonly id: string;
  readonly display: string;
  readonly unit: string;
  readonly class: FactClass;
  readonly baseline: string;
  readonly pointer: string;
  readonly source: string;
  readonly kind: FactKind;
  readonly raw: number | string | null;
  readonly denominator: number | null;
}

export class UnknownFactError extends Error {
  constructor(id: string) {
    super(`unknown fact id \`${id}\`; regenerate site/src/data/generated/facts.json`);
    this.name = 'UnknownFactError';
  }
}

const facts = [...(factsFile as { facts: Fact[] }).facts];
const byId = new Map(facts.map((fact) => [fact.id, fact]));

/** Every fact in register order. */
export function allFacts(): readonly Fact[] {
  return facts;
}

/** Look up one fact, throwing at build time when the id is not registered. */
export function getFact(id: string): Fact {
  const fact = byId.get(id);
  if (fact === undefined) {
    throw new UnknownFactError(id);
  }
  return fact;
}

/** Group the integer part of a decimal string with commas. */
export function groupIntegerPart(text: string): string {
  if (text.includes('e') || text.includes('E')) {
    return text;
  }
  const negative = text.startsWith('-');
  const rest = negative ? text.slice(1) : text;
  const [integer = '', fraction] = rest.split('.');
  const grouped = integer.replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  const body = fraction === undefined ? grouped : `${grouped}.${fraction}`;
  return negative ? `-${body}` : body;
}

/** Four significant digits, trimming trailing zeros. */
export function formatRatio(value: number): string {
  if (!Number.isFinite(value)) {
    return '—';
  }
  if (value === 0) {
    return '0';
  }
  const magnitude = Math.floor(Math.log10(Math.abs(value)));
  const decimals = Math.min(17, Math.max(0, 3 - magnitude));
  let text = value.toFixed(decimals);
  if (text.includes('.')) {
    text = text.replace(/0+$/, '').replace(/\.$/, '');
  }
  if (text === '0' || text === '-0') {
    return value.toExponential(3);
  }
  return groupIntegerPart(text);
}

/** Three decimals, the precision the quote sheet records. */
export function formatPercent(value: number): string {
  if (!Number.isFinite(value)) {
    return '—';
  }
  return value.toFixed(3);
}

/** A whole count with thousands separators. */
export function formatCount(value: number): string {
  if (!Number.isFinite(value)) {
    return '—';
  }
  return groupIntegerPart(String(Math.round(value)));
}

function approximation(className: FactClass, text: string): string {
  return className === 'extrapolated' ? `≈${text}` : text;
}

/** Render a fact's value from its raw number using the shared conventions. */
export function formatFactValue(
  fact: Pick<Fact, 'kind' | 'raw' | 'denominator' | 'class'>,
): string {
  let text: string;
  switch (fact.kind) {
    case 'ratio':
      text = typeof fact.raw === 'number' ? formatRatio(fact.raw) : '—';
      break;
    case 'percent':
      text = typeof fact.raw === 'number' ? formatPercent(fact.raw) : '—';
      break;
    case 'count':
      text = typeof fact.raw === 'number' ? formatCount(fact.raw) : '—';
      break;
    case 'count_of':
      text =
        typeof fact.raw === 'number' && fact.denominator !== null
          ? `${formatCount(fact.raw)} of ${formatCount(fact.denominator)}`
          : '—';
      break;
    case 'text':
      text = fact.raw === null ? '—' : String(fact.raw);
      break;
    default:
      text = '—';
  }
  return approximation(fact.class, text);
}
