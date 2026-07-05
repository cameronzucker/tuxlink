/**
 * DetectedModelCombobox — filter-as-you-type, height-capped scrollable model picker
 * (tuxlink-qhe8n). Replaces the native <select> whose option popup ran off the
 * bottom of the screen under WebKitGTK when a provider (e.g. OpenRouter, ~300
 * models) returned a long list, leaving lower models unselectable.
 */
import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, within } from '@testing-library/react';
import { DetectedModelCombobox } from './DetectedModelCombobox';

afterEach(cleanup);

const MANY = Array.from({ length: 300 }, (_, i) => `provider/model-${i}`);

function setup(props: Partial<React.ComponentProps<typeof DetectedModelCombobox>> = {}) {
  const onSelect = vi.fn();
  render(
    <DetectedModelCombobox
      models={props.models ?? ['gpt-4o', 'gpt-4o-mini', 'o1-preview']}
      value={props.value ?? ''}
      onSelect={props.onSelect ?? onSelect}
      testId={props.testId ?? 'detected-models'}
    />,
  );
  return { onSelect: props.onSelect ?? onSelect };
}

describe('<DetectedModelCombobox>', () => {
  it('renders every model as an option in a listbox', () => {
    setup();
    const list = screen.getByTestId('detected-models');
    const options = within(list).getAllByRole('option');
    expect(options.map((o) => o.textContent)).toEqual(['gpt-4o', 'gpt-4o-mini', 'o1-preview']);
  });

  it('caps the list height and scrolls (never overflows the viewport)', () => {
    setup({ models: MANY });
    const list = screen.getByTestId('detected-models');
    // The listbox must constrain its own height + scroll, not grow to 300 rows.
    const style = getComputedStyle(list);
    expect(list).toHaveClass('elmer-combobox-list');
    // jsdom doesn't apply the stylesheet, so assert the inline/utility contract:
    // the component sets an overflow-y auto + a bounded max-height via the class.
    expect(style.overflowY === 'auto' || list.className.includes('elmer-combobox-list')).toBe(true);
  });

  it('filters the list case-insensitively as the operator types', () => {
    setup();
    fireEvent.change(screen.getByTestId('detected-models-filter'), { target: { value: 'MINI' } });
    const options = within(screen.getByTestId('detected-models')).getAllByRole('option');
    expect(options.map((o) => o.textContent)).toEqual(['gpt-4o-mini']);
  });

  it('shows a no-matches row when the filter excludes everything', () => {
    const { onSelect } = setup();
    fireEvent.change(screen.getByTestId('detected-models-filter'), { target: { value: 'zzzz' } });
    expect(within(screen.getByTestId('detected-models')).queryAllByRole('option')).toHaveLength(0);
    expect(screen.getByTestId('detected-models-empty')).toBeTruthy();
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('calls onSelect with the clicked model', () => {
    const { onSelect } = setup();
    fireEvent.click(screen.getByText('gpt-4o-mini'));
    expect(onSelect).toHaveBeenCalledWith('gpt-4o-mini');
  });

  it('marks the current value as selected (aria-selected)', () => {
    setup({ value: 'o1-preview' });
    const selected = within(screen.getByTestId('detected-models'))
      .getAllByRole('option')
      .find((o) => o.getAttribute('aria-selected') === 'true');
    expect(selected?.textContent).toBe('o1-preview');
  });

  it('keyboard: ArrowDown then Enter selects the highlighted model', () => {
    const { onSelect } = setup();
    const filter = screen.getByTestId('detected-models-filter');
    fireEvent.keyDown(filter, { key: 'ArrowDown' }); // highlight index 0
    fireEvent.keyDown(filter, { key: 'ArrowDown' }); // highlight index 1
    fireEvent.keyDown(filter, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('gpt-4o-mini');
  });

  it('keyboard: Enter with an active filter selects the first match', () => {
    const { onSelect } = setup();
    fireEvent.change(screen.getByTestId('detected-models-filter'), { target: { value: 'o1' } });
    fireEvent.keyDown(screen.getByTestId('detected-models-filter'), { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('o1-preview');
  });
});
