import { observer } from "mobx-react-lite";
import { useStore } from "../../stores/useStore";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import {
  CLI_CONSENT_ALLOW,
  CLI_CONSENT_DECLINE,
  CLI_CONSENT_TITLE,
  cliChangeText,
  cliSourceText,
} from "./cliConsentCopy";

function CliConsentModal() {
  const { cliAccess } = useStore();
  const consent = cliAccess.current;
  return (
    <ConfirmModal
      open={consent !== null}
      title={CLI_CONSENT_TITLE}
      tone={consent?.recognised === false ? "danger" : "primary"}
      confirmLabel={CLI_CONSENT_ALLOW}
      cancelLabel={CLI_CONSENT_DECLINE}
      disabled={cliAccess.answering}
      content={
        consent === null ? null : (
          <>
            <p className="m-0 text-main-200">{cliChangeText(consent.change)}</p>
            <p className="m-0 mt-2">{cliSourceText(consent)}</p>
            {cliAccess.consentError === null ? null : (
              <p role="alert" className="m-0 mt-2 text-err">
                {cliAccess.consentError}
              </p>
            )}
          </>
        )
      }
      onConfirm={() => void cliAccess.answer(true)}
      onCancel={() => void cliAccess.answer(false)}
    />
  );
}

export default observer(CliConsentModal);
