import type { ProposalFieldResponse, ProposalResponse, ToolCallResponse } from "@/api/types.gen";
import { Button } from "@/components/ui/button";
import { useState } from "react";

/** `edit_note` as "Edit note", as the server words a proposal's action. */
export function sentenceCase(name: string) {
  const words = name
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[_\-\s]+/)
    .filter(Boolean)
    .map((word) => word.toLowerCase());
  const text = words.join(" ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/** What a call does, in words: its proposal's action and target, else its operation's name. */
export function actionOf(call: Pick<ToolCallResponse, "tool" | "proposal">) {
  const proposal = call.proposal;
  if (!proposal) return sentenceCase(call.tool);
  return proposal.target ? `${proposal.action}: ${proposal.target.name}` : proposal.action;
}

const LONG = 160;

/** The business data a pending write sends, as a label/value list, a changed value after its old. */
export function ProposalFields({ proposal }: { proposal: ProposalResponse }) {
  if (proposal.fields.length === 0) return null;
  return (
    <dl className="grid grid-cols-[minmax(6rem,auto)_1fr] gap-x-4 gap-y-1 text-sm">
      {proposal.fields.map((field, at) => (
        // Labels may repeat (a list's items), and the fields never reorder.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        <ProposalField key={at} field={field} />
      ))}
    </dl>
  );
}

function ProposalField({ field }: { field: ProposalFieldResponse }) {
  const value = shown(field.value, field.format);
  const changed = field.previous !== undefined;
  return (
    <>
      <dt className="text-muted-foreground">{field.label}</dt>
      <dd className="min-w-0 break-words">
        {changed && (
          <>
            <del className="text-muted-foreground">{shown(field.previous, field.format)}</del>
            <span aria-hidden="true"> → </span>
          </>
        )}
        <Clamped text={value} />
      </dd>
    </>
  );
}

/** `text`, clamped to a few lines when long, with a button to show it all. */
function Clamped({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const long = text.length > LONG || text.split("\n").length > 3;
  if (!long) return <span className="whitespace-pre-wrap">{text}</span>;
  return (
    <span className="block">
      <span className={`block whitespace-pre-wrap ${open ? "" : "line-clamp-3"}`}>{text}</span>
      <Button variant="link" size="sm" className="h-auto p-0" onClick={() => setOpen(!open)}>
        {open ? "Show less" : "Show more"}
      </Button>
    </span>
  );
}

/** A value as the user reads it: dates in their locale, never JSON. */
export function shown(value: unknown, format?: string): string {
  if (value === null || value === undefined || value === "") return "None";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (Array.isArray(value)) {
    return value.length === 0 ? "None" : value.map((item) => shown(item, format)).join(", ");
  }
  if (typeof value === "string") return formatted(value, format) ?? value;
  if (typeof value === "number") return value.toLocaleString();
  if (typeof value === "object") {
    return Object.entries(value)
      .map(([name, inner]) => `${sentenceCase(name)}: ${shown(inner)}`)
      .join(", ");
  }
  return String(value);
}

function formatted(value: string, format?: string) {
  if (format === "date") {
    const [year, month, day] = value.split("-").map(Number);
    // A calendar day, not an instant: read in the user's own time zone so it doesn't shift.
    const date = new Date(year, month - 1, day);
    return Number.isNaN(date.getTime())
      ? undefined
      : new Intl.DateTimeFormat(undefined, { dateStyle: "long" }).format(date);
  }
  if (format === "date-time") {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? undefined
      : new Intl.DateTimeFormat(undefined, { dateStyle: "long", timeStyle: "short" }).format(date);
  }
  return undefined;
}
