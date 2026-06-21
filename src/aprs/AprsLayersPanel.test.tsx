import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { AprsLayersPanel } from './AprsLayersPanel';
import { ALL_BUCKET_KEYS, emptyCounts } from './stationBuckets';

const baseProps = {
  enabled: new Set(ALL_BUCKET_KEYS),
  counts: { ...emptyCounts(), weather: 7, vehicles: 6, digipeater: 4, igate: 3, fixed: 2, other: 1 },
  total: 23,
  collapsed: false,
  onToggleBucket: () => {},
  onToggleAll: () => {},
  onToggleCollapsed: () => {},
};

describe('AprsLayersPanel', () => {
  it('collapsed: shows only the toggle button, not the panel', () => {
    render(<AprsLayersPanel {...baseProps} collapsed />);
    expect(screen.getByTestId('aprs-layers-toggle')).toBeInTheDocument();
    expect(screen.queryByTestId('aprs-layers-panel')).not.toBeInTheDocument();
  });

  it('expanded: lists all 8 buckets with live counts and the total', () => {
    render(<AprsLayersPanel {...baseProps} />);
    expect(screen.getByTestId('aprs-layers-panel')).toBeInTheDocument();
    for (const key of ALL_BUCKET_KEYS) {
      expect(screen.getByTestId(`aprs-layers-row-${key}`)).toBeInTheDocument();
    }
    expect(screen.getByTestId('aprs-layers-count-weather')).toHaveTextContent('7');
    expect(screen.getByTestId('aprs-layers-all')).toBeInTheDocument();
  });

  it('clicking a bucket checkbox calls onToggleBucket with its key', () => {
    const onToggleBucket = vi.fn();
    render(<AprsLayersPanel {...baseProps} onToggleBucket={onToggleBucket} />);
    fireEvent.click(screen.getByTestId('aprs-layers-check-weather'));
    expect(onToggleBucket).toHaveBeenCalledWith('weather');
  });

  it('a disabled bucket renders its checkbox unchecked', () => {
    const enabled = new Set(ALL_BUCKET_KEYS.filter((k) => k !== 'weather'));
    render(<AprsLayersPanel {...baseProps} enabled={enabled} />);
    expect(screen.getByTestId('aprs-layers-check-weather')).not.toBeChecked();
    expect(screen.getByTestId('aprs-layers-check-vehicles')).toBeChecked();
  });

  it('master "All" is checked when all on; clicking it calls onToggleAll(false)', () => {
    const onToggleAll = vi.fn();
    render(<AprsLayersPanel {...baseProps} onToggleAll={onToggleAll} />);
    expect(screen.getByTestId('aprs-layers-all')).toBeChecked();
    fireEvent.click(screen.getByTestId('aprs-layers-all'));
    expect(onToggleAll).toHaveBeenCalledWith(false);
  });

  it('master "All" unchecked (some off) → clicking calls onToggleAll(true)', () => {
    const onToggleAll = vi.fn();
    const enabled = new Set(ALL_BUCKET_KEYS.filter((k) => k !== 'weather'));
    render(<AprsLayersPanel {...baseProps} enabled={enabled} onToggleAll={onToggleAll} />);
    expect(screen.getByTestId('aprs-layers-all')).not.toBeChecked();
    fireEvent.click(screen.getByTestId('aprs-layers-all'));
    expect(onToggleAll).toHaveBeenCalledWith(true);
  });

  it('toggle button and collapse control call onToggleCollapsed', () => {
    const onToggleCollapsed = vi.fn();
    const { rerender } = render(
      <AprsLayersPanel {...baseProps} collapsed onToggleCollapsed={onToggleCollapsed} />,
    );
    fireEvent.click(screen.getByTestId('aprs-layers-toggle'));
    rerender(<AprsLayersPanel {...baseProps} onToggleCollapsed={onToggleCollapsed} />);
    fireEvent.click(screen.getByTestId('aprs-layers-collapse'));
    expect(onToggleCollapsed).toHaveBeenCalledTimes(2);
  });
});
