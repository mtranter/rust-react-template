import { Page } from "@/components/page";
import { AppVisibilityList, ThemeEditor } from "@/features/ui-settings";
import { registry } from "@/shell/capabilities";

export function ThemingPage() {
  return (
    <Page title="Theming" description="The organisation's theme: its colours, fonts and corners.">
      <ThemeEditor />
    </Page>
  );
}

export function AppVisibilityPage() {
  return (
    <Page
      title="App visibility"
      description="Which apps everyone sees. A hidden app's data and API stay as they are."
    >
      <AppVisibilityList apps={registry.apps} />
    </Page>
  );
}
