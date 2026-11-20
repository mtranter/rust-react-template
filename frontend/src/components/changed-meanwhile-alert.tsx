import { Alert, AlertDescription } from "@/components/ui/alert";
import { TriangleAlertIcon } from "lucide-react";

type Retry = "save" | "delete" | "send";

/** What to tell the user after a `412 precondition_failed`, once `subject` has been refetched. */
export function changedMeanwhileMessage(retry: Retry, subject: string) {
  return `Someone else changed this ${subject}. It's been reloaded: check it and ${retry} again.`;
}

/** After a `412 precondition_failed`, once `subject` (`"note"`) has been refetched. */
export function ChangedMeanwhileAlert({ retry, subject }: { retry: Retry; subject: string }) {
  return (
    <Alert>
      <TriangleAlertIcon />
      <AlertDescription>{changedMeanwhileMessage(retry, subject)}</AlertDescription>
    </Alert>
  );
}
