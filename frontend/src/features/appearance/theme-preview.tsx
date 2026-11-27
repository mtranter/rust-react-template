import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { InfoIcon } from "lucide-react";

/** Real components under the draft theme, in the scheme the app shows. */
export function ThemePreview() {
  return (
    <section aria-label="Preview">
      <Card className="overflow-hidden">
        <CardHeader>
          <CardTitle>Preview</CardTitle>
          <CardDescription>
            The whole app shows your changes until you save or discard.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div
            data-testid="theme-preview"
            className="grid gap-4 rounded-lg border bg-background p-4 text-foreground"
          >
            <div className="flex flex-wrap gap-2">
              <Button size="sm">Primary</Button>
              <Button size="sm" variant="secondary">
                Secondary
              </Button>
              <Button size="sm" variant="outline">
                Outline
              </Button>
              <Button size="sm" variant="destructive">
                Delete
              </Button>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge>Default</Badge>
              <Badge variant="success">Success</Badge>
              <Badge variant="warning">Warning</Badge>
              <Badge variant="info">Info</Badge>
              <Badge variant="highlight">New</Badge>
              <Badge variant="destructive">Failed</Badge>
            </div>
            <Alert variant="info">
              <InfoIcon />
              <AlertTitle>Heads up</AlertTitle>
              <AlertDescription>An alert, as a status message shows.</AlertDescription>
            </Alert>
            <Input aria-label="Sample input" placeholder="An input" />
            <div className="rounded-lg border bg-card p-3 text-card-foreground shadow-md">
              <p className="font-heading font-semibold">A card heading</p>
              <p className="text-muted-foreground text-sm">
                Muted text, and <code className="font-mono">code</code>.
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </section>
  );
}
