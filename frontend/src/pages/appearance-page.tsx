import { Page } from "@/components/page";
import { AppVisibilityList, AppearanceEditor } from "@/features/appearance";
import { registry } from "@/shell/capabilities";

export function AppearancePage() {
  return (
    <Page title="Theme" description="The organisation's theme: its colours, fonts and corners.">
      <AppearanceEditor />
    </Page>
  );
}

export function AppsPage() {
  return (
    <Page
      title="Apps"
      description="Which apps everyone sees. A hidden app's data and API stay as they are."
    >
      <AppVisibilityList apps={registry.apps} />
    </Page>
  );
}
