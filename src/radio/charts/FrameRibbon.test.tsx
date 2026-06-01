// src/radio/charts/FrameRibbon.test.tsx
//
// Spec §5.3 — horizontal ribbon of recent ARQ frame types. Each cell
// renders the frame token with a color class tied to its type
// (CON / IDLE / DATA / ACK / NAK / REJ). Used by the Signal section
// to give the operator a quick visual read on the recent on-air
// subprotocol traffic.

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { FrameRibbon, type ArdopFrameType } from './FrameRibbon';

describe('<FrameRibbon>', () => {
  it('renders cells in order oldest → newest', () => {
    const frames: ArdopFrameType[] = ['CON', 'IDLE', 'DATA', 'ACK'];
    render(<FrameRibbon frames={frames} showLegend={false} />);
    const ribbon = screen.getByTestId('frame-ribbon');
    expect(ribbon.children).toHaveLength(4);
    expect(ribbon.children[0]).toHaveTextContent('CON');
    expect(ribbon.children[1]).toHaveTextContent('IDLE');
    expect(ribbon.children[2]).toHaveTextContent('DATA');
    expect(ribbon.children[3]).toHaveTextContent('ACK');
  });

  it('applies the per-type color class', () => {
    const frames: ArdopFrameType[] = ['CON', 'DATA', 'NAK'];
    render(<FrameRibbon frames={frames} showLegend={false} />);
    const ribbon = screen.getByTestId('frame-ribbon');
    expect(ribbon.children[0].className).toMatch(/frame-con/);
    expect(ribbon.children[1].className).toMatch(/frame-data/);
    expect(ribbon.children[2].className).toMatch(/frame-nak/);
  });

  it('truncates to the most-recent 14 cells when over the limit', () => {
    // 20 frames → only the last 14 render.
    const frames: ArdopFrameType[] = Array(20).fill('DATA' as ArdopFrameType);
    render(<FrameRibbon frames={frames} showLegend={false} />);
    const ribbon = screen.getByTestId('frame-ribbon');
    expect(ribbon.children).toHaveLength(14);
  });

  it('renders the legend by default with all six frame types', () => {
    render(<FrameRibbon frames={[]} />);
    // Legend renders one span per frame type with the token text.
    expect(screen.getByText('CON')).toBeInTheDocument();
    expect(screen.getByText('IDLE')).toBeInTheDocument();
    expect(screen.getByText('DATA')).toBeInTheDocument();
    expect(screen.getByText('ACK')).toBeInTheDocument();
    expect(screen.getByText('NAK')).toBeInTheDocument();
    expect(screen.getByText('REJ')).toBeInTheDocument();
  });

  it('omits the legend when showLegend=false', () => {
    render(<FrameRibbon frames={[]} showLegend={false} />);
    expect(screen.queryByText('CON')).not.toBeInTheDocument();
  });

  it('renders an empty ribbon container when frames is empty', () => {
    render(<FrameRibbon frames={[]} showLegend={false} />);
    const ribbon = screen.getByTestId('frame-ribbon');
    expect(ribbon.children).toHaveLength(0);
  });
});
