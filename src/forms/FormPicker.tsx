import { useState } from 'react';

interface FormPickerProps {
  forms: { id: string; name: string }[];
  onPick: (id: string) => void;
  onCancel: () => void;
}

export function FormPicker({ forms, onPick, onCancel }: FormPickerProps) {
  const [selectedId, setSelectedId] = useState<string>(forms[0]?.id ?? '');
  return (
    <div className="form-picker" role="dialog" aria-label="Pick a form">
      <h3>Pick a form to author</h3>
      <ul className="form-picker-list">
        {forms.map((f) => (
          <li
            key={f.id}
            className={selectedId === f.id ? 'selected' : ''}
            onClick={() => setSelectedId(f.id)}
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
          data-testid="form-picker-confirm"
          disabled={!selectedId}
          onClick={() => onPick(selectedId)}
        >
          Use selected form
        </button>
      </div>
    </div>
  );
}
