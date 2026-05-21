import { useWizard } from './wizardContext';

export function Step1Welcome() {
  const { dispatch } = useWizard();

  function choose(connectToCms: boolean) {
    dispatch({ type: 'SET_CONNECT_TO_CMS', payload: connectToCms });
    dispatch({ type: 'ADVANCE_FROM_ACCOUNT' });
  }

  return (
    <div className="wizard-step wizard-step-account">
      <h1>Will this installation connect to the Winlink CMS?</h1>
      <p>
        Your choice determines whether tuxlink uses internet-backed CMS
        authentication (most operators) or runs offline (radio-only / drills /
        lab work). You can change this later in Tools → Settings → Connection.
      </p>
      <div className="wizard-choice-cards">
        <button type="button" className="wizard-choice-card" onClick={() => choose(true)} autoFocus>
          <strong>Yes, connect to the Winlink CMS</strong>
          <p>
            Default. Uses the internet-backed CMS for authentication. You'll
            enter your callsign and CMS password next.
          </p>
        </button>
        <button type="button" className="wizard-choice-card" onClick={() => choose(false)}>
          <strong>No, this is an offline / radio-only deployment</strong>
          <p>
            For Winlink Hybrid Network operators, ARES drills, EOC tabletops,
            and lab work. No CMS connection attempts.
          </p>
        </button>
      </div>
    </div>
  );
}
