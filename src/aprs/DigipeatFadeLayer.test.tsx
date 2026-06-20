import { describe, it, expect } from 'vitest';
import { render, act } from '@testing-library/react';
import { getLastMap, type MapLibreMock } from '../map/testMapLibreMock';
import { AprsPositionsMap } from './AprsPositionsMap';
import type { HeardPosition } from './aprsTypes';

// The fade layer renders + animates on the software-GL build — that's grim-only.
// These prove the WIRING through the maplibre test double: the pool layers exist
// and a hover pushes the honest resolved geometry into a slot source.

function loadLast(): MapLibreMock {
  const map = getLastMap()!;
  act(() => map.__emit('load'));
  return map;
}

function hover(map: MapLibreMock, call: string) {
  act(() =>
    map.__emit('mouseenter:aprs-position-pins-color', { features: [{ properties: { call } }] }),
  );
}

function anySlotHasGeometry(map: MapLibreMock): boolean {
  for (let i = 0; i < 6; i++) {
    const src = map.getSource(`aprs-trace-slot-${i}`) as { data?: { features?: unknown[] } } | undefined;
    if ((src?.data?.features?.length ?? 0) > 0) return true;
  }
  return false;
}

const viaStation: HeardPosition[] = [
  {
    call: 'W7WX',
    lat: 48.1,
    lon: -122.6,
    symbolTable: '/',
    symbolCode: '>',
    comment: '',
    at: 1,
    ambiguity: 0,
    via: [{ call: 'W7RPT-1', repeated: true }],
  },
];

describe('DigipeatFadeLayer', () => {
  it('registers the fade pool layers', () => {
    render(<AprsPositionsMap positions={[]} operatorGrid="CN87" />);
    const map = loadLast();
    const ids = map.__state.layers.map((l) => l.id);
    expect(ids).toContain('aprs-trace-slot-0-solid');
    expect(ids).toContain('aprs-trace-slot-0-dashed');
    expect(ids).toContain('aprs-trace-slot-5-solid');
  });

  it('pushes the resolved path geometry to a slot on hover', () => {
    render(<AprsPositionsMap positions={viaStation} operatorGrid="CN87" />);
    const map = loadLast();
    hover(map, 'W7WX');
    expect(anySlotHasGeometry(map)).toBe(true);
  });

  it('emits a pos? label for the unlocatable hop (W7RPT-1 is not located)', () => {
    render(<AprsPositionsMap positions={viaStation} operatorGrid="CN87" />);
    const map = loadLast();
    hover(map, 'W7WX');
    let label = '';
    for (let i = 0; i < 6; i++) {
      const src = map.getSource(`aprs-trace-slot-${i}-labels`) as
        | { data?: { features?: Array<{ properties?: { label?: string } }> } }
        | undefined;
      const f = src?.data?.features?.[0];
      if (f?.properties?.label) label = f.properties.label;
    }
    expect(label).toContain('W7RPT-1');
    expect(label).toContain('?');
  });

  it('does not trace from an object/item pin (honest RF source)', () => {
    const obj: HeardPosition[] = [{ ...viaStation[0], call: 'LEADER', isObject: true }];
    render(<AprsPositionsMap positions={obj} operatorGrid="CN87" />);
    const map = loadLast();
    hover(map, 'LEADER');
    expect(anySlotHasGeometry(map)).toBe(false);
  });
});
