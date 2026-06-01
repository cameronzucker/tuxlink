// src/radio/sections/SignalSection.test.tsx
//
// Spec §5.3 — Signal section. Composes the Quality big-number indicator,
// the S/N trend Sparkline, and the recent-frame FrameRibbon. Quality is
// shown as `—` when null (no PINGACK observed yet); the S/N "now" value
// formats with sign + 1 decimal place.

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { SignalSection } from './SignalSection';
import type { ArdopFrameType } from '../charts/FrameRibbon';

const FRAMES: ArdopFrameType[] = ['CON', 'IDLE', 'DATA', 'ACK'];

describe('<SignalSection>', () => {
  it('renders Quality value when not null', () => {
    render(
      <SignalSection
        quality={87}
        snrSamples={[1, 2, 3]}
        recentFrames={FRAMES}
        snrCurrent={5.4}
      />,
    );
    const score = screen.getByTestId('quality-score');
    expect(score).toHaveTextContent('87');
    expect(score).toHaveTextContent(/Quality/i);
    expect(score).toHaveTextContent('/100');
  });

  it('renders Quality em-dash when null (no PINGACK yet)', () => {
    render(
      <SignalSection
        quality={null}
        snrSamples={[]}
        recentFrames={[]}
        snrCurrent={null}
      />,
    );
    const score = screen.getByTestId('quality-score');
    expect(score).toHaveTextContent('—');
  });

  it('renders the S/N current value with sign and 1 decimal place', () => {
    render(
      <SignalSection
        quality={50}
        snrSamples={[1, 2, 3]}
        recentFrames={FRAMES}
        snrCurrent={5.4}
      />,
    );
    expect(screen.getByText(/\+5\.4 dB/)).toBeInTheDocument();
  });

  it('renders the S/N current value with negative sign formatted correctly', () => {
    render(
      <SignalSection
        quality={20}
        snrSamples={[-1, -2]}
        recentFrames={FRAMES}
        snrCurrent={-3.5}
      />,
    );
    // toFixed always emits the leading '-' on negatives, so don't prepend '+'.
    expect(screen.getByText(/-3\.5 dB/)).toBeInTheDocument();
  });

  it('renders the S/N current as em-dash when null', () => {
    render(
      <SignalSection
        quality={null}
        snrSamples={[]}
        recentFrames={[]}
        snrCurrent={null}
      />,
    );
    expect(screen.getByText(/— dB/)).toBeInTheDocument();
  });

  it('mounts the Sparkline child (S/N trend)', () => {
    render(
      <SignalSection
        quality={70}
        snrSamples={[1, 2, 3, 4]}
        recentFrames={FRAMES}
        snrCurrent={3.1}
      />,
    );
    const sparkline = screen.getByTestId('sparkline');
    expect(sparkline.children).toHaveLength(4);
  });

  it('mounts the FrameRibbon child with recent frames', () => {
    render(
      <SignalSection
        quality={70}
        snrSamples={[1, 2, 3]}
        recentFrames={FRAMES}
        snrCurrent={3.1}
      />,
    );
    const ribbon = screen.getByTestId('frame-ribbon');
    expect(ribbon.children).toHaveLength(4);
  });

  it('renders an avg label computed from the samples buffer', () => {
    render(
      <SignalSection
        quality={70}
        snrSamples={[2, 4, 6]} // avg = 4.0
        recentFrames={FRAMES}
        snrCurrent={5}
      />,
    );
    // Avg line includes "avg +4.0 dB".
    expect(screen.getByText(/\+4\.0 dB/)).toBeInTheDocument();
  });

  it('renders avg em-dash when samples buffer is empty', () => {
    render(
      <SignalSection
        quality={null}
        snrSamples={[]}
        recentFrames={[]}
        snrCurrent={null}
      />,
    );
    // The "avg —" leaf — exact substring not asserted to allow whitespace flex.
    expect(screen.getByText(/avg/)).toHaveTextContent('—');
  });
});
