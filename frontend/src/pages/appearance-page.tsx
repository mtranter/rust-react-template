import { Page } from "@/components/page";
import { AppearanceEditor } from "@/features/appearance";

export function AppearancePage() {
  return (
    <Page
      title="Appearance"
      description="The organisation's theme: its colours, fonts and corners."
    >
      <AppearanceEditor />
    </Page>
  );
}
