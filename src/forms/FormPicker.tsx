import { useEffect, useRef, useState } from 'react';
import './FormPicker.css';

interface FormPickerProps {
  forms: { id: string; name: string }[];
  onPick: (id: string) => void;
  onCancel: () => void;
}

export function FormPicker({ forms, onPick, onCancel }: FormPickerProps) {
  const [selectedId, setSelectedId] = useState<string>(forms[0]?.id ?? '');
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    listRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent<HTMLUListElement>) => {
    if (forms.length === 0) {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      }
      return;
    }
    const idx = forms.findIndex((f) => f.id === selectedId);
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedId(forms[Math.min(idx + 1, forms.length - 1)].id);
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedId(forms[Math.max(idx - 1, 0)].id);
        break;
      case 'Home':
        e.preventDefault();
        setSelectedId(forms[0].id);
        break;
      case 'End':
        e.preventDefault();
        setSelectedId(forms[forms.length - 1].id);
        break;
      case 'Enter':
      case ' ':
        e.preventDefault();
        if (selectedId) onPick(selectedId);
        break;
      case 'Escape':
        e.preventDefault();
        onCancel();
        break;
    }
  };

  return (
    <div className="form-picker" role="dialog" aria-modal="true" aria-label="Pick a form">
      <div className="form-picker__card">
        <h3 id="form-picker-title">Pick a form to author</h3>
        <ul
          ref={listRef}
          className="form-picker-list"
          role="listbox"
          aria-labelledby="form-picker-title"
          tabIndex={0}
          aria-activedescendant={selectedId ? `form-picker-opt-${selectedId}` : undefined}
          onKeyDown={handleKeyDown}
        >
          {forms.map((f) => (
            <li
              key={f.id}
              id={`form-picker-opt-${f.id}`}
              role="option"
              aria-selected={selectedId === f.id}
              className={selectedId === f.id ? 'selected' : ''}
              onClick={() => setSelectedId(f.id)}
              onDoubleClick={() => onPick(f.id)}
            >
              {f.name}
            </li>
          ))}
        </ul>
        <div className="form-picker-actions">
          <button type="button" data-testid="form-picker-cancel" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="form-picker-actions__primary"
            data-testid="form-picker-confirm"
            disabled={!selectedId}
            onClick={() => onPick(selectedId)}
          >
            Use selected form
          </button>
        </div>
      </div>
    </div>
  );
}
